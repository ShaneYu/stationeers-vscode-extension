use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use ic10_sim::{
    REGISTER_COUNT, RETURN_ADDRESS_REGISTER, STACK_POINTER_REGISTER, Simulator, channel_index,
    direct_register_index,
};
use serde_json::{Value, json};

const REGISTERS_SCOPE: u64 = 1;
const STACK_SCOPE: u64 = 2;
const CPU_SCOPE: u64 = 3;
const PINS_SCOPE: u64 = 4;
const DEVICES_SCOPE: u64 = 5;
const DEVICE_SCOPE: u64 = 6;
const NETWORKS_SCOPE: u64 = 7;
const NETWORK_SCOPE: u64 = 8;
const DEVICE_SLOTS_SCOPE: u64 = 9;
const DEVICE_SLOT_SCOPE: u64 = 10;
const DEVICE_MEMORY_SCOPE: u64 = 11;
const REFERENCE_KIND_SHIFT: u32 = 48;
const REFERENCE_THREAD_SHIFT: u32 = 32;
const REFERENCE_THREAD_MASK: u64 = 0xFFFF;
const COMPOSITE_INDEX_BITS: usize = 16;
const COMPOSITE_INDEX_MASK: usize = (1 << COMPOSITE_INDEX_BITS) - 1;

