use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use ic10_runner::{
    TestCase, Value as Evaluation, evaluate as evaluate_expression, evaluate_with_changed,
    load_expanded_case, set_value,
};
use ic10_sim::{
    EffectActor, EffectBatch, EffectTarget, REGISTER_COUNT, RETURN_ADDRESS_REGISTER,
    STACK_POINTER_REGISTER, Simulator, SimulatorSnapshot, channel_index, direct_register_index,
};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum ReplayAction {
    Scheduled,
    Instruction { cpu: usize },
    WorldTick,
    External,
    Stop,
}

#[derive(Clone, Debug, Default)]
struct ReplayOutcome {
    error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteDelta {
    target: String,
    before: String,
    after: String,
    before_bits: u64,
    after_bits: u64,
    attempted: bool,
}

#[derive(Clone, Debug)]
struct TraceRecord {
    sequence: u64,
    tick: u64,
    cpu: usize,
    replay_line: usize,
    line: usize,
    source_id: u32,
    action: ReplayAction,
    event_flags: u16,
    effects: EffectBatch,
    outcome: ReplayOutcome,
    state_hash: u64,
}

const EVENT_INSTRUCTION: u16 = 1 << 0;
const EVENT_TICK: u16 = 1 << 1;
const EVENT_YIELD: u16 = 1 << 2;
const EVENT_SLEEP: u16 = 1 << 3;
const EVENT_ERROR: u16 = 1 << 4;
const EVENT_HALT: u16 = 1 << 5;
const EVENT_BREAKPOINT: u16 = 1 << 6;
const EVENT_DATA_BREAKPOINT: u16 = 1 << 7;
const EVENT_EXCEPTION: u16 = 1 << 8;
const EVENT_ASSERTION: u16 = 1 << 9;
const EVENT_STIMULUS: u16 = 1 << 10;
const EVENT_DEBUGGER: u16 = 1 << 11;

fn event_flag(name: &str) -> u16 {
    match name {
        "instruction" => EVENT_INSTRUCTION,
        "tick" => EVENT_TICK,
        "yield" => EVENT_YIELD,
        "sleep" => EVENT_SLEEP,
        "error" => EVENT_ERROR,
        "halt" => EVENT_HALT,
        "breakpoint" => EVENT_BREAKPOINT,
        "data breakpoint" => EVENT_DATA_BREAKPOINT,
        "exception" => EVENT_EXCEPTION,
        "assertion" => EVENT_ASSERTION,
        "stimulus" => EVENT_STIMULUS,
        "debugger" => EVENT_DEBUGGER,
        _ => 0,
    }
}

fn event_names(flags: u16) -> Vec<&'static str> {
    [
        ("instruction", EVENT_INSTRUCTION),
        ("tick", EVENT_TICK),
        ("yield", EVENT_YIELD),
        ("sleep", EVENT_SLEEP),
        ("error", EVENT_ERROR),
        ("halt", EVENT_HALT),
        ("breakpoint", EVENT_BREAKPOINT),
        ("data breakpoint", EVENT_DATA_BREAKPOINT),
        ("exception", EVENT_EXCEPTION),
        ("assertion", EVENT_ASSERTION),
        ("stimulus", EVENT_STIMULUS),
        ("debugger", EVENT_DEBUGGER),
    ]
    .into_iter()
    .filter_map(|(name, flag)| (flags & flag != 0).then_some(name))
    .collect()
}

struct TraceCheckpoint {
    sequence: u64,
    snapshot: SimulatorSnapshot,
    estimated_bytes: usize,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceProfile {
    instructions_by_ic: BTreeMap<String, u64>,
    operations_by_tick: BTreeMap<u64, u64>,
    device_reads: u64,
    device_writes: u64,
    network_reads: u64,
    network_writes: u64,
    maximum_stack_pointer: BTreeMap<String, usize>,
    unchanged_writes: BTreeMap<String, u64>,
    oscillating_values: BTreeSet<String>,
    budget_ceiling_ticks: BTreeSet<u64>,
    branch_outcomes: BTreeMap<String, u64>,
}

struct TraceHistory {
    max_events: usize,
    max_memory_bytes: usize,
    checkpoint_interval: usize,
    records: VecDeque<TraceRecord>,
    checkpoints: VecDeque<TraceCheckpoint>,
    cursor: u64,
    next_sequence: u64,
    dropped: u64,
    sources: Vec<String>,
    memory_estimate: usize,
}

impl TraceHistory {
    fn new(
        simulator: &Simulator,
        max_events: usize,
        checkpoint_interval: usize,
        memory_mib: usize,
    ) -> Self {
        let mut checkpoints = VecDeque::new();
        checkpoints.push_back(TraceCheckpoint {
            sequence: 0,
            snapshot: simulator.snapshot(),
            estimated_bytes: estimate_snapshot_bytes(simulator),
        });
        let checkpoint_bytes = checkpoints[0].estimated_bytes;
        Self {
            max_events: max_events.max(2),
            max_memory_bytes: memory_mib.max(1).saturating_mul(1024 * 1024),
            checkpoint_interval: checkpoint_interval.max(1).min(max_events.max(2)),
            records: VecDeque::new(),
            checkpoints,
            cursor: 0,
            next_sequence: 1,
            dropped: 0,
            sources: simulator
                .cpus
                .iter()
                .map(|cpu| normalize_path(&cpu.program.debug_source_path))
                .collect(),
            memory_estimate: checkpoint_bytes,
        }
    }

