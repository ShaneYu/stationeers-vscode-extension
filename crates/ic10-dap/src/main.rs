use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use ic10_runner::{
    TestCase, Value as Evaluation, evaluate as evaluate_expression, evaluate_with_changed,
    load_expanded_case, set_value,
};
use ic10_sim::{
    REGISTER_COUNT, RETURN_ADDRESS_REGISTER, STACK_POINTER_REGISTER, Simulator, channel_index,
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

#[derive(Clone, Debug)]
struct SourceBreakpoint {
    id: u64,
    line: usize,
    condition: Option<String>,
    hit_condition: Option<String>,
    log_message: Option<String>,
    hits: u64,
}

#[derive(Clone, Debug)]
struct DataBreakpoint {
    id: u64,
    thread: usize,
    expression: String,
    condition: Option<String>,
    hit_condition: Option<String>,
    hits: u64,
}

#[derive(Clone, Debug)]
struct LastException {
    category: String,
    message: String,
    thread: usize,
}

#[derive(Default)]
struct AdapterState {
    simulator: Option<Simulator>,
    breakpoints: HashMap<String, Vec<SourceBreakpoint>>,
    function_breakpoints: Vec<SourceBreakpoint>,
    data_breakpoints: Vec<DataBreakpoint>,
    exception_filters: BTreeSet<String>,
    next_breakpoint_id: u64,
    launch_arguments: Option<Value>,
    running: bool,
    stop_on_entry: bool,
    focus_cpu: usize,
    configured: bool,
    last_stop: Option<(usize, usize)>,
    skip_breakpoint_once: Option<(usize, usize)>,
    test_case: Option<TestCase>,
    test_tick_applied: Option<u64>,
    test_thread: usize,
    test_satisfied: BTreeSet<usize>,
    previous_values: HashMap<String, f64>,
    stop_values: HashMap<String, f64>,
    last_exception: Option<LastException>,
    single_thread: Option<usize>,
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
                let skip = adapter.skip_breakpoint_once;
                let pending_test = adapter
                    .test_case
                    .clone()
                    .filter(|_| {
                        adapter.simulator.as_ref().is_some_and(|simulator| {
                            adapter.test_tick_applied != Some(simulator.tick)
                        })
                    })
                    .map(|test_case| (test_case, adapter.test_satisfied.clone()));
                let test_thread = adapter.test_thread;
                let mut applied_test = None;
                let mut assertion_stop = None;
                let mut assertion_failed = false;
                let mut capture_stop = false;
                let Some(mut simulator) = adapter.simulator.take() else {
                    adapter.running = false;
                    continue;
                };
                if let Some((test_case, mut satisfied)) = pending_test {
                    let tick = simulator.tick;
                    match apply_test_tick(&mut simulator, test_thread, &test_case, &mut satisfied) {
                        Ok(()) => {
                            applied_test = Some((tick, satisfied));
                        }
                        Err(error) => {
                            assertion_failed = true;
                            assertion_stop = simulator
                                .cpus
                                .get(test_thread)
                                .and_then(|cpu| cpu.current_line())
                                .map(|line| (test_thread, line));
                            stopped = Some((test_thread, "exception".to_owned(), Some(error)));
                        }
                    }
                }
                if stopped.is_some() {
                    // Assertion failures pause the complete shared world.
                } else if simulator.is_finished() {
                    adapter.running = false;
                    terminated = true;
                } else if let Some((cpu, line)) = adapter
                    .single_thread
                    .and_then(|cpu| {
                        simulator
                            .cpus
                            .get(cpu)?
                            .current_line()
                            .map(|line| (cpu, line))
                    })
                    .or_else(|| {
                        adapter
                            .single_thread
                            .is_none()
                            .then(|| simulator.next_scheduled_location())
                            .flatten()
                    })
                {
                    let location = (cpu, line);
                    let program = &simulator.cpus[cpu].program;
                    let path = normalize_path(&program.debug_source_path);
                    let debug_line = program.debug_line(line);
                    let label = program.labels.iter().find_map(|(name, target)| {
                        program
                            .operation_at_or_after(*target)
                            .is_some_and(|operation| operation.line == line)
                            .then_some(name.clone())
                    });
                    let mut candidates = adapter
                        .breakpoints
                        .get(&path)
                        .into_iter()
                        .flatten()
                        .filter(|breakpoint| breakpoint.line == debug_line + 1)
                        .cloned()
                        .collect::<Vec<_>>();
                    if let Some(label) = label {
                        candidates.extend(
                            adapter
                                .function_breakpoints
                                .iter()
                                .filter(|breakpoint| {
                                    breakpoint
                                        .condition
                                        .as_deref()
                                        .is_some_and(|name| name == label)
                                })
                                .cloned(),
                        );
                    }
                    let mut should_stop = false;
                    for candidate in candidates {
                        let next_hits = candidate.hits + 1;
                        update_breakpoint_hits(&mut adapter, candidate.id, next_hits);
                        match breakpoint_action(
                            &simulator,
                            cpu,
                            &candidate,
                            next_hits,
                            &adapter.previous_values,
                        ) {
                            Ok(BreakpointAction::Ignore) => {}
                            Ok(BreakpointAction::Stop) => should_stop = true,
                            Ok(BreakpointAction::Log(message)) => output.event(
                                "output",
                                json!({ "category": "console", "output": format!("{message}\n") }),
                            ),
                            Err(message) => output.event(
                                "output",
                                json!({ "category": "stderr", "output": format!("Breakpoint {message}\n") }),
                            ),
                        }
                    }
                    if should_stop && skip != Some(location) {
                        adapter.running = false;
                        adapter.last_stop = Some(location);
                        capture_stop = true;
                        stopped = Some((cpu, "breakpoint".to_owned(), None));
                    } else {
                        let clear_skip = skip == Some(location);
                        let before_data = data_values(&simulator, cpu, &adapter.data_breakpoints);
                        let previous_values = adapter.previous_values.clone();
                        let mnemonic = simulator.cpus[cpu]
                            .current_operation()
                            .map(|operation| operation.mnemonic.clone());
                        let step_result = if adapter.single_thread == Some(cpu) {
                            simulator.step_instruction(cpu).map(Some)
                        } else {
                            simulator.scheduler_step()
                        };
                        match step_result {
                            Ok(_) => {
                                if mnemonic.as_deref() == Some("hcf")
                                    && adapter.exception_filters.contains("hcf")
                                {
                                    adapter.running = false;
                                    let message = "IC executed explicit `hcf`.".to_owned();
                                    adapter.last_exception = Some(LastException {
                                        category: "hcf".to_owned(),
                                        message: message.clone(),
                                        thread: cpu,
                                    });
                                    stopped = Some((cpu, "exception".to_owned(), Some(message)));
                                } else if let Some(expression) = changed_data_breakpoint(
                                    &simulator,
                                    cpu,
                                    &before_data,
                                    &mut adapter.data_breakpoints,
                                    &previous_values,
                                ) {
                                    adapter.running = false;
                                    stopped = Some((
                                        cpu,
                                        "data breakpoint".to_owned(),
                                        Some(format!("Data breakpoint `{expression}` changed.")),
                                    ));
                                }
                            }
                            Err(error) => {
                                adapter.running = false;
                                adapter.last_stop = Some(location);
                                let category = exception_category(&error);
                                adapter.last_exception = Some(LastException {
                                    category: category.to_owned(),
                                    message: error.clone(),
                                    thread: cpu,
                                });
                                if adapter.exception_filters.is_empty()
                                    || adapter.exception_filters.contains(category)
                                {
                                    stopped = Some((cpu, "exception".to_owned(), Some(error)));
                                }
                            }
                        }
                        if stopped.is_some() {
                            adapter.last_stop = Some(location);
                            capture_stop = true;
                        }
                        if clear_skip {
                            adapter.skip_breakpoint_once = None;
                        }
                    }
                }
                if let Some((tick, satisfied)) = applied_test {
                    adapter.test_tick_applied = Some(tick);
                    adapter.test_satisfied = satisfied;
                }
                if assertion_failed {
                    adapter.running = false;
                    adapter.last_stop = assertion_stop;
                }
                adapter.simulator = Some(simulator);
                if capture_stop {
                    capture_previous_values(&mut adapter);
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
                    "supportsConditionalBreakpoints": true,
                    "supportsHitConditionalBreakpoints": true,
                    "supportsLogPoints": true,
                    "supportsFunctionBreakpoints": true,
                    "supportsDataBreakpoints": true,
                    "supportsDataBreakpointBytes": false,
                    "supportsExceptionInfoRequest": true,
                    "supportsRestartRequest": true,
                    "supportsGotoTargetsRequest": true,
                    "supportsInlineValues": true,
                    "supportsSingleThreadExecutionRequests": true,
                    "supportTerminateDebuggee": true,
                    "supportsTerminateRequest": true,
                    "exceptionBreakpointFilters": [
                        { "filter": "compile", "label": "Compile errors", "default": true },
                        { "filter": "instruction", "label": "Invalid instructions or operands", "default": true },
                        { "filter": "device", "label": "Missing devices", "default": true },
                        { "filter": "access", "label": "Access violations", "default": true },
                        { "filter": "address", "label": "Invalid addresses", "default": true },
                        { "filter": "hcf", "label": "Explicit hcf", "default": false }
                    ]
                }),
            );
            output.event("initialized", json!({}));
            Ok(())
        }
        "launch" => launch(state, output, &request),
        "setBreakpoints" => set_breakpoints(state, output, &request),
        "setExceptionBreakpoints" => set_exception_breakpoints(state, output, &request),
        "setFunctionBreakpoints" => set_function_breakpoints(state, output, &request),
        "dataBreakpointInfo" => data_breakpoint_info(state, output, &request),
        "setDataBreakpoints" => set_data_breakpoints(state, output, &request),
        "configurationDone" => configuration_done(state, output, &request),
        "threads" => threads(state, output, &request),
        "stackTrace" => stack_trace(state, output, &request),
        "scopes" => scopes(state, output, &request),
        "variables" => variables(state, output, &request),
        "setVariable" => set_variable(state, output, &request),
        "evaluate" => evaluate(state, output, &request),
        "exceptionInfo" => exception_info(state, output, &request),
        "restart" => restart(state, output, &request),
        "gotoTargets" => goto_targets(state, output, &request),
        "goto" => goto_location(state, output, &request),
        "inlineValues" => inline_values(state, output, &request),
        "continue" => continue_execution(state, output, &request),
        "next" | "stepIn" | "stepOut" => step_instruction(state, output, &request),
        "pause" => pause(state, output, &request),
        "disconnect" => {
            if let Ok(mut adapter) = state.lock() {
                adapter.running = false;
            }
            output.empty_response(&request);
            shutdown.store(true, Ordering::SeqCst);
            Ok(())
        }
        "terminate" => {
            if let Ok(mut adapter) = state.lock() {
                adapter.running = false;
            }
            output.empty_response(&request);
            output.event("terminated", json!({}));
            shutdown.store(true, Ordering::SeqCst);
            Ok(())
        }
        "ic10/stepTick" => step_tick(state, output, &request),
        "ic10/hotReload" => hot_reload(state, output, &request),
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
    let launch_arguments = request.arguments.clone();
    let path = request
        .arguments
        .get("scenario")
        .and_then(Value::as_str)
        .ok_or_else(|| "launch configuration requires `scenario`".to_owned())?;
    let selected_test = request
        .arguments
        .get("testFile")
        .and_then(Value::as_str)
        .zip(request.arguments.get("testName").and_then(Value::as_str))
        .map(|(file, name)| load_expanded_case(Path::new(file), name))
        .transpose()?;
    let scenario_path = selected_test
        .as_ref()
        .map_or_else(|| PathBuf::from(path), |(scenario, _, _)| scenario.clone());
    let mut simulator =
        Simulator::from_scenario_path(&scenario_path).map_err(|error| error.to_string())?;
    if let Some((_, seed, _)) = selected_test.as_ref() {
        simulator.set_seed(*seed);
    }
    let compatibility_warnings = simulator.compatibility_warnings.clone();
    let focus_id = request
        .arguments
        .get("focusIc")
        .and_then(Value::as_str)
        .or_else(|| {
            selected_test
                .as_ref()
                .and_then(|(_, _, test_case)| test_case.focus_ic.as_deref())
        });
    let focus_cpu = if let Some(focus_ic) = focus_id {
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
    if let Some((_, _, test_case)) = &selected_test {
        for (target, value) in &test_case.initial {
            set_value(&mut simulator, focus_cpu, target, value.as_f64()?)?;
        }
    }
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.simulator = Some(simulator);
    adapter.launch_arguments = Some(launch_arguments);
    adapter.stop_on_entry = request
        .arguments
        .get("stopOnEntry")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    adapter.focus_cpu = focus_cpu;
    adapter.configured = false;
    adapter.running = false;
    adapter.single_thread = None;
    adapter.last_stop = None;
    adapter.skip_breakpoint_once = None;
    adapter.test_case = selected_test.map(|(_, _, test_case)| test_case);
    adapter.test_tick_applied = None;
    adapter.test_thread = focus_cpu;
    adapter.test_satisfied.clear();
    adapter.previous_values.clear();
    adapter.stop_values.clear();
    adapter.last_exception = None;
    adapter.single_thread = None;
    capture_previous_values(&mut adapter);
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

fn apply_test_tick(
    simulator: &mut Simulator,
    thread: usize,
    test_case: &TestCase,
    satisfied: &mut BTreeSet<usize>,
) -> Result<(), String> {
    let current_tick = simulator.tick;
    for entry in test_case
        .timeline
        .iter()
        .filter(|entry| entry.tick == current_tick)
    {
        for (target, value) in &entry.set {
            set_value(simulator, thread, target, value.as_f64()?)
                .map_err(|error| format!("test stimulus `{target}` failed: {error}"))?;
        }
        for event in &entry.events {
            set_value(simulator, thread, &event.target, event.value.as_f64()?)
                .map_err(|error| format!("test event `{}` failed: {error}", event.target))?;
        }
    }
    for (index, assertion) in test_case.assertions.iter().enumerate() {
        let expression = assertion.expression()?;
        if assertion.at_tick.is_some_and(|tick| tick != simulator.tick) {
            continue;
        }
        let actual = evaluate_expression(simulator, thread, expression)?;
        let matches = if let Some(expected) = &assertion.expected {
            let actual = actual.number()?;
            let expected = expected.as_f64()?;
            if actual.is_nan() || expected.is_nan() {
                actual.is_nan() && expected.is_nan()
            } else if actual.is_infinite()
                || expected.is_infinite()
                || (actual == 0.0 && expected == 0.0)
            {
                actual.to_bits() == expected.to_bits()
            } else {
                let tolerance = assertion.tolerance.clone().unwrap_or_default();
                (actual - expected).abs()
                    <= tolerance
                        .absolute
                        .max(tolerance.relative * actual.abs().max(expected.abs()))
            }
        } else {
            actual.truthy()?
        };
        if assertion.eventually.is_some() {
            if matches {
                satisfied.insert(index);
            } else if simulator.tick >= assertion.within_ticks.unwrap_or(test_case.max_ticks)
                && !satisfied.contains(&index)
            {
                return Err(format!(
                    "test assertion failed at tick {}: `{expression}` expected true, actual {}",
                    simulator.tick,
                    actual.display()
                ));
            }
        } else if (assertion.always.is_some()
            || assertion.at_tick == Some(simulator.tick)
            || (assertion.at_tick.is_none() && simulator.tick == test_case.max_ticks))
            && !matches
        {
            return Err(format!(
                "test assertion failed at tick {}: `{expression}` expected {}, actual {}",
                simulator.tick,
                assertion
                    .expected
                    .as_ref()
                    .map(|value| value.as_f64().map(format_number))
                    .transpose()?
                    .unwrap_or_else(|| "true".to_owned()),
                actual.display()
            ));
        }
    }
    Ok(())
}

fn apply_pending_test(adapter: &mut AdapterState) -> Result<(), String> {
    let Some(test_case) = adapter.test_case.clone() else {
        return Ok(());
    };
    let tick = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?
        .tick;
    if adapter.test_tick_applied == Some(tick) {
        return Ok(());
    }
    let mut satisfied = adapter.test_satisfied.clone();
    apply_test_tick(
        adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?,
        adapter.test_thread,
        &test_case,
        &mut satisfied,
    )?;
    adapter.test_tick_applied = Some(tick);
    adapter.test_satisfied = satisfied;
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
    let requested: Vec<Value> = request
        .arguments
        .get("breakpoints")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let executable: BTreeSet<_> = adapter
        .simulator
        .as_ref()
        .into_iter()
        .flat_map(|simulator| &simulator.cpus)
        .filter(|cpu| normalize_path(&cpu.program.debug_source_path) == key)
        .flat_map(|cpu| {
            cpu.program
                .operations
                .keys()
                .map(|line| cpu.program.debug_line(*line) + 1)
        })
        .collect();
    let mut installed = Vec::new();
    let breakpoints: Vec<_> = requested
        .into_iter()
        .map(|value| {
            let line = value.get("line").and_then(Value::as_u64).unwrap_or(0) as usize;
            let condition = value.get("condition").and_then(Value::as_str).map(str::to_owned);
            let hit_condition = value
                .get("hitCondition")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let log_message = value
                .get("logMessage")
                .and_then(Value::as_str)
                .map(str::to_owned);
            adapter.next_breakpoint_id += 1;
            let breakpoint = SourceBreakpoint {
                id: adapter.next_breakpoint_id,
                line,
                condition,
                hit_condition,
                log_message,
                hits: 0,
            };
            let expression_error = breakpoint
                .condition
                .as_deref()
                .and_then(|expression| validate_expression_for_path(&adapter, &key, expression).err())
                .or_else(|| {
                    breakpoint
                        .hit_condition
                        .as_deref()
                        .and_then(|condition| validate_hit_condition(condition).err())
                })
                .or_else(|| {
                    breakpoint
                        .log_message
                        .as_deref()
                        .and_then(|message| validate_log_message(&adapter, &key, message).err())
                });
            let verified = executable.contains(&line) && expression_error.is_none();
            if verified {
                installed.push(breakpoint.clone());
            }
            json!({
                "id": breakpoint.id,
                "verified": verified,
                "line": line,
                "message": expression_error.map_or_else(
                    || if executable.contains(&line) { Value::Null } else { Value::String("No executable IC10 instruction exists on this line.".to_owned()) },
                    |error| Value::String(format!("Invalid breakpoint expression: {error}"))
                )
            })
        })
        .collect();
    adapter.breakpoints.insert(key, installed);
    output.response(request, json!({ "breakpoints": breakpoints }));
    Ok(())
}

fn set_exception_breakpoints(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let filters = request
        .arguments
        .get("filters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?
        .exception_filters = filters;
    output.response(request, json!({ "breakpoints": [] }));
    Ok(())
}

fn set_function_breakpoints(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let requested = request
        .arguments
        .get("breakpoints")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let mut installed = Vec::new();
    let response = requested
        .into_iter()
        .map(|value| {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let exists = adapter.simulator.as_ref().is_some_and(|simulator| {
                simulator
                    .cpus
                    .iter()
                    .any(|cpu| cpu.program.labels.contains_key(name))
            });
            adapter.next_breakpoint_id += 1;
            let breakpoint = SourceBreakpoint {
                id: adapter.next_breakpoint_id,
                line: 0,
                // Function breakpoints use this field to retain their symbolic name.
                condition: Some(name.to_owned()),
                hit_condition: value
                    .get("hitCondition")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                log_message: None,
                hits: 0,
            };
            let hit_error = breakpoint
                .hit_condition
                .as_deref()
                .and_then(|condition| validate_hit_condition(condition).err());
            let verified = exists && hit_error.is_none();
            if verified {
                installed.push(breakpoint.clone());
            }
            json!({
                "id": breakpoint.id,
                "verified": verified,
                "message": hit_error
                    .map(|error| format!("Invalid hit condition: {error}"))
                    .or_else(|| (!exists).then(|| format!("Unknown IC10 label `{name}`.")))
            })
        })
        .collect::<Vec<_>>();
    adapter.function_breakpoints = installed;
    output.response(request, json!({ "breakpoints": response }));
    Ok(())
}

fn data_breakpoint_info(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let encoded = request
        .arguments
        .get("variablesReference")
        .and_then(Value::as_u64)
        .ok_or_else(|| "dataBreakpointInfo requires variablesReference".to_owned())?;
    let name = request
        .arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "dataBreakpointInfo requires name".to_owned())?;
    let (kind, thread, index) = decode_reference(encoded)?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let expression = expression_for_variable(simulator, kind, thread, index, name);
    output.response(
        request,
        expression.map_or_else(
            || json!({ "dataId": Value::Null, "description": "This value is not writable.", "accessTypes": [], "canPersist": false }),
            |expression| json!({
                "dataId": format!("{}|{expression}", thread + 1),
                "description": format!("Break when `{expression}` changes"),
                "accessTypes": ["write"],
                "canPersist": true
            }),
        ),
    );
    Ok(())
}

fn set_data_breakpoints(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let requested = request
        .arguments
        .get("breakpoints")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let mut installed = Vec::new();
    let response = requested
        .into_iter()
        .map(|value| {
            let data_id = value
                .get("dataId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let (thread, expression) = data_id
                .split_once('|')
                .and_then(|(thread, expression)| {
                    thread
                        .parse::<usize>()
                        .ok()
                        .and_then(|thread| thread.checked_sub(1))
                        .map(|thread| (thread, expression.to_owned()))
                })
                .unwrap_or((adapter.focus_cpu, data_id));
            let condition = value
                .get("condition")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let hit_condition = value
                .get("hitCondition")
                .and_then(Value::as_str)
                .map(str::to_owned);
            adapter.next_breakpoint_id += 1;
            let breakpoint = DataBreakpoint {
                id: adapter.next_breakpoint_id,
                thread,
                expression: expression.clone(),
                condition,
                hit_condition,
                hits: 0,
            };
            let expression_error = adapter
                .simulator
                .as_ref()
                .and_then(|simulator| {
                    evaluate_expression(simulator, thread, &expression)
                        .and_then(|value| value.number().map(|_| value))
                        .err()
                })
                .or_else(|| {
                    breakpoint.condition.as_deref().and_then(|condition| {
                        adapter.simulator.as_ref().and_then(|simulator| {
                            evaluate_expression(simulator, thread, condition).err()
                        })
                    })
                })
                .or_else(|| {
                    breakpoint
                        .hit_condition
                        .as_deref()
                        .and_then(|condition| validate_hit_condition(condition).err())
                });
            let verified = !expression.is_empty() && expression_error.is_none();
            if verified {
                installed.push(breakpoint.clone());
            }
            json!({
                "id": breakpoint.id,
                "verified": verified,
                "message": expression_error.map(|error| format!("Invalid data breakpoint: {error}"))
            })
        })
        .collect::<Vec<_>>();
    adapter.data_breakpoints = installed;
    output.response(request, json!({ "breakpoints": response }));
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
    let generated_line = cpu.current_line().unwrap_or(cpu.pc);
    let line = cpu.program.debug_line(generated_line) + 1;
    let path = cpu.program.debug_source_path.to_string_lossy();
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
                    "name": cpu.program.debug_source_path.file_name().and_then(|value| value.to_str()),
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
    let mut values = match kind {
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
    for value in &mut values {
        let Some(expression) = value.get("evaluateName").and_then(Value::as_str) else {
            continue;
        };
        let Some(current) = value
            .get("value")
            .and_then(Value::as_str)
            .and_then(|value| parse_debug_number(value).ok())
        else {
            continue;
        };
        if adapter
            .previous_values
            .get(&canonical_expression(expression))
            .is_some_and(|previous| !same_number(*previous, current))
        {
            value["presentationHint"] = json!({
                "kind": "property",
                "attributes": ["valueChanged"]
            });
        }
    }
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
            if !device.fields.contains_key(name) {
                return Err(format!("device `{}` has no field `{name}`", device.id));
            }
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
    let evaluated = evaluate_with_changed(simulator, thread, expression, &|target, current| {
        adapter
            .previous_values
            .get(&canonical_expression(target))
            .is_some_and(|previous| !same_number(*previous, current))
    })?;
    let (result, value_type) = match evaluated {
        Evaluation::Number(value) => (format_number(value), "number"),
        Evaluation::Boolean(value) => (value.to_string(), "boolean"),
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

fn exception_info(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = thread_index(&request.arguments)?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let exception = adapter
        .last_exception
        .as_ref()
        .filter(|exception| exception.thread == thread)
        .ok_or_else(|| "this thread is not stopped on a runtime exception".to_owned())?;
    output.response(
        request,
        json!({
            "exceptionId": exception.category,
            "description": exception.message,
            "breakMode": "always",
            "details": {
                "message": exception.message,
                "typeName": exception.category,
                "fullTypeName": format!("ic10.runtime.{}", exception.category)
            }
        }),
    );
    Ok(())
}

fn restart(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let arguments = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?
        .launch_arguments
        .clone()
        .ok_or_else(|| "no original launch configuration is available".to_owned())?;
    let restart_request = Request {
        seq: request.seq,
        command: request.command.clone(),
        arguments,
    };
    launch(state, output, &restart_request)?;
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.configured = true;
    if adapter.stop_on_entry {
        let focus = adapter.focus_cpu;
        adapter.last_stop = adapter
            .simulator
            .as_ref()
            .and_then(|simulator| simulator.cpus[focus].current_line())
            .map(|line| (focus, line));
        capture_previous_values(&mut adapter);
        output.stopped(
            "restart",
            focus,
            Some("Simulation reset to its original launch state."),
        );
    } else {
        adapter.running = true;
    }
    emit_breakpoint_updates(&adapter, output);
    Ok(())
}

fn hot_reload(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let preserve_state = request
        .arguments
        .get("preserveState")
        .and_then(Value::as_bool)
        .ok_or_else(|| "hot reload requires an explicit `preserveState` choice".to_owned())?;
    if !preserve_state {
        return restart(state, output, request);
    }
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    if adapter.running {
        return Err("pause the simulation before hot reloading".to_owned());
    }
    let focus = adapter.focus_cpu;
    let (last_stop, source_path) = {
        let simulator = adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        let mut programs = Vec::with_capacity(simulator.cpus.len());
        for cpu in &simulator.cpus {
            let source = std::fs::read_to_string(&cpu.program.source_path).map_err(|error| {
                format!(
                    "could not read `{}`: {error}",
                    cpu.program.source_path.display()
                )
            })?;
            programs.push(
                ic10_sim::Program::compile(
                    cpu.program.source_path.clone(),
                    source,
                    &simulator.knowledge,
                )
                .map_err(|error| format!("hot reload rejected: {error}"))?,
            );
        }
        for (cpu, program) in simulator.cpus.iter().zip(&programs) {
            if program.operation_at_or_after(cpu.pc).is_none() {
                return Err(format!(
                    "hot reload cannot preserve `{}` at line {} because the new program has no executable instruction there",
                    cpu.name,
                    cpu.pc + 1
                ));
            }
        }
        for (cpu, program) in simulator.cpus.iter_mut().zip(programs) {
            cpu.program = program;
        }
        (
            simulator.cpus[focus]
                .current_line()
                .map(|line| (focus, line)),
            simulator.cpus[focus].program.debug_source_path.clone(),
        )
    };
    adapter.last_stop = last_stop;
    emit_breakpoint_updates(&adapter, output);
    output.empty_response(request);
    output.event(
        "loadedSource",
        json!({ "reason": "changed", "source": { "path": source_path } }),
    );
    output.stopped(
        "restart",
        focus,
        Some("Source hot reloaded; CPU and world state preserved."),
    );
    Ok(())
}

fn goto_targets(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let path = request
        .arguments
        .pointer("/source/path")
        .and_then(Value::as_str)
        .ok_or_else(|| "gotoTargets requires source.path".to_owned())?;
    let requested_line = request
        .arguments
        .get("line")
        .and_then(Value::as_u64)
        .ok_or_else(|| "gotoTargets requires line".to_owned())? as usize;
    let key = normalize_path(Path::new(path));
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let mut targets = Vec::new();
    if let Some(simulator) = &adapter.simulator {
        for (thread, cpu) in simulator.cpus.iter().enumerate() {
            if normalize_path(&cpu.program.debug_source_path) != key {
                continue;
            }
            if let Some(generated) = cpu.program.generated_line(requested_line.saturating_sub(1))
                && cpu.program.operations.contains_key(&generated)
            {
                targets.push(json!({
                    "id": goto_target_id(thread, generated),
                    "label": format!("{}: line {requested_line}", cpu.name),
                    "line": requested_line
                }));
            }
        }
    }
    output.response(request, json!({ "targets": targets }));
    Ok(())
}

fn goto_location(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let target_id = request
        .arguments
        .get("targetId")
        .and_then(Value::as_u64)
        .ok_or_else(|| "goto requires targetId".to_owned())?;
    let (thread, line) = decode_goto_target(target_id)?;
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    if adapter.running {
        return Err("pause the simulation before running to a source location".to_owned());
    }
    let simulator = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let cpu = simulator
        .cpus
        .get_mut(thread)
        .ok_or_else(|| format!("unknown thread {}", thread + 1))?;
    if !cpu.program.operations.contains_key(&line) {
        return Err("goto target is no longer executable".to_owned());
    }
    cpu.pc = line;
    adapter.last_stop = Some((thread, line));
    output.empty_response(request);
    output.stopped("goto", thread, None);
    Ok(())
}

fn inline_values(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let thread = request
        .arguments
        .get("frameId")
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
    let Some(operation) = cpu.current_operation() else {
        output.response(request, json!({ "inlineValues": [] }));
        return Ok(());
    };
    let source_line = cpu.program.debug_line(operation.line);
    let mut expressions = operation
        .operands
        .iter()
        .filter(|operand| {
            evaluate_expression(simulator, thread, operand).is_ok()
                && (operand.starts_with('r')
                    || cpu.program.aliases.contains_key(*operand)
                    || cpu.program.defines.contains_key(*operand))
        })
        .cloned()
        .collect::<Vec<_>>();
    expressions.extend(["tick".to_owned(), "operationsThisTick".to_owned()]);
    expressions.sort();
    expressions.dedup();
    let values = expressions
        .into_iter()
        .map(|expression| {
            json!({
                "type": "variable",
                "range": {
                    "start": { "line": source_line, "column": 0 },
                    "end": { "line": source_line, "column": 1 }
                },
                "variableName": expression,
                "caseSensitiveLookup": true
            })
        })
        .collect::<Vec<_>>();
    output.response(request, json!({ "inlineValues": values }));
    Ok(())
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
    let single_thread = request
        .arguments
        .get("singleThread")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let thread = thread_index(&request.arguments).unwrap_or(adapter.focus_cpu);
    adapter.skip_breakpoint_once = adapter.last_stop.take();
    adapter.single_thread = single_thread.then_some(thread);
    adapter.running = true;
    output.response(
        request,
        json!({
            "allThreadsContinued": !single_thread
        }),
    );
    if single_thread {
        output.event(
            "output",
            json!({
                "category": "console",
                "output": "Warning: single-thread continue bypasses normal coordinated world scheduling.\n"
            }),
        );
    }
    output.event(
        "continued",
        json!({ "threadId": thread + 1, "allThreadsContinued": !single_thread }),
    );
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
    adapter.single_thread = None;
    apply_pending_test(&mut adapter)?;
    let (result, last_stop) = {
        let simulator = adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        let result = simulator.step_instruction(thread);
        let last_stop = simulator.cpus[thread]
            .current_line()
            .map(|line| (thread, line));
        (result, last_stop)
    };
    adapter.last_stop = last_stop;
    if let Err(error) = &result {
        adapter.last_exception = Some(LastException {
            category: exception_category(error).to_owned(),
            message: error.clone(),
            thread,
        });
    }
    capture_previous_values(&mut adapter);
    let description = result.as_ref().err().map(String::as_str);
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
    adapter.single_thread = None;
    capture_previous_values(&mut adapter);
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
    adapter.single_thread = None;
    apply_pending_test(&mut adapter)?;
    let (line, tick) = {
        let simulator = adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        simulator.step_world_tick()?;
        (simulator.cpus[thread].current_line(), simulator.tick)
    };
    adapter.last_stop = line.map(|line| (thread, line));
    capture_previous_values(&mut adapter);
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

enum BreakpointAction {
    Ignore,
    Stop,
    Log(String),
}

fn breakpoint_action(
    simulator: &Simulator,
    thread: usize,
    breakpoint: &SourceBreakpoint,
    hits: u64,
    previous: &HashMap<String, f64>,
) -> Result<BreakpointAction, String> {
    if !hit_condition_matches(breakpoint.hit_condition.as_deref(), hits)? {
        return Ok(BreakpointAction::Ignore);
    }
    if let Some(condition) = &breakpoint.condition
        && breakpoint.line != 0
        && !evaluate_with_changed(simulator, thread, condition, &|target, current| {
            previous
                .get(&canonical_expression(target))
                .is_some_and(|value| !same_number(*value, current))
        })?
        .truthy()?
    {
        return Ok(BreakpointAction::Ignore);
    }
    breakpoint
        .log_message
        .as_deref()
        .map_or(Ok(BreakpointAction::Stop), |message| {
            interpolate_log_message(simulator, thread, message, previous).map(BreakpointAction::Log)
        })
}

fn validate_expression_for_path(
    adapter: &AdapterState,
    path: &str,
    expression: &str,
) -> Result<(), String> {
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "launch the simulation before setting expression breakpoints".to_owned())?;
    let thread = simulator
        .cpus
        .iter()
        .position(|cpu| normalize_path(&cpu.program.debug_source_path) == path)
        .ok_or_else(|| "source is not loaded by this simulation".to_owned())?;
    evaluate_expression(simulator, thread, expression).map(|_| ())
}

fn validate_log_message(adapter: &AdapterState, path: &str, message: &str) -> Result<(), String> {
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "launch the simulation before setting logpoints".to_owned())?;
    let thread = simulator
        .cpus
        .iter()
        .position(|cpu| normalize_path(&cpu.program.debug_source_path) == path)
        .ok_or_else(|| "source is not loaded by this simulation".to_owned())?;
    interpolate_log_message(simulator, thread, message, &HashMap::new()).map(|_| ())
}

fn validate_hit_condition(condition: &str) -> Result<(), String> {
    hit_condition_matches(Some(condition), 1).map(|_| ())
}

fn hit_condition_matches(condition: Option<&str>, hits: u64) -> Result<bool, String> {
    let Some(condition) = condition.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(true);
    };
    let (operator, number) = [">=", "<=", "==", "!=", ">", "<", "%"]
        .into_iter()
        .find_map(|operator| {
            condition
                .strip_prefix(operator)
                .map(|rest| (operator, rest.trim()))
        })
        .unwrap_or(("==", condition));
    let number = number
        .parse::<u64>()
        .map_err(|_| format!("`{condition}` must be a count such as `5`, `>= 3`, or `% 2`"))?;
    Ok(match operator {
        "==" => hits == number,
        "!=" => hits != number,
        ">=" => hits >= number,
        "<=" => hits <= number,
        ">" => hits > number,
        "<" => hits < number,
        "%" => hits.is_multiple_of(number),
        _ => false,
    })
}

fn interpolate_log_message(
    simulator: &Simulator,
    thread: usize,
    message: &str,
    previous: &HashMap<String, f64>,
) -> Result<String, String> {
    let mut rendered = String::new();
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        rendered.push_str(&rest[..open]);
        let expression = &rest[open + 1..];
        let close = expression
            .find('}')
            .ok_or_else(|| "logpoint has an unmatched `{`".to_owned())?;
        let expression_text = expression[..close].trim();
        if expression_text.is_empty() {
            return Err("logpoint interpolation cannot be empty".to_owned());
        }
        rendered.push_str(
            &evaluate_with_changed(simulator, thread, expression_text, &|target, current| {
                previous
                    .get(&canonical_expression(target))
                    .is_some_and(|value| !same_number(*value, current))
            })?
            .display(),
        );
        rest = &expression[close + 1..];
    }
    if rest.contains('}') {
        return Err("logpoint has an unmatched `}`".to_owned());
    }
    rendered.push_str(rest);
    Ok(rendered)
}

fn update_breakpoint_hits(adapter: &mut AdapterState, id: u64, hits: u64) {
    for breakpoint in adapter
        .breakpoints
        .values_mut()
        .flatten()
        .chain(adapter.function_breakpoints.iter_mut())
    {
        if breakpoint.id == id {
            breakpoint.hits = hits;
            return;
        }
    }
}

fn emit_breakpoint_updates(adapter: &AdapterState, output: &Output) {
    let Some(simulator) = &adapter.simulator else {
        return;
    };
    for (path, breakpoints) in &adapter.breakpoints {
        let executable = simulator
            .cpus
            .iter()
            .filter(|cpu| normalize_path(&cpu.program.debug_source_path) == *path)
            .flat_map(|cpu| {
                cpu.program
                    .operations
                    .keys()
                    .map(|line| cpu.program.debug_line(*line) + 1)
            })
            .collect::<BTreeSet<_>>();
        for breakpoint in breakpoints {
            let verified = executable.contains(&breakpoint.line);
            output.event(
                "breakpoint",
                json!({
                    "reason": "changed",
                    "breakpoint": {
                        "id": breakpoint.id,
                        "verified": verified,
                        "line": breakpoint.line,
                        "source": { "path": path },
                        "message": (!verified).then_some("No executable IC10 instruction exists on this line.")
                    }
                }),
            );
        }
    }
    for breakpoint in &adapter.function_breakpoints {
        let name = breakpoint.condition.as_deref().unwrap_or("");
        let verified = simulator
            .cpus
            .iter()
            .any(|cpu| cpu.program.labels.contains_key(name));
        output.event(
            "breakpoint",
            json!({
                "reason": "changed",
                "breakpoint": {
                    "id": breakpoint.id,
                    "verified": verified,
                    "message": (!verified).then(|| format!("Unknown IC10 label `{name}`."))
                }
            }),
        );
    }
}

fn data_values(
    simulator: &Simulator,
    _thread: usize,
    breakpoints: &[DataBreakpoint],
) -> HashMap<u64, f64> {
    breakpoints
        .iter()
        .filter_map(|breakpoint| {
            evaluate_expression(simulator, breakpoint.thread, &breakpoint.expression)
                .ok()
                .and_then(|value| value.number().ok())
                .map(|value| (breakpoint.id, value))
        })
        .collect()
}

fn changed_data_breakpoint(
    simulator: &Simulator,
    _thread: usize,
    before: &HashMap<u64, f64>,
    breakpoints: &mut [DataBreakpoint],
    previous: &HashMap<String, f64>,
) -> Option<String> {
    for breakpoint in breakpoints {
        let current = evaluate_expression(simulator, breakpoint.thread, &breakpoint.expression)
            .ok()?
            .number()
            .ok()?;
        if before
            .get(&breakpoint.id)
            .is_none_or(|value| same_number(*value, current))
        {
            continue;
        }
        breakpoint.hits += 1;
        if !hit_condition_matches(breakpoint.hit_condition.as_deref(), breakpoint.hits).ok()? {
            continue;
        }
        if let Some(condition) = &breakpoint.condition
            && !evaluate_with_changed(simulator, breakpoint.thread, condition, &|target, value| {
                previous
                    .get(&canonical_expression(target))
                    .is_some_and(|old| !same_number(*old, value))
            })
            .ok()?
            .truthy()
            .ok()?
        {
            continue;
        }
        return Some(breakpoint.expression.clone());
    }
    None
}

fn exception_category(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("device") || lower.contains("pin") {
        "device"
    } else if lower.contains("address") || lower.contains("stack") || lower.contains("memory") {
        "address"
    } else if lower.contains("access") || lower.contains("writable") || lower.contains("readable") {
        "access"
    } else {
        "instruction"
    }
}

fn expression_for_variable(
    simulator: &Simulator,
    kind: u64,
    _thread: usize,
    index: usize,
    name: &str,
) -> Option<String> {
    match kind {
        REGISTERS_SCOPE => Some(name.to_owned()),
        STACK_SCOPE => Some(format!("stack[{}]", name.trim_matches(['[', ']']))),
        DEVICE_SCOPE => simulator
            .world
            .devices
            .get(index)
            .map(|device| format!("device(\"{}\").{name}", device.id)),
        DEVICE_SLOT_SCOPE => {
            let (device, slot) = decode_composite_index(index);
            simulator
                .world
                .devices
                .get(device)
                .map(|device| format!("device(\"{}\").slot[{slot}].{name}", device.id))
        }
        DEVICE_MEMORY_SCOPE => simulator.world.devices.get(index).map(|device| {
            format!(
                "device(\"{}\").memory[{}]",
                device.id,
                name.trim_matches(['[', ']'])
            )
        }),
        NETWORK_SCOPE => simulator
            .world
            .networks
            .get(index)
            .map(|network| format!("network(\"{}\").{name}", network.id)),
        _ => None,
    }
}

fn snapshot_values(simulator: &Simulator) -> HashMap<String, f64> {
    let mut values = HashMap::new();
    for cpu in &simulator.cpus {
        for index in 0..REGISTER_COUNT {
            let name = match index {
                RETURN_ADDRESS_REGISTER => "ra".to_owned(),
                STACK_POINTER_REGISTER => "sp".to_owned(),
                _ => format!("r{index}"),
            };
            values.entry(name).or_insert(cpu.registers[index]);
        }
        for (address, value) in cpu.stack.iter().enumerate() {
            values.insert(format!("stack[{address}]"), *value);
        }
    }
    for device in &simulator.world.devices {
        for (field, value) in &device.fields {
            values.insert(format!("device(\"{}\").{field}", device.id), *value);
        }
        for (slot, fields) in &device.slots {
            for (field, value) in fields {
                values.insert(
                    format!("device(\"{}\").slot[{slot}].{field}", device.id),
                    *value,
                );
            }
        }
        for (address, value) in device.memory.iter().enumerate() {
            values.insert(
                format!("device(\"{}\").memory[{address}]", device.id),
                *value,
            );
        }
    }
    for network in &simulator.world.networks {
        for (channel, value) in network.channels.iter().enumerate() {
            values.insert(
                format!("network(\"{}\").Channel{channel}", network.id),
                *value,
            );
        }
    }
    values
}

fn capture_previous_values(adapter: &mut AdapterState) {
    let Some(simulator) = adapter.simulator.as_ref() else {
        return;
    };
    adapter.previous_values = std::mem::take(&mut adapter.stop_values);
    adapter.stop_values = snapshot_values(simulator);
}

fn canonical_expression(expression: &str) -> String {
    expression
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn same_number(left: f64, right: f64) -> bool {
    (left.is_nan() && right.is_nan()) || left.to_bits() == right.to_bits()
}

fn goto_target_id(thread: usize, line: usize) -> u64 {
    ((thread as u64 + 1) << 32) | line as u64
}

fn decode_goto_target(id: u64) -> Result<(usize, usize), String> {
    let thread = ((id >> 32) as usize)
        .checked_sub(1)
        .ok_or_else(|| "invalid goto target".to_owned())?;
    Ok((thread, (id & u32::MAX as u64) as usize))
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
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use ic10_runner::{load_expanded_case, set_value};
    use ic10_sim::Simulator;

    use super::{
        DEVICE_MEMORY_SCOPE, DEVICE_SLOT_SCOPE, apply_test_tick, composite_index, decode_reference,
        reference,
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

    #[test]
    fn debug_test_plan_applies_initial_state_timeline_and_assertions() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("examples/scenario-tests/airlock/airlock.ic10test.json");
        let (scenario, seed, test_case) =
            load_expanded_case(&fixture, "opens after the chamber is depressurised").unwrap();
        let mut simulator = Simulator::from_scenario_path(&scenario).unwrap();
        simulator.set_seed(seed);
        for (target, value) in &test_case.initial {
            set_value(&mut simulator, 0, target, value.as_f64().unwrap()).unwrap();
        }
        let mut satisfied = BTreeSet::new();
        for _ in 0..=3 {
            apply_test_tick(&mut simulator, 0, &test_case, &mut satisfied).unwrap();
            if simulator.tick < 3 {
                simulator.step_world_tick().unwrap();
            }
        }
        let exterior = simulator
            .world
            .device_index("exterior-door")
            .map(|index| simulator.world.devices[index].fields["On"])
            .unwrap();
        assert_eq!(exterior, 1.0);
        assert!(satisfied.contains(&1));
    }
}