fn main() {
    if let Err(error) = run() {
        eprintln!("ic10-dap: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (messages, receiver) = mpsc::channel::<Value>();
    let output_sequence = Arc::new(AtomicU64::new(1));
    let writer = thread::spawn(move || write_messages(receiver));
    let output = Output {
        messages,
        sequence: output_sequence,
    };
    let state = Arc::new(Mutex::new(AdapterState::default()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let runner = spawn_runner(state.clone(), output.clone(), shutdown.clone());

    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    while let Some(message) = read_message(&mut input)? {
        let Some(request) = Request::parse(message) else {
            continue;
        };
        handle_request(&state, &output, &shutdown, request);
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
    }

    shutdown.store(true, Ordering::SeqCst);
    let _ = runner.join();
    drop(output);
    let _ = writer.join();
    Ok(())
}

#[derive(Clone)]
struct Output {
    messages: mpsc::Sender<Value>,
    sequence: Arc<AtomicU64>,
}

impl Output {
    fn response(&self, request: &Request, body: Value) {
        self.send(json!({
            "type": "response",
            "request_seq": request.seq,
            "success": true,
            "command": request.command,
            "body": body
        }));
    }

    fn empty_response(&self, request: &Request) {
        self.response(request, json!({}));
    }

    fn error(&self, request: &Request, message: impl Into<String>) {
        let message = message.into();
        self.send(json!({
            "type": "response",
            "request_seq": request.seq,
            "success": false,
            "command": request.command,
            "message": message,
            "body": {
                "error": {
                    "id": 1,
                    "format": message,
                    "showUser": true
                }
            }
        }));
    }

    fn event(&self, event: &str, body: Value) {
        self.send(json!({
            "type": "event",
            "event": event,
            "body": body
        }));
    }

    fn stopped(&self, reason: &str, thread_id: usize, description: Option<&str>) {
        self.event(
            "stopped",
            json!({
                "reason": reason,
                "description": description,
                "threadId": thread_id + 1,
                "allThreadsStopped": true
            }),
        );
        self.event("ic10/stateChanged", json!({ "threadId": thread_id + 1 }));
    }

    fn send(&self, mut value: Value) {
        value["seq"] = Value::from(self.sequence.fetch_add(1, Ordering::SeqCst));
        let _ = self.messages.send(value);
    }
}

fn write_messages(receiver: mpsc::Receiver<Value>) {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    for message in receiver {
        let Ok(payload) = serde_json::to_vec(&message) else {
            continue;
        };
        if write!(output, "Content-Length: {}\r\n\r\n", payload.len()).is_err() {
            return;
        }
        if output.write_all(&payload).is_err() || output.flush().is_err() {
            return;
        }
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Option<Value>, String> {
    let mut content_length = None;
    loop {
        let mut header = String::new();
        let read = reader
            .read_line(&mut header)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(None);
        }
        if header == "\r\n" || header == "\n" {
            break;
        }
        if let Some(value) = header
            .strip_prefix("Content-Length:")
            .and_then(|value| value.trim().parse::<usize>().ok())
        {
            content_length = Some(value);
        }
    }
    let length = content_length.ok_or_else(|| "missing Content-Length header".to_owned())?;
    let mut payload = vec![0; length];
    reader
        .read_exact(&mut payload)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&payload)
        .map(Some)
        .map_err(|error| format!("invalid DAP request: {error}"))
}

struct Request {
    seq: u64,
    command: String,
    arguments: Value,
}

impl Request {
    fn parse(value: Value) -> Option<Self> {
        if value.get("type")?.as_str()? != "request" {
            return None;
        }
        Some(Self {
            seq: value.get("seq")?.as_u64()?,
            command: value.get("command")?.as_str()?.to_owned(),
            arguments: value.get("arguments").cloned().unwrap_or_else(|| json!({})),
        })
    }
}

#[derive(Default)]
struct AdapterState {
    simulator: Option<Simulator>,
    breakpoints: HashMap<String, BTreeSet<usize>>,
    running: bool,
    stop_on_entry: bool,
    focus_cpu: usize,
    configured: bool,
    last_stop: Option<(usize, usize)>,
    skip_breakpoint_once: Option<(usize, usize)>,
}

fn spawn_runner(
    state: Arc<Mutex<AdapterState>>,
    output: Output,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !shutdown.load(Ordering::SeqCst) {
            let mut stopped = None;
            let mut terminated = false;
            {
                let Ok(mut adapter) = state.lock() else {
                    return;
                };
                if !adapter.running {
                    drop(adapter);
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                let breakpoints = adapter.breakpoints.clone();
                let skip = adapter.skip_breakpoint_once;
                let Some(simulator) = adapter.simulator.as_mut() else {
                    adapter.running = false;
                    continue;
                };
                if simulator.is_finished() {
                    adapter.running = false;
                    terminated = true;
                } else if let Some((cpu, line)) = simulator.next_scheduled_location() {
                    let location = (cpu, line);
                    let path = normalize_path(&simulator.cpus[cpu].program.source_path);
                    let hits = breakpoints
                        .get(&path)
                        .is_some_and(|lines| lines.contains(&(line + 1)));
                    if hits && skip != Some(location) {
                        adapter.running = false;
                        adapter.last_stop = Some(location);
                        stopped = Some((cpu, "breakpoint".to_owned(), None));
                    } else {
                        let clear_skip = skip == Some(location);
                        match simulator.scheduler_step() {
                            Ok(_) => {}
                            Err(error) => {
                                adapter.running = false;
                                adapter.last_stop = Some(location);
                                stopped = Some((cpu, "exception".to_owned(), Some(error)));
                            }
                        }
                        if clear_skip {
                            adapter.skip_breakpoint_once = None;
                        }
                    }
                }
            }
            if let Some((cpu, reason, description)) = stopped {
                output.stopped(&reason, cpu, description.as_deref());
            } else if terminated {
                output.event("terminated", json!({}));
            } else {
                thread::yield_now();
            }
        }
    })
}

fn handle_request(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    shutdown: &Arc<AtomicBool>,
    request: Request,
) {
    let result = match request.command.as_str() {
        "initialize" => {
            output.response(
                &request,
                json!({
                    "supportsConfigurationDoneRequest": true,
                    "supportsSetVariable": true,
                    "supportsSetExpression": false,
                    "supportsEvaluateForHovers": true,
                    "supportsConditionalBreakpoints": false,
                    "supportsHitConditionalBreakpoints": false,
                    "supportsLogPoints": false,
                    "supportsSingleThreadExecutionRequests": true,
                    "supportTerminateDebuggee": true
                }),
            );
            output.event("initialized", json!({}));
            Ok(())
        }
        "launch" => launch(state, output, &request),
        "setBreakpoints" => set_breakpoints(state, output, &request),
        "setExceptionBreakpoints" | "setFunctionBreakpoints" => {
            output.response(&request, json!({ "breakpoints": [] }));
            Ok(())
        }
        "configurationDone" => configuration_done(state, output, &request),
        "threads" => threads(state, output, &request),
        "stackTrace" => stack_trace(state, output, &request),
        "scopes" => scopes(state, output, &request),
        "variables" => variables(state, output, &request),
        "setVariable" => set_variable(state, output, &request),
        "evaluate" => evaluate(state, output, &request),
        "continue" => continue_execution(state, output, &request),
        "next" | "stepIn" | "stepOut" => step_instruction(state, output, &request),
        "pause" => pause(state, output, &request),
        "disconnect" | "terminate" => {
            if let Ok(mut adapter) = state.lock() {
                adapter.running = false;
            }
            output.empty_response(&request);
            output.event("terminated", json!({}));
            shutdown.store(true, Ordering::SeqCst);
            Ok(())
        }
        "ic10/stepTick" => step_tick(state, output, &request),
        "ic10/getState" => get_state(state, output, &request),
        "ic10/setState" => set_state(state, output, &request),
        command => Err(format!("unsupported DAP request `{command}`")),
    };
    if let Err(error) = result {
        output.error(&request, error);
    }
}

fn launch(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let path = request
        .arguments
        .get("scenario")
        .and_then(Value::as_str)
        .ok_or_else(|| "launch configuration requires `scenario`".to_owned())?;
    let simulator =
        Simulator::from_scenario_path(Path::new(path)).map_err(|error| error.to_string())?;
    let compatibility_warnings = simulator.compatibility_warnings.clone();
    let focus_cpu = if let Some(focus_ic) = request.arguments.get("focusIc").and_then(Value::as_str)
    {
        simulator
            .cpus
            .iter()
            .position(|cpu| cpu.id == focus_ic)
            .ok_or_else(|| {
                format!("simulation does not contain an IC housing with stable ID `{focus_ic}`")
            })?
    } else {
        (!simulator.cpus.is_empty())
            .then_some(0)
            .ok_or_else(|| "simulation does not contain a runnable IC".to_owned())?
    };
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.simulator = Some(simulator);
    adapter.stop_on_entry = request
        .arguments
        .get("stopOnEntry")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    adapter.focus_cpu = focus_cpu;
    adapter.configured = false;
    adapter.running = false;
    adapter.last_stop = None;
    adapter.skip_breakpoint_once = None;
    output.empty_response(request);
    for warning in compatibility_warnings {
        output.event(
            "output",
            json!({
                "category": "console",
                "output": format!("IC10 compatibility warning: {warning}\n")
            }),
        );
    }
    Ok(())
}

fn configuration_done(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.configured = true;
    let stop_on_entry = adapter.stop_on_entry;
    if stop_on_entry {
        let cpu = adapter.focus_cpu;
        let simulator = adapter
            .simulator
            .as_ref()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        adapter.last_stop = simulator.cpus[cpu].current_line().map(|line| (cpu, line));
        output.empty_response(request);
        output.stopped("entry", cpu, None);
    } else {
        adapter.running = true;
        output.empty_response(request);
    }
    Ok(())
}

fn set_breakpoints(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let path = request
        .arguments
        .pointer("/source/path")
        .and_then(Value::as_str)
        .ok_or_else(|| "setBreakpoints requires source.path".to_owned())?;
    let key = normalize_path(Path::new(path));
    let requested: Vec<usize> = request
        .arguments
        .get("breakpoints")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.get("line").and_then(Value::as_u64))
                .map(|value| value as usize)
                .collect()
        })
        .unwrap_or_default();
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter
        .breakpoints
        .insert(key.clone(), requested.iter().copied().collect());
    let executable: BTreeSet<_> = adapter
        .simulator
        .as_ref()
        .into_iter()
        .flat_map(|simulator| &simulator.cpus)
        .filter(|cpu| normalize_path(&cpu.program.source_path) == key)
        .flat_map(|cpu| cpu.program.operations.keys().map(|line| line + 1))
        .collect();
    let breakpoints: Vec<_> = requested
        .into_iter()
        .map(|line| {
            let verified = executable.contains(&line);
            json!({
                "verified": verified,
                "line": line,
                "message": if verified { Value::Null } else { Value::String("No executable IC10 instruction exists on this line.".to_owned()) }
            })
        })
        .collect();
    output.response(request, json!({ "breakpoints": breakpoints }));
    Ok(())
}