    fn record(
        &mut self,
        simulator: &Simulator,
        cpu: usize,
        line: usize,
        action: ReplayAction,
        effects: EffectBatch,
        error: Option<String>,
    ) {
        if self.cursor + 1 < self.next_sequence {
            self.records.retain(|record| record.sequence <= self.cursor);
            self.checkpoints
                .retain(|checkpoint| checkpoint.sequence <= self.cursor);
            self.next_sequence = self.cursor + 1;
            self.recompute_memory_estimate();
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.cursor = sequence;
        let mut event_flags = EVENT_INSTRUCTION;
        if self
            .records
            .back()
            .is_some_and(|record| record.tick != simulator.tick)
        {
            event_flags |= EVENT_TICK;
        }
        if let Some(item) = simulator.cpus.get(cpu) {
            if let Some(operation) = item.program.operations.get(&line) {
                match operation.mnemonic.as_str() {
                    "yield" => event_flags |= EVENT_YIELD,
                    "sleep" => event_flags |= EVENT_SLEEP,
                    _ => {}
                }
            }
            if matches!(item.state, ic10_sim::CpuState::Error) {
                event_flags |= EVENT_ERROR;
            }
            if matches!(item.state, ic10_sim::CpuState::Halted) {
                event_flags |= EVENT_HALT;
            }
        }
        let record = TraceRecord {
            sequence,
            tick: simulator.tick,
            cpu,
            replay_line: line,
            line: simulator
                .cpus
                .get(cpu)
                .map_or(line + 1, |item| item.program.debug_line(line) + 1),
            source_id: cpu as u32,
            action,
            event_flags,
            effects,
            outcome: ReplayOutcome { error },
            state_hash: simulator.state_hash(),
        };
        self.memory_estimate = self
            .memory_estimate
            .saturating_add(Self::record_bytes(&record));
        let requires_checkpoint = matches!(record.action, ReplayAction::External);
        self.records.push_back(record);
        if (sequence as usize).is_multiple_of(self.checkpoint_interval) || requires_checkpoint {
            self.checkpoints.push_back(TraceCheckpoint {
                sequence,
                snapshot: simulator.snapshot(),
                estimated_bytes: estimate_snapshot_bytes(simulator),
            });
            self.memory_estimate = self.memory_estimate.saturating_add(self.checkpoint_bytes());
        }
        if self.records.len() > self.max_events {
            let desired_first = self.records[self.records.len() - self.max_events].sequence;
            if let Some(baseline) = self
                .checkpoints
                .iter()
                .find(|checkpoint| {
                    checkpoint.sequence >= desired_first
                        && checkpoint.sequence
                            > self.checkpoints.front().map_or(0, |item| item.sequence)
                })
                .map(|checkpoint| checkpoint.sequence)
            {
                self.drop_through(baseline);
            } else {
                self.checkpoints.push_back(TraceCheckpoint {
                    sequence,
                    snapshot: simulator.snapshot(),
                    estimated_bytes: estimate_snapshot_bytes(simulator),
                });
                self.drop_through(sequence);
            }
            self.recompute_memory_estimate();
        }
        while self.estimated_memory_bytes() > self.max_memory_bytes {
            if self.checkpoints.len() == 1 && self.records.is_empty() {
                // A single valid checkpoint is the irreducible history cost.
                // Report its estimate rather than spinning when a huge world
                // cannot fit beneath the requested soft cap.
                break;
            }
            let Some(next_checkpoint) = self.checkpoints.get(1).map(|item| item.sequence) else {
                // Preserve a valid anchor even under an extremely small cap.
                self.checkpoints.push_back(TraceCheckpoint {
                    sequence: self.cursor,
                    snapshot: simulator.snapshot(),
                    estimated_bytes: estimate_snapshot_bytes(simulator),
                });
                self.recompute_memory_estimate();
                continue;
            };
            self.drop_through(next_checkpoint);
            self.recompute_memory_estimate();
        }
    }

    fn drop_through(&mut self, baseline: u64) {
        while self
            .records
            .front()
            .is_some_and(|record| record.sequence <= baseline)
        {
            self.records.pop_front();
            self.dropped += 1;
        }
        while self.checkpoints.len() > 1
            && self
                .checkpoints
                .get(1)
                .is_some_and(|checkpoint| checkpoint.sequence <= baseline)
        {
            self.checkpoints.pop_front();
        }
    }

    fn restore(&mut self, simulator: &mut Simulator, target: u64) -> Result<(), String> {
        let checkpoint = self
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.sequence <= target)
            .ok_or_else(|| "requested state is no longer retained".to_owned())?;
        simulator.restore(&checkpoint.snapshot)?;
        for record in self
            .records
            .iter()
            .filter(|record| record.sequence > checkpoint.sequence && record.sequence <= target)
        {
            let external = matches!(record.action, ReplayAction::External);
            match record.action {
                ReplayAction::Scheduled => {
                    while simulator.tick < record.tick {
                        if simulator.next_scheduled_location().is_some() {
                            return Err(format!(
                                "replay expected a tick anchor before trace event {}",
                                record.sequence
                            ));
                        }
                    }
                    let location = simulator.next_scheduled_location().ok_or_else(|| {
                        format!(
                            "replay found no instruction for trace event {}",
                            record.sequence
                        )
                    })?;
                    if location != (record.cpu, record.replay_line) {
                        return Err(format!(
                            "replay location diverged at trace event {}",
                            record.sequence
                        ));
                    }
                    let result = simulator.scheduler_step();
                    replay_result_matches(record, result.map(|value| value.is_some()))?;
                }
                ReplayAction::Instruction { cpu } => {
                    replay_result_matches(record, simulator.step_instruction(cpu).map(|_| true))?;
                }
                ReplayAction::WorldTick => {
                    replay_result_matches(record, simulator.step_world_tick().map(|_| true))?;
                }
                ReplayAction::External => {
                    return Err(format!(
                        "external trace event {} is missing its checkpoint",
                        record.sequence
                    ));
                }
                ReplayAction::Stop => {}
            }
            let actual = simulator.take_effects();
            let actual_writes = actual
                .writes
                .iter()
                .map(|write| {
                    (
                        simulator.effect_target_name(&write.target),
                        write.before_bits,
                        write.after_bits,
                    )
                })
                .collect::<Vec<_>>();
            let expected_writes = record
                .effects
                .writes
                .iter()
                .map(|write| {
                    (
                        simulator.effect_target_name(&write.target),
                        write.before_bits,
                        write.after_bits,
                    )
                })
                .collect::<Vec<_>>();
            if !external && actual_writes != expected_writes {
                return Err(format!(
                    "deterministic replay effects diverged at trace event {}",
                    record.sequence
                ));
            }
            let actual_hash = simulator.state_hash();
            if actual_hash != record.state_hash {
                return Err(format!(
                    "deterministic replay state diverged at trace event {}: expected {:016x}, got {:016x}",
                    record.sequence, record.state_hash, actual_hash
                ));
            }
        }
        self.cursor = target;
        Ok(())
    }

    fn previous_write<'a>(
        &'a self,
        simulator: &Simulator,
        target: &str,
    ) -> Option<(&'a TraceRecord, &'a ic10_sim::WriteEffect)> {
        self.records
            .iter()
            .rev()
            .filter(|record| record.sequence <= self.cursor)
            .find_map(|record| {
                record
                    .effects
                    .writes
                    .iter()
                    .find(|write| simulator.effect_target_name(&write.target) == target)
                    .map(|write| (record, write))
            })
    }

    fn mark_stop(&mut self, reason: &str) {
        if let Some(record) = self.records.back_mut() {
            record.event_flags |= event_flag(reason);
        }
    }

    fn record_stop(&mut self, simulator: &Simulator, cpu: usize, replay_line: usize, reason: &str) {
        self.record(
            simulator,
            cpu,
            replay_line,
            ReplayAction::Stop,
            EffectBatch::default(),
            None,
        );
        if let Some(record) = self.records.back_mut() {
            record.event_flags = event_flag(reason);
        }
    }

    fn checkpoint_current(&mut self, simulator: &Simulator) {
        if self
            .checkpoints
            .back()
            .is_some_and(|checkpoint| checkpoint.sequence == self.cursor)
        {
            self.checkpoints.pop_back();
        }
        self.checkpoints.push_back(TraceCheckpoint {
            sequence: self.cursor,
            snapshot: simulator.snapshot(),
            estimated_bytes: estimate_snapshot_bytes(simulator),
        });
        self.recompute_memory_estimate();
    }

    fn estimated_memory_bytes(&self) -> usize {
        self.memory_estimate
    }

    fn record_bytes(record: &TraceRecord) -> usize {
        96 + record.effects.reads.capacity() * std::mem::size_of::<ic10_sim::ReadEffect>()
            + record.effects.writes.capacity() * std::mem::size_of::<ic10_sim::WriteEffect>()
            + record
                .outcome
                .error
                .as_ref()
                .map_or(0, |value| value.capacity())
    }

    fn checkpoint_bytes(&self) -> usize {
        self.checkpoints
            .back()
            .map_or(0, |checkpoint| checkpoint.estimated_bytes)
    }

    fn recompute_memory_estimate(&mut self) {
        self.memory_estimate = self
            .records
            .iter()
            .map(Self::record_bytes)
            .sum::<usize>()
            .saturating_add(
                self.checkpoints
                    .iter()
                    .map(|checkpoint| checkpoint.estimated_bytes)
                    .sum::<usize>(),
            );
        self.memory_estimate = self.memory_estimate.saturating_add(
            self.sources
                .iter()
                .map(|source| std::mem::size_of::<String>() + source.capacity())
                .sum::<usize>(),
        );
    }
}

fn replay_result_matches<T>(record: &TraceRecord, result: Result<T, String>) -> Result<(), String> {
    match (record.outcome.error.as_deref(), result) {
        (None, Ok(_)) => Ok(()),
        (Some(expected), Err(actual)) if expected == actual => Ok(()),
        (Some(expected), Err(actual)) => Err(format!(
            "replay error diverged at trace event {}: expected `{expected}`, got `{actual}`",
            record.sequence
        )),
        (Some(expected), Ok(_)) => Err(format!(
            "replay unexpectedly succeeded at trace event {} (expected `{expected}`)",
            record.sequence
        )),
        (None, Err(actual)) => Err(format!(
            "replay unexpectedly failed at trace event {}: {actual}",
            record.sequence
        )),
    }
}

fn estimate_snapshot_bytes(simulator: &Simulator) -> usize {
    let mut bytes = 8 * 1024;
    for cpu in &simulator.cpus {
        bytes += (REGISTER_COUNT + cpu.stack.capacity()) * std::mem::size_of::<f64>();
        bytes += cpu.error.as_ref().map_or(0, |value| value.capacity());
    }
    for network in &simulator.world.networks {
        bytes +=
            256 + network.id.capacity() + network.kind.capacity() + network.cable_role.capacity();
    }
    for device in &simulator.world.devices {
        bytes += 512
            + device.id.capacity()
            + device.prefab.capacity()
            + device.name.capacity()
            + device.memory.capacity() * std::mem::size_of::<f64>();
        bytes += device
            .fields
            .keys()
            .map(|key| 64 + key.capacity())
            .sum::<usize>();
        bytes += device
            .slots
            .values()
            .flat_map(|fields| fields.keys())
            .map(|key| 80 + key.capacity())
            .sum::<usize>();
        bytes += device.connections.len() * 64;
    }
    bytes += format!("{:?}", simulator.behaviour_runtime()).len() * 2 + 4096;
    bytes += simulator
        .test_driver_state("scenario.scripted")
        .map_or(0, |state| state.len() * 2 + 256);
    // Include cloned World lookup tables and allocator/container overhead with
    // a conservative factor rather than presenting payload bytes as RSS.
    bytes.saturating_mul(2)
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
    history: Option<TraceHistory>,
}