fn threads(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let threads: Vec<_> = simulator
        .cpus
        .iter()
        .enumerate()
        .map(|(index, cpu)| {
            json!({
                "id": index + 1,
                "name": cpu.name
            })
        })
        .collect();
    output.response(request, json!({ "threads": threads }));
    Ok(())
}

fn stack_trace(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = thread_index(&request.arguments)?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let cpu = simulator
        .cpus
        .get(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    let line = cpu.current_line().unwrap_or(cpu.pc) + 1;
    let path = cpu.program.source_path.to_string_lossy();
    let name = cpu
        .program
        .labels
        .iter()
        .filter(|(_, label_line)| **label_line <= line.saturating_sub(1))
        .max_by_key(|(_, label_line)| *label_line)
        .map_or_else(
            || cpu.name.clone(),
            |(label, _)| format!("{} — {label}", cpu.name),
        );
    output.response(
        request,
        json!({
            "stackFrames": [{
                "id": thread + 1,
                "name": name,
                "source": {
                    "name": cpu.program.source_path.file_name().and_then(|value| value.to_str()),
                    "path": path
                },
                "line": line,
                "column": 1
            }],
            "totalFrames": 1
        }),
    );
    Ok(())
}

fn scopes(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let frame = request
        .arguments
        .get("frameId")
        .and_then(Value::as_u64)
        .ok_or_else(|| "scopes requires frameId".to_owned())? as usize;
    let thread = frame
        .checked_sub(1)
        .ok_or_else(|| "invalid frameId".to_owned())?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    if thread >= simulator.cpus.len() {
        return Err(format!("unknown frame {frame}"));
    }
    output.response(
        request,
        json!({
            "scopes": [
                scope("Registers", reference(REGISTERS_SCOPE, thread, 0), false, None),
                scope("Stack", reference(STACK_SCOPE, thread, 0), false, Some(512)),
                scope("CPU", reference(CPU_SCOPE, thread, 0), false, None),
                scope("Pins", reference(PINS_SCOPE, thread, 0), false, None),
                scope("Devices", reference(DEVICES_SCOPE, thread, 0), true, None),
                scope("Networks", reference(NETWORKS_SCOPE, thread, 0), true, None)
            ]
        }),
    );
    Ok(())
}

fn scope(name: &str, variables_reference: u64, expensive: bool, indexed: Option<usize>) -> Value {
    json!({
        "name": name,
        "variablesReference": variables_reference,
        "expensive": expensive,
        "indexedVariables": indexed
    })
}

fn variables(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let encoded = request
        .arguments
        .get("variablesReference")
        .and_then(Value::as_u64)
        .ok_or_else(|| "variables requires variablesReference".to_owned())?;
    let (kind, thread, index) = decode_reference(encoded)?;
    let start = request
        .arguments
        .get("start")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let count = request
        .arguments
        .get("count")
        .and_then(Value::as_u64)
        .map_or(usize::MAX, |value| value as usize);
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let cpu = simulator
        .cpus
        .get(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    let values = match kind {
        REGISTERS_SCOPE => register_variables(cpu),
        STACK_SCOPE => cpu
            .stack
            .iter()
            .enumerate()
            .skip(start)
            .take(count)
            .map(|(address, value)| {
                leaf(
                    &address.to_string(),
                    *value,
                    Some(format!("stack[{address}]")),
                )
            })
            .collect(),
        CPU_SCOPE => vec![
            text_leaf("state", &format!("{:?}", cpu.state), None),
            leaf("line", cpu.current_line().unwrap_or(cpu.pc) as f64, None),
            leaf("tick", simulator.tick as f64, Some("tick".to_owned())),
            leaf("operationsThisTick", cpu.operations_this_tick as f64, None),
            text_leaf("error", cpu.error.as_deref().unwrap_or(""), None),
        ],
        PINS_SCOPE => pin_variables(simulator, thread),
        DEVICES_SCOPE => simulator
            .world
            .devices
            .iter()
            .enumerate()
            .map(|(device_index, device)| {
                structured(
                    &device.id,
                    &format!("{} ({})", device.name, device.prefab),
                    reference(DEVICE_SCOPE, thread, device_index),
                    Some("device".to_owned()),
                    Some(
                        device.fields.len()
                            + usize::from(!device.slots.is_empty())
                            + usize::from(!device.memory.is_empty()),
                    ),
                )
            })
            .collect(),
        DEVICE_SCOPE => {
            let device = simulator
                .world
                .devices
                .get(index)
                .ok_or_else(|| format!("unknown device index {index}"))?;
            let mut values: Vec<_> = device
                .fields
                .iter()
                .map(|(name, value)| {
                    leaf(
                        name,
                        *value,
                        Some(format!("device(\"{}\").{name}", device.id)),
                    )
                })
                .collect();
            if !device.slots.is_empty() {
                values.push(structured(
                    "Slots",
                    &format!("{} slots", device.slots.len()),
                    reference(DEVICE_SLOTS_SCOPE, thread, index),
                    Some("object".to_owned()),
                    Some(device.slots.len()),
                ));
            }
            if !device.memory.is_empty() {
                values.push(indexed_structured(
                    "Memory",
                    &format!("{} cells", device.memory.len()),
                    reference(DEVICE_MEMORY_SCOPE, thread, index),
                    device.memory.len(),
                ));
            }
            values
        }
        DEVICE_SLOTS_SCOPE => {
            let device = simulator
                .world
                .devices
                .get(index)
                .ok_or_else(|| format!("unknown device index {index}"))?;
            let metadata = simulator
                .knowledge
                .device_by_name(&device.prefab)
                .ok_or_else(|| format!("unknown prefab `{}`", device.prefab))?;
            device
                .slots
                .iter()
                .map(|(slot, fields)| {
                    let slot_name = metadata
                        .slots
                        .get(&slot.to_string())
                        .map(|slot| slot.name.as_str())
                        .filter(|name| !name.is_empty())
                        .unwrap_or("Unnamed");
                    structured(
                        &format!("Slot {slot}"),
                        slot_name,
                        reference(DEVICE_SLOT_SCOPE, thread, composite_index(index, *slot)),
                        Some("object".to_owned()),
                        Some(fields.len()),
                    )
                })
                .collect()
        }
        DEVICE_SLOT_SCOPE => {
            let (device_index, slot_index) = decode_composite_index(index);
            let device = simulator
                .world
                .devices
                .get(device_index)
                .ok_or_else(|| format!("unknown device index {device_index}"))?;
            let slot = device
                .slots
                .get(&slot_index)
                .ok_or_else(|| format!("device `{}` does not have slot {slot_index}", device.id))?;
            slot.iter()
                .map(|(name, value)| {
                    leaf(
                        name,
                        *value,
                        Some(format!(
                            "device(\"{}\").slot[{slot_index}].{name}",
                            device.id
                        )),
                    )
                })
                .collect()
        }
        DEVICE_MEMORY_SCOPE => {
            let device = simulator
                .world
                .devices
                .get(index)
                .ok_or_else(|| format!("unknown device index {index}"))?;
            device
                .memory
                .iter()
                .enumerate()
                .skip(start)
                .take(count)
                .map(|(address, value)| {
                    leaf(
                        &address.to_string(),
                        *value,
                        Some(format!("device(\"{}\").memory[{address}]", device.id)),
                    )
                })
                .collect()
        }
        NETWORKS_SCOPE => simulator
            .world
            .networks
            .iter()
            .enumerate()
            .filter(|(_, network)| network.kind.eq_ignore_ascii_case("cable"))
            .map(|(network_index, network)| {
                structured(
                    &network.id,
                    &format!("{} · {}", network.kind, network.cable_role),
                    reference(NETWORK_SCOPE, thread, network_index),
                    Some("network".to_owned()),
                    Some(8),
                )
            })
            .collect(),
        NETWORK_SCOPE => {
            let network = simulator
                .world
                .networks
                .get(index)
                .ok_or_else(|| format!("unknown network index {index}"))?;
            if !network.kind.eq_ignore_ascii_case("cable") {
                return Err(format!("network `{}` does not expose channels", network.id));
            }
            network
                .channels
                .iter()
                .enumerate()
                .map(|(channel, value)| {
                    leaf(
                        &format!("Channel{channel}"),
                        *value,
                        Some(format!("network(\"{}\").Channel{channel}", network.id)),
                    )
                })
                .collect()
        }
        _ => return Err(format!("unknown variable reference {encoded}")),
    };
    output.response(request, json!({ "variables": values }));
    Ok(())
}

fn register_variables(cpu: &ic10_sim::Cpu) -> Vec<Value> {
    (0..REGISTER_COUNT)
        .map(|index| {
            let name = match index {
                RETURN_ADDRESS_REGISTER => "ra".to_owned(),
                STACK_POINTER_REGISTER => "sp".to_owned(),
                _ => format!("r{index}"),
            };
            leaf(&name, cpu.registers[index], Some(name.clone()))
        })
        .collect()
}

fn pin_variables(simulator: &Simulator, thread: usize) -> Vec<Value> {
    let cpu = &simulator.cpus[thread];
    let mut values = Vec::new();
    for (pin, target) in cpu.pins.iter().enumerate() {
        values.push(text_leaf(
            &format!("d{pin}"),
            target.map_or("<not set>", |index| {
                simulator.world.devices[index].id.as_str()
            }),
            None,
        ));
    }
    values.push(text_leaf(
        "db",
        &simulator.world.devices[cpu.housing].id,
        None,
    ));
    values
}

fn leaf(name: &str, value: f64, evaluate_name: Option<String>) -> Value {
    json!({
        "name": name,
        "value": format_number(value),
        "type": "number",
        "evaluateName": evaluate_name,
        "variablesReference": 0
    })
}

fn text_leaf(name: &str, value: &str, evaluate_name: Option<String>) -> Value {
    json!({
        "name": name,
        "value": value,
        "type": "string",
        "evaluateName": evaluate_name,
        "variablesReference": 0
    })
}

fn structured(
    name: &str,
    value: &str,
    variables_reference: u64,
    kind: Option<String>,
    named_variables: Option<usize>,
) -> Value {
    json!({
        "name": name,
        "value": value,
        "type": kind,
        "variablesReference": variables_reference,
        "namedVariables": named_variables
    })
}

fn indexed_structured(
    name: &str,
    value: &str,
    variables_reference: u64,
    indexed_variables: usize,
) -> Value {
    json!({
        "name": name,
        "value": value,
        "type": "array",
        "variablesReference": variables_reference,
        "indexedVariables": indexed_variables
    })
}

fn set_variable(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let encoded = request
        .arguments
        .get("variablesReference")
        .and_then(Value::as_u64)
        .ok_or_else(|| "setVariable requires variablesReference".to_owned())?;
    let name = request
        .arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "setVariable requires name".to_owned())?;
    let parsed = request
        .arguments
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| "setVariable requires value".to_owned())
        .and_then(parse_debug_number)?;
    let (kind, thread, index) = decode_reference(encoded)?;
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    if adapter.running {
        return Err("pause the simulation before editing state".to_owned());
    }
    let simulator = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    match kind {
        REGISTERS_SCOPE => simulator.cpus[thread].set_register(name, parsed)?,
        STACK_SCOPE => {
            let address = name
                .trim_matches(|character| character == '[' || character == ']')
                .parse::<usize>()
                .map_err(|_| format!("invalid stack address `{name}`"))?;
            let slot = simulator.cpus[thread]
                .stack
                .get_mut(address)
                .ok_or_else(|| format!("stack address {address} is out of range"))?;
            *slot = parsed;
        }
        DEVICE_SCOPE => {
            let device = simulator
                .world
                .devices
                .get_mut(index)
                .ok_or_else(|| format!("unknown device index {index}"))?;
            device.fields.insert(name.to_owned(), parsed);
        }
        DEVICE_SLOT_SCOPE => {
            let (device_index, slot_index) = decode_composite_index(index);
            let device = simulator
                .world
                .devices
                .get_mut(device_index)
                .ok_or_else(|| format!("unknown device index {device_index}"))?;
            let slot = device
                .slots
                .get_mut(&slot_index)
                .ok_or_else(|| format!("device `{}` does not have slot {slot_index}", device.id))?;
            if !slot.contains_key(name) {
                return Err(format!(
                    "device `{}` slot {slot_index} has no field `{name}`",
                    device.id
                ));
            }
            slot.insert(name.to_owned(), parsed);
        }
        DEVICE_MEMORY_SCOPE => {
            let address = name
                .trim_matches(|character| character == '[' || character == ']')
                .parse::<usize>()
                .map_err(|_| format!("invalid device memory address `{name}`"))?;
            let device = simulator
                .world
                .devices
                .get_mut(index)
                .ok_or_else(|| format!("unknown device index {index}"))?;
            let cell = device.memory.get_mut(address).ok_or_else(|| {
                format!(
                    "device `{}` memory address {address} is out of range",
                    device.id
                )
            })?;
            *cell = parsed;
        }
        NETWORK_SCOPE => {
            let channel = channel_index(name).ok_or_else(|| format!("invalid channel `{name}`"))?;
            let network = simulator
                .world
                .networks
                .get_mut(index)
                .ok_or_else(|| format!("unknown network index {index}"))?;
            if !network.kind.eq_ignore_ascii_case("cable") {
                return Err(format!("network `{}` does not expose channels", network.id));
            }
            network.channels[channel] = parsed;
        }
        _ => return Err("variables in this scope are not editable".to_owned()),
    }
    output.response(
        request,
        json!({
            "value": format_number(parsed),
            "type": "number",
            "variablesReference": 0
        }),
    );
    output.event("ic10/stateChanged", json!({ "threadId": thread + 1 }));
    Ok(())
}

fn evaluate(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let expression = request
        .arguments
        .get("expression")
        .and_then(Value::as_str)
        .ok_or_else(|| "evaluate requires expression".to_owned())?;
    let thread = request
        .arguments
        .get("frameId")
        .and_then(Value::as_u64)
        .or_else(|| request.arguments.get("threadId").and_then(Value::as_u64))
        .unwrap_or(1)
        .saturating_sub(1) as usize;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let evaluated = evaluate_expression(simulator, thread, expression)?;
    let (result, value_type) = match evaluated {
        Evaluation::Number(value) => (format_number(value), "number"),
        Evaluation::Text(value) => (value, "string"),
    };
    output.response(
        request,
        json!({
            "result": result,
            "type": value_type,
            "variablesReference": 0
        }),
    );
    Ok(())
}

enum Evaluation {
    Number(f64),
    Text(String),
}

fn evaluate_expression(
    simulator: &Simulator,
    thread: usize,
    expression: &str,
) -> Result<Evaluation, String> {
    let cpu = simulator
        .cpus
        .get(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    let expression = expression.trim();
    if expression == "tick" {
        return Ok(Evaluation::Number(simulator.tick as f64));
    }
    if let Some(index) = direct_register_index(expression) {
        return Ok(Evaluation::Number(cpu.registers[index]));
    }
    if let Some(address) = expression
        .strip_prefix("stack[")
        .and_then(|value| value.strip_suffix(']'))
        .and_then(|value| value.parse::<usize>().ok())
    {
        return cpu
            .stack
            .get(address)
            .copied()
            .map(Evaluation::Number)
            .ok_or_else(|| format!("stack address {address} is out of range"));
    }
    if let Some((id, slot_index, field)) = device_slot_expression(expression) {
        let device_index = simulator
            .world
            .device_index(id)
            .ok_or_else(|| format!("unknown device `{id}`"))?;
        return simulator.world.devices[device_index]
            .slots
            .get(&slot_index)
            .ok_or_else(|| format!("device `{id}` does not have slot {slot_index}"))?
            .get(field)
            .copied()
            .map(Evaluation::Number)
            .ok_or_else(|| format!("device `{id}` slot {slot_index} has no field `{field}`"));
    }
    if let Some((id, address)) = device_memory_expression(expression) {
        let device_index = simulator
            .world
            .device_index(id)
            .ok_or_else(|| format!("unknown device `{id}`"))?;
        return simulator.world.devices[device_index]
            .memory
            .get(address)
            .copied()
            .map(Evaluation::Number)
            .ok_or_else(|| format!("device `{id}` memory address {address} is out of range"));
    }
    if let Some((id, field)) = object_expression(expression, "device") {
        let index = simulator
            .world
            .device_index(id)
            .ok_or_else(|| format!("unknown device `{id}`"))?;
        return simulator.world.devices[index]
            .fields
            .get(field)
            .copied()
            .map(Evaluation::Number)
            .ok_or_else(|| format!("device `{id}` has no field `{field}`"));
    }
    if let Some((id, field)) = object_expression(expression, "network") {
        let index = simulator
            .world
            .network_index(id)
            .ok_or_else(|| format!("unknown network `{id}`"))?;
        if !simulator.world.networks[index]
            .kind
            .eq_ignore_ascii_case("cable")
        {
            return Err(format!("network `{id}` does not expose channels"));
        }
        let channel = channel_index(field).ok_or_else(|| format!("invalid channel `{field}`"))?;
        return Ok(Evaluation::Number(
            simulator.world.networks[index].channels[channel],
        ));
    }
    if let Some(description) = evaluate_device_reference(simulator, thread, expression) {
        return Ok(Evaluation::Text(description));
    }
    if let Ok(value) = cpu.program.resolve_number(expression, &simulator.knowledge) {
        return Ok(Evaluation::Number(value));
    }
    parse_debug_number(expression).map(Evaluation::Number)
}

fn evaluate_device_reference(
    simulator: &Simulator,
    thread: usize,
    expression: &str,
) -> Option<String> {
    let cpu = simulator.cpus.get(thread)?;
    let resolved = cpu.program.resolve_alias(expression);
    let (reference, connection) = resolved
        .split_once(':')
        .map_or((resolved, None), |(reference, connection)| {
            (reference, connection.parse::<usize>().ok())
        });
    let device_index = if reference == "db" {
        cpu.housing
    } else {
        let pin = reference
            .strip_prefix('d')?
            .parse::<usize>()
            .ok()
            .filter(|pin| *pin < cpu.pins.len())?;
        let Some(device) = cpu.pins[pin] else {
            return Some(format!("{reference} · <not set>"));
        };
        device
    };
    let device = &simulator.world.devices[device_index];
    let mut description = format!("{} · {}", device.id, device.name);
    if let Some(connection) = connection {
        if let Some(network_index) = device.connections.get(&connection) {
            let network = &simulator.world.networks[*network_index];
            description.push_str(&format!(" · connection {connection} → {}", network.id));
        } else {
            description.push_str(&format!(" · connection {connection} not attached"));
        }
    }
    Some(description)
}

fn object_expression<'a>(expression: &'a str, function: &str) -> Option<(&'a str, &'a str)> {
    let rest = expression.strip_prefix(function)?.strip_prefix("(\"")?;
    let (id, field) = rest.split_once("\").")?;
    Some((id, field))
}