fn spawn_runner(
    state: Arc<Mutex<AdapterState>>,
    output: Output,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut overlay_reads = VecDeque::<Value>::new();
        let mut overlay_writes = VecDeque::<Value>::new();
        let mut overlay_dropped = 0_u64;
        let mut overlay_sequence = 0_u64;
        let mut overlay_last_emit = Instant::now();
        while !shutdown.load(Ordering::SeqCst) {
            let mut stopped = None;
            let mut terminated = false;
            let mut trace_batch = None;
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
                            if let Some(history) = adapter.history.as_mut() {
                                let effects = simulator.take_effects();
                                let line = simulator.cpus[test_thread]
                                    .current_line()
                                    .unwrap_or_default();
                                history.record(
                                    &simulator,
                                    test_thread,
                                    line,
                                    ReplayAction::External,
                                    effects,
                                    None,
                                );
                                history.mark_stop("stimulus");
                            }
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
                        let replay_action = if adapter.single_thread == Some(cpu) {
                            ReplayAction::Instruction { cpu }
                        } else {
                            ReplayAction::Scheduled
                        };
                        let effects = simulator.take_effects();
                        append_topology_effects(
                            &simulator,
                            cpu,
                            line,
                            &effects,
                            &mut overlay_sequence,
                            &mut overlay_reads,
                            &mut overlay_writes,
                            &mut overlay_dropped,
                        );
                        if let Some(history) = adapter.history.as_mut() {
                            history.record(
                                &simulator,
                                cpu,
                                line,
                                replay_action,
                                effects,
                                step_result.as_ref().err().cloned(),
                            );
                        }
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
                if let Some((stop_cpu, reason, _)) = &stopped
                    && let Some(history) = adapter.history.as_mut()
                {
                    let stop_line = simulator
                        .cpus
                        .get(*stop_cpu)
                        .and_then(|cpu| cpu.current_line())
                        .unwrap_or_default();
                    history.record_stop(&simulator, *stop_cpu, stop_line, reason);
                }
                adapter.simulator = Some(simulator);
                if (!overlay_reads.is_empty() || !overlay_writes.is_empty())
                    && (overlay_reads.len() + overlay_writes.len() >= 64
                        || overlay_last_emit.elapsed() >= Duration::from_millis(50)
                        || stopped.is_some()
                        || terminated)
                {
                    let scenario_id = adapter
                        .launch_arguments
                        .as_ref()
                        .and_then(|arguments| arguments.get("scenario"))
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    trace_batch = Some(json!({
                        "type": "traceBatch",
                        "scenarioId": scenario_id,
                        "sequence": overlay_sequence,
                        "dropped": overlay_dropped,
                        "reads": overlay_reads.drain(..).collect::<Vec<_>>(),
                        "writes": overlay_writes.drain(..).collect::<Vec<_>>(),
                        "ics": topology_ic_states(
                            adapter.simulator.as_ref().expect("simulator restored")
                        )
                    }));
                    overlay_dropped = 0;
                    overlay_last_emit = Instant::now();
                }
                if capture_stop {
                    capture_previous_values(&mut adapter);
                }
            }
            if let Some(batch) = trace_batch {
                output.event("ic10/traceBatch", batch);
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
                    "supportsStepBack": true,
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
        "stepBack" => step_back(state, output, &request),
        "reverseContinue" => reverse_continue(state, output, &request),
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
        "ic10/getTrace" => get_trace(state, output, &request),
        "ic10/getTopologyState" => get_topology_state(state, output, &request),
        "ic10/previousWrite" => previous_write(state, output, &request),
        "ic10/navigateHistory" => navigate_history(state, output, &request),
        "ic10/stateDiff" => state_diff(state, output, &request),
        "ic10/exportTrace" => export_trace(state, output, &request),
        "ic10/importTrace" => import_trace(output, &request),
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
    let lua_library_paths = request
        .arguments
        .get("luaLibraryPaths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let mut simulator =
        Simulator::from_scenario_path_with_lua_library_paths(&scenario_path, &lua_library_paths)
            .map_err(|error| error.to_string())?;
    if let Some((_, seed, _)) = selected_test.as_ref() {
        simulator.set_seed(*seed);
    }
    let compatibility_warnings = simulator.compatibility_warnings.clone();
    // `focusIc` is the legacy device selector.  Neutral test files use
    // `program`, and the debug request surface also accepts `focusProgram`.
    // Keep the legacy request alias while resolving either a device id or a
    // stable program id so old and canonical files select the same CPU.
    let focus_id = request
        .arguments
        .get("focusProgram")
        .and_then(Value::as_str)
        .or_else(|| request.arguments.get("program").and_then(Value::as_str))
        .or_else(|| request.arguments.get("focusIc").and_then(Value::as_str))
        .or_else(|| {
            selected_test
                .as_ref()
                .and_then(|(_, _, test_case)| test_case.program.as_deref())
        });
    let focus_cpu = if let Some(selector) = focus_id {
        simulator
            .cpus
            .iter()
            .position(|cpu| cpu.id == selector || cpu.program_id == selector)
            .ok_or_else(|| {
                format!("simulation does not contain a runnable program or device with stable ID `{selector}`")
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
    let history_enabled = request
        .arguments
        .get("enableHistory")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    adapter.history = history_enabled.then(|| {
        let max_events = request
            .arguments
            .get("historyEvents")
            .and_then(Value::as_u64)
            .unwrap_or(20_000) as usize;
        let checkpoint_interval = request
            .arguments
            .get("checkpointInterval")
            .and_then(Value::as_u64)
            .unwrap_or(10_000) as usize;
        let memory_mib = request
            .arguments
            .get("historyMemoryMiB")
            .and_then(Value::as_u64)
            .unwrap_or(64) as usize;
        TraceHistory::new(
            adapter.simulator.as_ref().expect("simulator loaded"),
            max_events,
            checkpoint_interval,
            memory_mib,
        )
    });
    adapter
        .simulator
        .as_mut()
        .expect("simulator loaded")
        // Topology overlays consume the same bounded execution journal even
        // when reversible history is disabled. Effects are drained and
        // coalesced continuously, so this never accumulates an unbounded log.
        .set_journaling(true);
    adapter
        .simulator
        .as_mut()
        .expect("simulator loaded")
        .take_effects();
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
    let tracing = adapter.history.is_some();
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
    if tracing {
        let simulator = adapter.simulator.as_mut().expect("simulator loaded");
        let effects = simulator.take_effects();
        let line = simulator.cpus[adapter.test_thread]
            .current_line()
            .unwrap_or_default();
        let mut history = adapter.history.take().expect("history checked");
        history.record(
            adapter.simulator.as_ref().expect("simulator loaded"),
            adapter.test_thread,
            line,
            ReplayAction::External,
            effects,
            None,
        );
        history.mark_stop("stimulus");
        adapter.history = Some(history);
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
    let tracing = adapter.history.is_some();
    let simulator = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    match kind {
        REGISTERS_SCOPE => {
            let register =
                direct_register_index(name).ok_or_else(|| format!("invalid register `{name}`"))?;
            simulator.set_register_as(thread, register, parsed, EffectActor::Debugger)?;
        }
        STACK_SCOPE => {
            let address = name
                .trim_matches(|character| character == '[' || character == ']')
                .parse::<usize>()
                .map_err(|_| format!("invalid stack address `{name}`"))?;
            simulator.set_stack_as(thread, address, parsed, EffectActor::Debugger)?;
        }
        DEVICE_SCOPE => {
            let device_id = simulator
                .world
                .devices
                .get(index)
                .ok_or_else(|| format!("unknown device index {index}"))?
                .id
                .clone();
            simulator.set_device_field_as(&device_id, name, parsed, EffectActor::Debugger)?;
        }
        DEVICE_SLOT_SCOPE => {
            let (device_index, slot_index) = decode_composite_index(index);
            simulator.set_device_slot_as(
                device_index,
                slot_index,
                name,
                parsed,
                EffectActor::Debugger,
            )?;
        }
        DEVICE_MEMORY_SCOPE => {
            let address = name
                .trim_matches(|character| character == '[' || character == ']')
                .parse::<usize>()
                .map_err(|_| format!("invalid device memory address `{name}`"))?;
            simulator.set_device_memory_as(index, address, parsed, EffectActor::Debugger)?;
        }
        NETWORK_SCOPE => {
            let channel = channel_index(name).ok_or_else(|| format!("invalid channel `{name}`"))?;
            let network = simulator
                .world
                .networks
                .get(index)
                .ok_or_else(|| format!("unknown network index {index}"))?;
            if !network.kind.eq_ignore_ascii_case("cable") {
                return Err(format!("network `{}` does not expose channels", network.id));
            }
            simulator.set_network_channel_as(index, channel, parsed, EffectActor::Debugger)?;
        }
        _ => return Err("variables in this scope are not editable".to_owned()),
    }
    let line = simulator
        .cpus
        .get(thread)
        .and_then(|cpu| cpu.current_line())
        .unwrap_or_default();
    if tracing {
        let effects = simulator.take_effects();
        if let Some(mut history) = adapter.history.take() {
            history.record(
                adapter.simulator.as_ref().expect("simulator loaded"),
                thread,
                line,
                ReplayAction::External,
                effects,
                None,
            );
            history.mark_stop("debugger");
            adapter.history = Some(history);
        }
    }
    if let Some(mut history) = adapter.history.take() {
        history.checkpoint_current(adapter.simulator.as_ref().expect("simulator loaded"));
        adapter.history = Some(history);
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
    if let Some(history) = adapter.history.take() {
        adapter.history = Some(TraceHistory::new(
            adapter.simulator.as_ref().expect("simulator loaded"),
            history.max_events,
            history.checkpoint_interval,
            history.max_memory_bytes / (1024 * 1024),
        ));
    }
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

fn step_back(
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
    let target = adapter
        .history
        .as_ref()
        .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?
        .cursor
        .checked_sub(1)
        .ok_or_else(|| "already at the oldest retained state".to_owned())?;
    let mut history = adapter.history.take().expect("history checked");
    let restore = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())
        .and_then(|simulator| history.restore(simulator, target));
    adapter.history = Some(history);
    restore?;
    reconcile_reversible_bookkeeping(&mut adapter);
    adapter.last_stop = adapter
        .simulator
        .as_ref()
        .and_then(|simulator| simulator.cpus.get(thread))
        .and_then(|cpu| cpu.current_line())
        .map(|line| (thread, line));
    capture_previous_values(&mut adapter);
    output.empty_response(request);
    output.stopped(
        "step",
        thread,
        Some("Restored the previous retained instruction."),
    );
    Ok(())
}

fn reverse_continue(
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
    let history = adapter
        .history
        .as_ref()
        .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?;
    let target = history
        .records
        .iter()
        .rev()
        .find(|record| {
            record.sequence < history.cursor
                && record.event_flags
                    & (EVENT_BREAKPOINT
                        | EVENT_DATA_BREAKPOINT
                        | EVENT_EXCEPTION
                        | EVENT_ASSERTION
                        | EVENT_YIELD
                        | EVENT_SLEEP
                        | EVENT_TICK)
                    != 0
        })
        .map(|record| record.sequence)
        .unwrap_or(0);
    let mut history = adapter.history.take().expect("history checked");
    let restore = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())
        .and_then(|simulator| history.restore(simulator, target));
    adapter.history = Some(history);
    restore?;
    reconcile_reversible_bookkeeping(&mut adapter);
    adapter.last_stop = adapter
        .simulator
        .as_ref()
        .and_then(|simulator| simulator.cpus.get(thread))
        .and_then(|cpu| cpu.current_line())
        .map(|line| (thread, line));
    capture_previous_values(&mut adapter);
    output.empty_response(request);
    output.stopped(
        "step",
        thread,
        Some("Reverse-continued to the previous source location or tick."),
    );
    Ok(())
}

fn record_source<'a>(history: &'a TraceHistory, record: &TraceRecord) -> &'a str {
    history
        .sources
        .get(record.source_id as usize)
        .map(String::as_str)
        .unwrap_or_default()
}

fn write_delta(simulator: &Simulator, write: &ic10_sim::WriteEffect) -> WriteDelta {
    WriteDelta {
        target: simulator.effect_target_name(&write.target),
        before: format_number(f64::from_bits(write.before_bits)),
        after: format_number(f64::from_bits(write.after_bits)),
        before_bits: write.before_bits,
        after_bits: write.after_bits,
        attempted: write.attempted,
    }
}

fn effect_actor_json(simulator: &Simulator, actor: &EffectActor) -> Value {
    match actor {
        EffectActor::Ic { cpu, source, line } => {
            let source = simulator
                .resolve_journal_symbol(*source)
                .unwrap_or_default();
            json!({
                "kind": "ic",
                "ic": simulator.cpus.get(*cpu).map(|item| item.id.as_str()),
                "source": source,
                "line": simulator.cpus.get(*cpu)
                    .map(|item| item.program.debug_line(*line) + 1)
            })
        }
        EffectActor::Behaviour {
            device,
            model,
            version,
        } => json!({
            "kind": "behaviour",
            "device": simulator.world.devices.get(*device).map(|item| item.id.as_str()),
            "model": simulator.resolve_journal_symbol(*model),
            "version": version
        }),
        EffectActor::ScriptedDriver { driver, rule } => json!({
            "kind": "scriptedDriver",
            "driver": simulator.resolve_journal_symbol(*driver),
            "rule": rule
        }),
        EffectActor::Scenario => json!({ "kind": "scenario" }),
        EffectActor::Debugger => json!({ "kind": "debugger" }),
        EffectActor::Scheduler => json!({ "kind": "scheduler" }),
    }
}

fn record_json(history: &TraceHistory, simulator: &Simulator, record: &TraceRecord) -> Value {
    json!({
        "sequence": record.sequence,
        "tick": record.tick,
        "cpu": record.cpu,
        "line": record.line,
        "source": record_source(history, record),
        "action": record.action,
        "eventTypes": event_names(record.event_flags),
        "reads": record.effects.reads.iter()
            .map(|read| json!({
                "target": simulator.effect_target_name(&read.target),
                "value": format_number(f64::from_bits(read.value_bits)),
                "actor": effect_actor_json(simulator, &read.actor)
            }))
            .collect::<Vec<_>>(),
        "writes": record.effects.writes.iter()
            .map(|write| {
                let delta = write_delta(simulator, write);
                json!({
                    "target": delta.target,
                    "before": delta.before,
                    "after": delta.after,
                    "beforeBits": delta.before_bits,
                    "afterBits": delta.after_bits,
                    "attempted": delta.attempted,
                    "actor": effect_actor_json(simulator, &write.actor)
                })
            })
            .collect::<Vec<_>>(),
    })
}

#[allow(clippy::too_many_arguments)]
fn append_topology_effects(
    simulator: &Simulator,
    cpu: usize,
    line: usize,
    effects: &EffectBatch,
    sequence: &mut u64,
    reads: &mut VecDeque<Value>,
    writes: &mut VecDeque<Value>,
    dropped: &mut u64,
) {
    for read in &effects.reads {
        *sequence = sequence.saturating_add(1);
        let (source_id, source_path, cpu_id, source_line) =
            topology_actor(simulator, &read.actor, cpu, line);
        let (target_id, target_kind, field) = topology_target(simulator, &read.target);
        reads.push_back(json!({
            "sequence": *sequence,
            "tick": simulator.tick,
            "sourceId": source_id,
            "sourcePath": source_path,
            "line": source_line,
            "cpuId": cpu_id,
            "targetId": target_id,
            "targetKind": target_kind,
            "field": field,
            "value": format_number(f64::from_bits(read.value_bits))
        }));
    }
    for write in &effects.writes {
        *sequence = sequence.saturating_add(1);
        let (source_id, source_path, cpu_id, source_line) =
            topology_actor(simulator, &write.actor, cpu, line);
        let (target_id, target_kind, field) = topology_target(simulator, &write.target);
        writes.push_back(json!({
            "sequence": *sequence,
            "tick": simulator.tick,
            "sourceId": source_id,
            "sourcePath": source_path,
            "line": source_line,
            "cpuId": cpu_id,
            "targetId": target_id,
            "targetKind": target_kind,
            "field": field,
            "before": format_number(f64::from_bits(write.before_bits)),
            "after": format_number(f64::from_bits(write.after_bits))
        }));
    }
    const MAX_PENDING_EFFECTS: usize = 256;
    while reads.len() + writes.len() > MAX_PENDING_EFFECTS {
        if reads.len() >= writes.len() {
            reads.pop_front();
        } else {
            writes.pop_front();
        }
        *dropped = dropped.saturating_add(1);
    }
}

fn emit_topology_effects_immediately(
    adapter: &AdapterState,
    output: &Output,
    cpu: usize,
    line: usize,
    effects: &EffectBatch,
) {
    if effects.reads.is_empty() && effects.writes.is_empty() {
        return;
    }
    let Some(simulator) = adapter.simulator.as_ref() else {
        return;
    };
    let mut reads = VecDeque::new();
    let mut writes = VecDeque::new();
    let mut dropped = 0;
    let mut sequence = output.sequence.load(Ordering::Relaxed);
    append_topology_effects(
        simulator,
        cpu,
        line,
        effects,
        &mut sequence,
        &mut reads,
        &mut writes,
        &mut dropped,
    );
    let scenario_id = adapter
        .launch_arguments
        .as_ref()
        .and_then(|arguments| arguments.get("scenario"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    output.event(
        "ic10/traceBatch",
        json!({
            "type": "traceBatch",
            "scenarioId": scenario_id,
            "sequence": sequence,
            "dropped": dropped,
            "reads": reads,
            "writes": writes,
            "ics": topology_ic_states(simulator)
        }),
    );
}

fn topology_ic_states(simulator: &Simulator) -> BTreeMap<String, Value> {
    simulator
        .cpus
        .iter()
        .map(|cpu| {
            (
                cpu.id.clone(),
                json!({
                    "runState": format!("{:?}", cpu.state),
                    "sourceId": normalize_path(&cpu.program.debug_source_path),
                    "sourcePath": cpu.program.debug_source_path.clone(),
                    "line": cpu.current_line().map(|line| cpu.program.debug_line(line) + 1)
                }),
            )
        })
        .collect()
}

fn topology_actor(
    simulator: &Simulator,
    actor: &EffectActor,
    fallback_cpu: usize,
    fallback_line: usize,
) -> (String, Option<String>, Option<String>, Option<usize>) {
    match actor {
        EffectActor::Ic { cpu, source, line } => {
            let source_path = simulator
                .resolve_journal_symbol(*source)
                .unwrap_or_default()
                .to_owned();
            (
                normalize_path(Path::new(&source_path)),
                Some(source_path),
                simulator.cpus.get(*cpu).map(|value| value.id.clone()),
                simulator
                    .cpus
                    .get(*cpu)
                    .map(|value| value.program.debug_line(*line) + 1),
            )
        }
        EffectActor::Behaviour { device, model, .. } => (
            format!(
                "behaviour:{}:{}",
                simulator
                    .world
                    .devices
                    .get(*device)
                    .map(|value| value.id.as_str())
                    .unwrap_or("<unknown>"),
                simulator
                    .resolve_journal_symbol(*model)
                    .unwrap_or("<unknown>")
            ),
            None,
            None,
            None,
        ),
        other => (
            format!("{other:?}"),
            simulator.cpus.get(fallback_cpu).map(|value| {
                value
                    .program
                    .debug_source_path
                    .to_string_lossy()
                    .into_owned()
            }),
            simulator
                .cpus
                .get(fallback_cpu)
                .map(|value| value.id.clone()),
            simulator
                .cpus
                .get(fallback_cpu)
                .map(|value| value.program.debug_line(fallback_line) + 1),
        ),
    }
}

fn topology_target(
    simulator: &Simulator,
    target: &EffectTarget,
) -> (String, &'static str, Option<String>) {
    match target {
        EffectTarget::DeviceField { device, field } => (
            simulator.world.devices[*device].id.clone(),
            "device",
            simulator.resolve_journal_symbol(*field).map(str::to_owned),
        ),
        EffectTarget::DeviceSlot {
            device,
            slot,
            field,
        } => (
            simulator.world.devices[*device].id.clone(),
            "device",
            Some(format!(
                "slot[{slot}].{}",
                simulator
                    .resolve_journal_symbol(*field)
                    .unwrap_or("<unknown>")
            )),
        ),
        EffectTarget::DeviceMemory { device, address } => (
            simulator.world.devices[*device].id.clone(),
            "device",
            Some(format!("memory[{address}]")),
        ),
        EffectTarget::NetworkChannel { network, channel } => (
            simulator.world.networks[*network].id.clone(),
            "network",
            Some(format!("Channel{channel}")),
        ),
        EffectTarget::Register { cpu, register } => (
            simulator.cpus[*cpu].id.clone(),
            "register",
            Some(format!("r{register}")),
        ),
        EffectTarget::Stack { cpu, address } => (
            simulator.cpus[*cpu].id.clone(),
            "stack",
            Some(format!("stack[{address}]")),
        ),
        _ => (simulator.effect_target_name(target), "other", None),
    }
}

fn trace_analysis(
    history: &TraceHistory,
    simulator: &Simulator,
) -> (BTreeMap<String, BTreeSet<usize>>, TraceProfile) {
    let mut coverage = BTreeMap::<String, BTreeSet<usize>>::new();
    let mut profile = TraceProfile::default();
    let mut values = BTreeMap::<EffectTarget, (Option<u64>, Option<u64>)>::new();
    let mut cpu_tick_operations = BTreeMap::<(u64, usize), u64>::new();
    for cpu in &simulator.cpus {
        profile.maximum_stack_pointer.insert(
            cpu.id.clone(),
            cpu.registers[STACK_POINTER_REGISTER].max(0.0) as usize,
        );
    }
    for record in &history.records {
        let source = record_source(history, record);
        coverage
            .entry(source.to_owned())
            .or_default()
            .insert(record.line);
        *profile.operations_by_tick.entry(record.tick).or_default() += 1;
        *cpu_tick_operations
            .entry((record.tick, record.cpu))
            .or_default() += 1;
        if let Some(cpu) = simulator.cpus.get(record.cpu) {
            *profile
                .instructions_by_ic
                .entry(cpu.id.clone())
                .or_default() += 1;
            if let Some(operation) = cpu.program.operations.get(&record.replay_line)
                && operation.mnemonic.starts_with('b')
            {
                let fallthrough = cpu
                    .program
                    .operation_at_or_after(record.replay_line.saturating_add(1))
                    .map(|next| next.line);
                let actual_pc = record.effects.writes.iter().rev().find_map(|write| {
                    matches!(write.target, EffectTarget::CpuPc { cpu } if cpu == record.cpu)
                        .then_some(write.after_bits as usize)
                });
                let outcome = if actual_pc == fallthrough {
                    "not-taken"
                } else {
                    "taken"
                };
                *profile
                    .branch_outcomes
                    .entry(format!("{source}:{}:{outcome}", record.line))
                    .or_default() += 1;
            }
        }
        for read in &record.effects.reads {
            match read.target {
                EffectTarget::DeviceField { .. }
                | EffectTarget::DeviceSlot { .. }
                | EffectTarget::DeviceMemory { .. } => profile.device_reads += 1,
                EffectTarget::NetworkChannel { .. } => profile.network_reads += 1,
                _ => {}
            }
        }
        for write in &record.effects.writes {
            match write.target {
                EffectTarget::DeviceField { .. }
                | EffectTarget::DeviceSlot { .. }
                | EffectTarget::DeviceMemory { .. } => profile.device_writes += 1,
                EffectTarget::NetworkChannel { .. } => profile.network_writes += 1,
                EffectTarget::Register { cpu, register }
                    if register == STACK_POINTER_REGISTER as u8 =>
                {
                    let id = simulator
                        .cpus
                        .get(cpu)
                        .map(|cpu| cpu.id.clone())
                        .unwrap_or_else(|| format!("cpu-{cpu}"));
                    let value = f64::from_bits(write.after_bits).max(0.0) as usize;
                    profile
                        .maximum_stack_pointer
                        .entry(id)
                        .and_modify(|maximum| *maximum = (*maximum).max(value))
                        .or_insert(value);
                }
                _ => {}
            }
            let target = simulator.effect_target_name(&write.target);
            if write.before_bits == write.after_bits {
                *profile.unchanged_writes.entry(target.clone()).or_default() += 1;
            }
            let entry = values.entry(write.target.clone()).or_default();
            if entry.0 == Some(write.after_bits) && entry.1 != Some(write.after_bits) {
                profile.oscillating_values.insert(target);
            }
            entry.0 = entry.1;
            entry.1 = Some(write.after_bits);
        }
    }
    let ceiling = u64::from(
        simulator
            .knowledge
            .language
            .architecture
            .maximum_instructions_per_tick,
    );
    profile.budget_ceiling_ticks.extend(
        cpu_tick_operations
            .iter()
            .filter_map(|((tick, _), operations)| (*operations >= ceiling).then_some(*tick)),
    );
    (coverage, profile)
}

fn trace_payload(
    history: &TraceHistory,
    simulator: &Simulator,
    redact: bool,
    offset: usize,
    limit: Option<usize>,
    summary_only: bool,
    include_analysis: bool,
) -> Value {
    let retained_from = history.records.front().map_or(0, |record| record.sequence);
    let retained_to = history.records.back().map_or(0, |record| record.sequence);
    let (coverage, mut profile) = if include_analysis {
        trace_analysis(history, simulator)
    } else {
        (BTreeMap::new(), TraceProfile::default())
    };
    let mut redacted_sources = BTreeMap::new();
    if redact {
        for (index, source) in coverage.keys().enumerate() {
            let extension = Path::new(source)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| format!(".{value}"))
                .unwrap_or_default();
            redacted_sources.insert(
                source.clone(),
                format!("source-{:04}{extension}", index + 1),
            );
        }
        profile.branch_outcomes = profile
            .branch_outcomes
            .into_iter()
            .map(|(key, count)| {
                let redacted = redacted_sources
                    .iter()
                    .find_map(|(source, id)| {
                        key.strip_prefix(source)
                            .map(|suffix| format!("{id}{suffix}"))
                    })
                    .unwrap_or(key);
                (redacted, count)
            })
            .collect();
    }
    let available = history.records.len().saturating_sub(offset);
    let returned = limit.unwrap_or(available).min(available);
    let records = if summary_only {
        Vec::new()
    } else {
        history
            .records
            .iter()
            .skip(offset)
            .take(returned)
            .map(|record| {
                let mut value = record_json(history, simulator, record);
                if redact {
                    value["source"] = Value::from(
                        redacted_sources
                            .get(record_source(history, record))
                            .cloned()
                            .unwrap_or_else(|| "source-unknown".to_owned()),
                    );
                    for collection in ["reads", "writes"] {
                        if let Some(items) = value[collection].as_array_mut() {
                            for item in items {
                                let Some(source) =
                                    item["actor"]["source"].as_str().map(str::to_owned)
                                else {
                                    continue;
                                };
                                item["actor"]["source"] = Value::from(
                                    redacted_sources
                                        .get(&source)
                                        .cloned()
                                        .unwrap_or_else(|| "source-unknown".to_owned()),
                                );
                            }
                        }
                    }
                }
                value
            })
            .collect::<Vec<_>>()
    };
    let coverage = coverage
        .iter()
        .map(|(source, lines)| {
            (
                if redact {
                    redacted_sources
                        .get(source)
                        .cloned()
                        .unwrap_or_else(|| "source-unknown".to_owned())
                } else {
                    source.clone()
                },
                lines,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let source_ids = if redact {
        Value::Array(
            redacted_sources
                .values()
                .cloned()
                .map(Value::String)
                .collect(),
        )
    } else {
        serde_json::to_value(
            history
                .sources
                .iter()
                .enumerate()
                .map(|(index, source)| (index.to_string(), source))
                .collect::<BTreeMap<_, _>>(),
        )
        .expect("source identifiers serialize")
    };
    json!({
        "schemaVersion": 1,
        "toolVersion": env!("CARGO_PKG_VERSION"),
        "gameDataVersion": simulator.knowledge.language.game_version,
        "pathsRedacted": redact,
        "analysisIncluded": include_analysis,
        "history": {
            "cursor": history.cursor,
            "retainedFrom": retained_from,
            "retainedTo": retained_to,
            "retainedEvents": history.records.len(),
            "eventLimit": history.max_events,
            "checkpointInterval": history.checkpoint_interval,
            "checkpoints": history.checkpoints.len(),
            "estimatedMemoryBytes": history.estimated_memory_bytes(),
            "memoryLimitBytes": history.max_memory_bytes,
            "droppedEvents": history.dropped,
            "retainedTicks": history.records.back().map_or(0, |last| {
                last.tick.saturating_sub(history.records.front().map_or(last.tick, |first| first.tick))
            })
        },
        "records": records,
        "page": {
            "offset": offset,
            "returned": if summary_only { 0 } else { returned },
            "total": history.records.len(),
            "hasMore": !summary_only && offset.saturating_add(returned) < history.records.len()
        },
        "sourceIds": source_ids,
        "coverage": coverage,
        "profile": profile
    })
}

fn get_trace(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let history = adapter
        .history
        .as_ref()
        .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let total = history.records.len();
    let tail = request
        .arguments
        .get("tail")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let offset = tail
        .map(|count| total.saturating_sub(count))
        .unwrap_or_else(|| {
            request
                .arguments
                .get("offset")
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize
        });
    let limit = request
        .arguments
        .get("limit")
        .and_then(Value::as_u64)
        .map(|value| value as usize);
    let summary_only = request
        .arguments
        .get("summaryOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let include_analysis = request
        .arguments
        .get("includeAnalysis")
        .and_then(Value::as_bool)
        .unwrap_or(
            summary_only
                || (tail.is_none() && request.arguments.get("offset").is_none() && limit.is_none()),
        );
    output.response(
        request,
        trace_payload(
            history,
            simulator,
            false,
            offset,
            limit,
            summary_only,
            include_analysis,
        ),
    );
    Ok(())
}

fn previous_write(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let target = request
        .arguments
        .get("target")
        .and_then(Value::as_str)
        .ok_or_else(|| "previousWrite requires a `target`".to_owned())?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let history = adapter
        .history
        .as_ref()
        .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?;
    let (record, write) = history
        .previous_write(simulator, target)
        .ok_or_else(|| format!("no retained write to `{target}`"))?;
    let delta = write_delta(simulator, write);
    let actor = effect_actor_json(simulator, &write.actor);
    output.response(
        request,
        json!({
            "sequence": record.sequence,
            "tick": record.tick,
            "ic": actor.get("ic"),
            "source": actor.get("source"),
            "line": actor.get("line"),
            "actor": actor,
            "before": delta.before,
            "after": delta.after
        }),
    );
    Ok(())
}

fn navigate_history(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let direction = request
        .arguments
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("previous");
    let target_value = request.arguments.get("target").and_then(Value::as_str);
    let event_type = request.arguments.get("eventType").and_then(Value::as_str);
    let mut adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    adapter.running = false;
    let history = adapter
        .history
        .as_ref()
        .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?;
    let simulator = adapter
        .simulator
        .as_ref()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    let matches_record = |record: &TraceRecord| {
        target_value.is_none_or(|target| {
            record
                .effects
                .writes
                .iter()
                .any(|write| simulator.effect_target_name(&write.target) == target)
        }) && event_type.is_none_or(|kind| record.event_flags & event_flag(kind) != 0)
    };
    let candidate = if direction == "next" {
        history
            .records
            .iter()
            .find(|record| record.sequence > history.cursor && matches_record(record))
    } else {
        history
            .records
            .iter()
            .rev()
            .find(|record| record.sequence < history.cursor && matches_record(record))
    };
    let target = candidate
        .map(|record| (record.sequence, record.cpu))
        .ok_or_else(|| "no matching retained history event".to_owned())?;
    let mut history = adapter.history.take().expect("history checked");
    let restore = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())
        .and_then(|simulator| history.restore(simulator, target.0));
    adapter.history = Some(history);
    restore?;
    reconcile_reversible_bookkeeping(&mut adapter);
    adapter.focus_cpu = target.1;
    adapter.last_stop = adapter
        .simulator
        .as_ref()
        .and_then(|simulator| simulator.cpus.get(target.1))
        .and_then(|cpu| cpu.current_line())
        .map(|line| (target.1, line));
    capture_previous_values(&mut adapter);
    output.response(request, json!({ "sequence": target.0 }));
    output.stopped("step", target.1, Some("Navigated retained IC10 history."));
    Ok(())
}

fn state_diff(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let from = request
        .arguments
        .get("from")
        .and_then(Value::as_u64)
        .ok_or_else(|| "stateDiff requires `from`".to_owned())?;
    let to = request
        .arguments
        .get("to")
        .and_then(Value::as_u64)
        .ok_or_else(|| "stateDiff requires `to`".to_owned())?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let history = adapter
        .history
        .as_ref()
        .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?;
    for endpoint in [from, to] {
        let retained = history
            .records
            .iter()
            .any(|record| record.sequence == endpoint)
            || history
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.sequence == endpoint);
        if !retained {
            return Err(format!(
                "stateDiff event {endpoint} is not a retained restorable state"
            ));
        }
    }
    let (low, high) = if from <= to { (from, to) } else { (to, from) };
    let mut changes: BTreeMap<String, (String, String)> = BTreeMap::new();
    for record in history
        .records
        .iter()
        .filter(|record| record.sequence > low && record.sequence <= high)
    {
        for write in &record.effects.writes {
            let write = write_delta(
                adapter
                    .simulator
                    .as_ref()
                    .ok_or_else(|| "no simulation is loaded".to_owned())?,
                write,
            );
            changes
                .entry(write.target.clone())
                .and_modify(|value| value.1.clone_from(&write.after))
                .or_insert_with(|| (write.before.clone(), write.after.clone()));
        }
    }
    let reverse = from > to;
    let changes = changes
        .into_iter()
        .map(|(target, (before, after))| {
            let (before, after) = if reverse {
                (after, before)
            } else {
                (before, after)
            };
            json!({ "target": target, "before": before, "after": after })
        })
        .collect::<Vec<_>>();
    output.response(
        request,
        json!({ "from": from, "to": to, "changes": changes }),
    );
    Ok(())
}

fn export_trace(
    state: &Arc<Mutex<AdapterState>>,
    output: &Output,
    request: &Request,
) -> Result<(), String> {
    let path = request
        .arguments
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "exportTrace requires a `path`".to_owned())?;
    let adapter = state
        .lock()
        .map_err(|_| "debug state poisoned".to_owned())?;
    let payload = trace_payload(
        adapter
            .history
            .as_ref()
            .ok_or_else(|| "reversible history is disabled for this launch".to_owned())?,
        adapter
            .simulator
            .as_ref()
            .ok_or_else(|| "no simulation is loaded".to_owned())?,
        true,
        0,
        None,
        false,
        true,
    );
    let serialized = serde_json::to_vec_pretty(&payload).map_err(|error| error.to_string())?;
    std::fs::write(path, serialized)
        .map_err(|error| format!("could not export trace to `{path}`: {error}"))?;
    output.response(request, json!({ "path": path }));
    Ok(())
}

fn import_trace(output: &Output, request: &Request) -> Result<(), String> {
    let path = request
        .arguments
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| "importTrace requires a `path`".to_owned())?;
    let bytes =
        std::fs::read(path).map_err(|error| format!("could not read trace `{path}`: {error}"))?;
    let payload: Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("invalid trace: {error}"))?;
    if payload.get("schemaVersion").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported trace schema version".to_owned());
    }
    output.response(
        request,
        json!({
            "toolVersion": payload.get("toolVersion"),
            "gameDataVersion": payload.get("gameDataVersion"),
            "history": payload.get("history"),
            "coverage": payload.get("coverage"),
            "profile": payload.get("profile"),
            "records": payload.get("records")
        }),
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
    let (result, last_stop, executed_line) = {
        let simulator = adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        let executed_line = simulator.cpus[thread].current_line().unwrap_or_default();
        let result = simulator.step_instruction(thread);
        let last_stop = simulator.cpus[thread]
            .current_line()
            .map(|line| (thread, line));
        (result, last_stop, executed_line)
    };
    let effects = adapter
        .simulator
        .as_mut()
        .expect("simulator loaded")
        .take_effects();
    emit_topology_effects_immediately(&adapter, output, thread, executed_line, &effects);
    if let Some(mut history) = adapter.history.take() {
        history.record(
            adapter.simulator.as_ref().expect("simulator loaded"),
            thread,
            executed_line,
            ReplayAction::Instruction { cpu: thread },
            effects.clone(),
            result.as_ref().err().cloned(),
        );
        adapter.history = Some(history);
    }
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
    let (line, tick, executed_line) = {
        let simulator = adapter
            .simulator
            .as_mut()
            .ok_or_else(|| "no simulation is loaded".to_owned())?;
        let executed_line = simulator.cpus[thread].current_line().unwrap_or_default();
        simulator.step_world_tick()?;
        (
            simulator.cpus[thread].current_line(),
            simulator.tick,
            executed_line,
        )
    };
    let effects = adapter
        .simulator
        .as_mut()
        .expect("simulator loaded")
        .take_effects();
    emit_topology_effects_immediately(&adapter, output, thread, executed_line, &effects);
    if let Some(mut history) = adapter.history.take() {
        history.record(
            adapter.simulator.as_ref().expect("simulator loaded"),
            thread,
            executed_line,
            ReplayAction::WorldTick,
            effects.clone(),
            None,
        );
        adapter.history = Some(history);
    }
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

fn get_topology_state(
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
    let scenario_id = adapter
        .launch_arguments
        .as_ref()
        .and_then(|arguments| arguments.get("scenario"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let devices = simulator
        .world
        .devices
        .iter()
        .enumerate()
        .map(|(index, device)| {
            let behaviour = simulator.behaviour(index).cloned().unwrap_or_else(|| {
                ic10_sim::BehaviourDescriptor {
                    model: "passive".to_owned(),
                    version: 1,
                    kind: ic10_sim::BehaviourKind::Passive,
                    modelled: false,
                    dependencies: Vec::new(),
                }
            });
            (
                device.id.clone(),
                json!({
                    "behaviour": behaviour,
                    "fields": device.fields.iter()
                        .map(|(name, value)| (name.clone(), format_number(*value)))
                        .collect::<BTreeMap<_, _>>()
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let networks = simulator
        .world
        .networks
        .iter()
        .map(|network| {
            (
                network.id.clone(),
                json!({
                    "channels": network.channels.iter().enumerate()
                        .map(|(index, value)| (format!("Channel{index}"), format_number(*value)))
                        .collect::<BTreeMap<_, _>>()
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let ics = simulator
        .cpus
        .iter()
        .map(|cpu| {
            (
                cpu.id.clone(),
                json!({
                    "runState": format!("{:?}", cpu.state),
                    "sourceId": normalize_path(&cpu.program.debug_source_path),
                    "sourcePath": cpu.program.debug_source_path.clone(),
                    "line": cpu.current_line().map(|line| cpu.program.debug_line(line) + 1)
                }),
            )
        })
        .collect::<BTreeMap<_, _>>();
    output.response(
        request,
        json!({
            "scenarioId": scenario_id,
            "tick": simulator.tick,
            "devices": devices,
            "networks": networks,
            "ics": ics,
            "behaviourCatalog": ic10_sim::behaviour_catalog()
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
    let tracing = adapter.history.is_some();
    let simulator = adapter
        .simulator
        .as_mut()
        .ok_or_else(|| "no simulation is loaded".to_owned())?;
    if simulator.cpus.get(thread).is_none() {
        return Err(format!("unknown thread {}", thread + 1));
    }
    if let Some(registers) = request
        .arguments
        .get("registers")
        .and_then(Value::as_object)
    {
        for (name, value) in registers {
            let register =
                direct_register_index(name).ok_or_else(|| format!("invalid register `{name}`"))?;
            simulator.set_register_as(
                thread,
                register,
                debug_value(value)?,
                EffectActor::Debugger,
            )?;
        }
    }
    if let Some(stack) = request.arguments.get("stack").and_then(Value::as_object) {
        for (address, value) in stack {
            let address = address
                .parse::<usize>()
                .map_err(|_| format!("invalid stack address `{address}`"))?;
            simulator.set_stack_as(thread, address, debug_value(value)?, EffectActor::Debugger)?;
        }
    }
    let line = simulator.cpus[thread].current_line().unwrap_or_default();
    if tracing {
        let effects = simulator.take_effects();
        let mut history = adapter.history.take().expect("history checked");
        history.record(
            adapter.simulator.as_ref().expect("simulator loaded"),
            thread,
            line,
            ReplayAction::External,
            effects,
            None,
        );
        history.mark_stop("debugger");
        adapter.history = Some(history);
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

fn reconcile_reversible_bookkeeping(adapter: &mut AdapterState) {
    let Some(history) = adapter.history.as_ref() else {
        return;
    };
    let Some(simulator) = adapter.simulator.as_ref() else {
        return;
    };
    let cursor = history.cursor;
    for (source, breakpoints) in &mut adapter.breakpoints {
        let source = normalize_path(Path::new(source));
        for breakpoint in breakpoints {
            breakpoint.hits = history
                .records
                .iter()
                .filter(|record| {
                    record.sequence <= cursor
                        && record_source(history, record) == source
                        && record.line == breakpoint.line
                })
                .count() as u64;
        }
    }
    for breakpoint in &mut adapter.function_breakpoints {
        breakpoint.hits = history
            .records
            .iter()
            .filter(|record| record.sequence <= cursor && record.line == breakpoint.line)
            .count() as u64;
    }
    for breakpoint in &mut adapter.data_breakpoints {
        breakpoint.hits = history
            .records
            .iter()
            .filter(|record| {
                record.sequence <= cursor
                    && record.effects.writes.iter().any(|write| {
                        simulator.effect_target_name(&write.target) == breakpoint.expression
                    })
            })
            .count() as u64;
    }
    adapter.test_tick_applied = None;
    // "Eventually" assertions are path-dependent. Do not retain satisfaction
    // earned in the abandoned future; forward execution rebuilds the set.
    adapter.test_satisfied.clear();
    adapter.last_exception = None;
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
        "pinf" | "Infinity" | "+Infinity" | "inf" | "+inf" => Ok(f64::INFINITY),
        "ninf" | "-Infinity" | "-inf" => Ok(f64::NEG_INFINITY),
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
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::path::PathBuf;
    use std::time::Instant;

    use ic10_runner::{load_expanded_case, set_value};
    use ic10_sim::{Scenario, Simulator};

    use super::{
        DEVICE_MEMORY_SCOPE, DEVICE_SLOT_SCOPE, ReplayAction, TraceHistory,
        append_topology_effects, apply_test_tick, composite_index, decode_reference, reference,
        trace_payload,
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
            .join("examples/scenario-tests/airlock/airlock.ictest");
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

    #[test]
    fn bounded_history_restores_and_reports_previous_writes() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("crates/ic10-sim/tests/fixtures/multi-ic.icsim");
        let mut simulator = Simulator::from_scenario_path(&fixture).unwrap();
        simulator.set_journaling(true);
        let mut history = TraceHistory::new(&simulator, 3, 2, 8);
        let (cpu, line) = simulator.next_scheduled_location().unwrap();
        simulator.scheduler_step().unwrap();
        let effects = simulator.take_effects();
        history.record(
            &simulator,
            cpu,
            line,
            ReplayAction::Scheduled,
            effects,
            None,
        );
        let written_target =
            simulator.effect_target_name(&history.records.back().unwrap().effects.writes[0].target);
        assert!(
            history
                .previous_write(&simulator, &written_target)
                .is_some()
        );
        let mut hashes = BTreeMap::new();
        hashes.insert(1, simulator.state_hash());
        for sequence in 2..=5 {
            let (cpu, line) = simulator.next_scheduled_location().unwrap();
            simulator.scheduler_step().unwrap();
            let effects = simulator.take_effects();
            history.record(
                &simulator,
                cpu,
                line,
                ReplayAction::Scheduled,
                effects,
                None,
            );
            hashes.insert(sequence, simulator.state_hash());
        }
        assert_eq!(history.records.len(), 3);
        assert_eq!(history.dropped, 2);

        history.restore(&mut simulator, 2).unwrap();
        assert_eq!(simulator.state_hash(), hashes[&2]);
        history.restore(&mut simulator, 5).unwrap();
        assert_eq!(simulator.state_hash(), hashes[&5]);
    }

    #[test]
    fn topology_effect_batches_are_bounded_and_use_stable_ids() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("crates/ic10-sim/tests/fixtures/multi-ic.icsim");
        let mut simulator = Simulator::from_scenario_path(&fixture).unwrap();
        simulator.set_journaling(true);
        for _ in 0..300 {
            simulator.scheduler_step().unwrap();
        }
        let effects = simulator.take_effects();
        let mut reads = VecDeque::new();
        let mut writes = VecDeque::new();
        let mut sequence = 0;
        let mut dropped = 0;
        append_topology_effects(
            &simulator,
            0,
            0,
            &effects,
            &mut sequence,
            &mut reads,
            &mut writes,
            &mut dropped,
        );
        assert!(reads.len() + writes.len() <= 256);
        assert!(dropped > 0);
        assert!(reads.iter().chain(writes.iter()).all(|entry| {
            entry["sourceId"].is_string()
                && entry["targetId"].is_string()
                && entry["targetKind"].is_string()
        }));
    }

    #[test]
    fn trace_payload_pages_records_and_redacts_colliding_basenames() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("crates/ic10-sim/tests/fixtures/multi-ic.icsim");
        let mut simulator = Simulator::from_scenario_path(&fixture).unwrap();
        simulator.set_journaling(true);
        let mut history = TraceHistory::new(&simulator, 20, 10, 8);
        for _ in 0..3 {
            let (cpu, line) = simulator.next_scheduled_location().unwrap();
            simulator.scheduler_step().unwrap();
            let effects = simulator.take_effects();
            history.record(
                &simulator,
                cpu,
                line,
                ReplayAction::Scheduled,
                effects,
                None,
            );
        }
        history.sources.push("C:/one/main.ic10".to_owned());
        history.sources.push("D:/two/main.ic10".to_owned());
        history.records[0].source_id = (history.sources.len() - 2) as u32;
        history.records[1].source_id = (history.sources.len() - 1) as u32;

        let payload = trace_payload(&history, &simulator, true, 1, Some(1), false, true);
        assert_eq!(payload["records"].as_array().unwrap().len(), 1);
        assert_eq!(payload["page"]["offset"], 1);
        assert_eq!(payload["page"]["total"], 3);
        let source_ids = payload["sourceIds"].as_array().unwrap();
        let unique = source_ids
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(source_ids.len(), unique.len());
        assert!(!serde_json::to_string(&payload).unwrap().contains("C:/one"));
        assert!(!serde_json::to_string(&payload).unwrap().contains("D:/two"));

        let summary = trace_payload(&history, &simulator, false, 0, None, true, true);
        assert!(summary["records"].as_array().unwrap().is_empty());
        assert_eq!(summary["page"]["returned"], 0);
    }

    #[test]
    #[ignore = "release benchmark; run with --ignored --nocapture"]
    fn benchmark_trace_overhead_for_one_ten_and_many_ic_worlds() {
        const OPERATIONS: usize = 100_000;
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("crates/ic10-sim/tests/fixtures/multi-ic.icsim");
        let base = fixture.parent().unwrap();
        let template = Scenario::load(&fixture).unwrap();
        let requester = template
            .devices
            .iter()
            .find(|device| device.id == "requester")
            .unwrap()
            .clone();
        for count in [1_usize, 10, 50] {
            let mut scenario = template.clone();
            scenario.devices = (0..count)
                .map(|index| {
                    let mut device = requester.clone();
                    device.id = format!("benchmark-{index}");
                    device.name = format!("Benchmark IC {index}");
                    device
                })
                .collect();
            let mut ratios = Vec::new();
            let mut disabled_ratios = Vec::new();
            for _ in 0..3 {
                let mut plain = Simulator::from_scenario(scenario.clone(), base).unwrap();
                let started = Instant::now();
                let mut operations = 0;
                while operations < OPERATIONS {
                    if plain.next_scheduled_location().is_some() {
                        plain.scheduler_step().unwrap();
                        operations += 1;
                    }
                }
                let plain_time = started.elapsed();
                let plain_hash = plain.state_hash();

                let mut disabled = Simulator::from_scenario(scenario.clone(), base).unwrap();
                disabled.set_journaling(false);
                let started = Instant::now();
                let mut operations = 0;
                while operations < OPERATIONS {
                    if disabled.next_scheduled_location().is_some() {
                        disabled.scheduler_step().unwrap();
                        operations += 1;
                    }
                }
                let disabled_time = started.elapsed();
                assert_eq!(disabled.state_hash(), plain_hash);

                let mut traced = Simulator::from_scenario(scenario.clone(), base).unwrap();
                traced.set_journaling(true);
                let mut history = TraceHistory::new(&traced, 200_000, 10_000, 64);
                let started = Instant::now();
                let mut operations = 0;
                while operations < OPERATIONS {
                    if let Some((cpu, line)) = traced.next_scheduled_location() {
                        traced.scheduler_step().unwrap();
                        let effects = traced.take_effects();
                        history.record(&traced, cpu, line, ReplayAction::Scheduled, effects, None);
                        operations += 1;
                    }
                }
                let traced_time = started.elapsed();
                assert_eq!(traced.state_hash(), plain_hash);
                disabled_ratios.push(disabled_time.as_secs_f64() / plain_time.as_secs_f64());
                ratios.push(traced_time.as_secs_f64() / plain_time.as_secs_f64());
            }
            ratios.sort_by(f64::total_cmp);
            disabled_ratios.sort_by(f64::total_cmp);
            eprintln!(
                "{count} ICs: disabled={:.3}x traced={:.2}x",
                disabled_ratios[1], ratios[1]
            );
        }
    }
}