fn device_slot_expression(expression: &str) -> Option<(&str, usize, &str)> {
    let rest = expression.strip_prefix("device(\"")?;
    let (id, rest) = rest.split_once("\").slot[")?;
    let (slot, field) = rest.split_once("].")?;
    Some((id, slot.parse().ok()?, field))
}

fn device_memory_expression(expression: &str) -> Option<(&str, usize)> {
    let rest = expression.strip_prefix("device(\"")?;
    let (id, address) = rest.split_once("\").memory[")?;
    Some((id, address.strip_suffix(']')?.parse().ok()?))
}

fn continue_execution(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    if adapter.simulator.is_none() {
        return Err("no simulation is loaded".to_owned());
    }
    adapter.skip_breakpoint_once = adapter.last_stop.take();
    adapter.running = true;
    output.response(
        request,
        json!({
            "allThreadsContinued": true
        }),
    );
    output.event("continued", json!({ "allThreadsContinued": true }));
    Ok(())
}

fn step_instruction(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = thread_index(&request.arguments)?;
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.running = false;
    let simulator = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let result = simulator.step_instruction(thread);
    let description = result.as_ref().err().map(String::as_str);
    adapter.last_stop = simulator.cpus[thread]
        .current_line()
        .map(|line| (thread, line));
    output.empty_response(request);
    output.stopped(
        if result.is_ok() { "step" } else { "exception" },
        thread,
        description,
    );
    Ok(())
}

fn pause(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = thread_index(&request.arguments).unwrap_or(0);
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.running = false;
    output.empty_response(request);
    output.stopped("pause", thread, None);
    Ok(())
}

fn step_tick(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = request
        .arguments
        .get("threadId")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .saturating_sub(1) as usize;
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.running = false;
    let (line, tick) = {
        let simulator = adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        simulator.step_world_tick()?;
        (simulator.cpus[thread].current_line(), simulator.tick)
    };
    adapter.last_stop = line.map(|line| (thread, line));
    output.response(request, json!({ "tick": tick }));
    output.stopped("step", thread, Some("Completed one simulated world tick."));
    Ok(())
}

fn get_state(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = request
        .arguments
        .get("threadId")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .saturating_sub(1) as usize;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let cpu = simulator
        .cpus
        .get(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    let registers: Vec<_> = register_variables(cpu);
    let stack: Vec<_> = cpu
        .stack
        .iter()
        .map(|value| format_number(*value))
        .collect();
    let cpus: Vec<_> = simulator
        .cpus
        .iter()
        .enumerate()
        .map(|(index, cpu)| {
            json!({
                "threadId": index + 1,
                "id": cpu.id,
                "name": cpu.name,
                "line": cpu.current_line().map(|line| line + 1),
                "state": format!("{:?}", cpu.state)
            })
        })
        .collect();
    output.response(
        request,
        json!({
            "threadId": thread + 1,
            "tick": simulator.tick,
            "cpu": {
                "id": cpu.id,
                "name": cpu.name,
                "line": cpu.current_line().map(|line| line + 1),
                "state": format!("{:?}", cpu.state),
                "error": cpu.error
            },
            "cpus": cpus,
            "registers": registers,
            "stack": stack
        }),
    );
    Ok(())
}

fn set_state(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = request
        .arguments
        .get("threadId")
        .and_then(Value::as_u64)
        .unwrap_or(1)
        .saturating_sub(1) as usize;
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    if adapter.running {
        return Err("pause the simulation before editing state".to_owned());
    }
    let simulator = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let cpu = simulator
        .cpus
        .get_mut(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    if let Some(registers) = request
        .arguments
        .get("registers")
        .and_then(Value::as_object)
    {
        for (name, value) in registers {
            let parsed = debug_value(value)?;
            cpu.set_register(name, parsed)?;
        }
    }
    if let Some(stack) = request.arguments.get("stack").and_then(Value::as_object) {
        for (address, value) in stack {
            let address = address
                .parse::<usize>()
                .map_err(|_| format!("invalid stack address `{address}`"))?;
            let slot = cpu
                .stack
                .get_mut(address)
                .ok_or_else(|| format!("stack address {address} is out of range"))?;
            *slot = debug_value(value)?;
        }
    }
    output.empty_response(request);
    output.event("ic10/stateChanged", json!({ "threadId": thread + 1 }));
    Ok(())
}

fn thread_index(arguments: &Value) -> Result<usize, String> {
    arguments
        .get("threadId")
        .and_then(Value::as_u64)
        .and_then(|value| value.checked_sub(1))
        .map(|value| value as usize)
        .ok_or_else(|| "request requires a valid threadId".to_owned())
}

fn reference(kind: u64, thread: usize, index: usize) -> u64 {
    (kind << REFERENCE_KIND_SHIFT)
        | ((thread as u64) << REFERENCE_THREAD_SHIFT)
        | (index as u64 + 1)
}

fn decode_reference(value: u64) -> Result<(u64, usize, usize), String> {
    let kind = value >> REFERENCE_KIND_SHIFT;
    let thread = ((value >> REFERENCE_THREAD_SHIFT) & REFERENCE_THREAD_MASK) as usize;
    let index = (value & 0xFFFF_FFFF)
        .checked_sub(1)
        .ok_or_else(|| format!("invalid variable reference {value}"))? as usize;
    Ok((kind, thread, index))
}

fn composite_index(first: usize, second: usize) -> usize {
    (first << COMPOSITE_INDEX_BITS) | (second & COMPOSITE_INDEX_MASK)
}

fn decode_composite_index(value: usize) -> (usize, usize) {
    (value >> COMPOSITE_INDEX_BITS, value & COMPOSITE_INDEX_MASK)
}

fn normalize_path(path: &Path) -> String {
    let normalized = path
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .replace('\\', "/");
    if cfg!(windows) {
        normalized.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn format_number(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value == f64::INFINITY {
        "Infinity".to_owned()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".to_owned()
    } else if value == 0.0 && value.is_sign_negative() {
        "-0".to_owned()
    } else {
        value.to_string()
    }
}

fn parse_debug_number(value: &str) -> Result<f64, String> {
    match value.trim() {
        "NaN" | "nan" => Ok(f64::NAN),
        "Infinity" | "+Infinity" | "inf" | "+inf" => Ok(f64::INFINITY),
        "-Infinity" | "-inf" => Ok(f64::NEG_INFINITY),
        value => value
            .parse::<f64>()
            .map_err(|_| format!("`{value}` is not a number")),
    }
}

fn debug_value(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .map(Ok)
        .or_else(|| value.as_str().map(parse_debug_number))
        .unwrap_or_else(|| Err("state value must be a number or special-number string".to_owned()))
}

#[cfg(test)]
mod tests {
    use super::{
        DEVICE_MEMORY_SCOPE, DEVICE_SLOT_SCOPE, composite_index, decode_reference, reference,
    };

    #[test]
    fn variable_references_survive_a_javascript_number_round_trip() {
        let references = [
            reference(DEVICE_MEMORY_SCOPE, 0, 0),
            reference(DEVICE_SLOT_SCOPE, 1, composite_index(2, 0)),
            reference(
                DEVICE_SLOT_SCOPE,
                u16::MAX as usize,
                composite_index(u16::MAX as usize, u16::MAX as usize),
            ),
        ];

        for original in references {
            assert!(original < (1_u64 << 53));
            let javascript_round_trip = original as f64 as u64;
            assert_eq!(javascript_round_trip, original);
            assert_eq!(
                decode_reference(javascript_round_trip),
                decode_reference(original)
            );
        }
    }
}
