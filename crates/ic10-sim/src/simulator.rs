use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use ic10_data::KnowledgeBase;

use crate::behaviour::BehaviourRuntime;
use crate::journal::{EffectActor, EffectBatch, EffectJournal, EffectTarget};
use crate::lua::{LuaProgramRuntime, LuaProgramStatus};
use crate::program::{CompileError, Operation, Program};
use crate::scenario::{ProgramLanguage, Scenario, ScenarioError};
use crate::vm::{
    Ic10RuntimeSnapshot, LuaRuntimeSnapshot, VmAdapter, VmHost, VmRuntimeSnapshot, VmSchedule,
    VmSourceLocation, VmState, VmStepResult, lifecycle, step_result,
};
use crate::world::{World, WorldError};

pub const GENERAL_REGISTER_COUNT: usize = 16;
pub const RETURN_ADDRESS_REGISTER: usize = 16;
pub const STACK_POINTER_REGISTER: usize = 17;
pub const REGISTER_COUNT: usize = 18;
pub const STACK_SIZE: usize = 512;
pub const TICK_SECONDS: f64 = 0.5;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CpuState {
    Ready,
    WaitingUntil(u64),
    Halted,
    Error,
}

#[derive(Clone, Debug)]
pub struct Cpu {
    pub id: String,
    pub program_id: String,
    pub name: String,
    pub housing: usize,
    pub program: Program,
    pub registers: [f64; REGISTER_COUNT],
    pub stack: Vec<f64>,
    pub pins: [Option<usize>; 6],
    pub pc: usize,
    pub state: CpuState,
    pub error: Option<String>,
    pub operations_this_tick: u32,
    random_state: u64,
    journal: RefCell<EffectJournal>,
    journal_enabled: Cell<bool>,
    journal_actor: Cell<EffectActor>,
}

impl Cpu {
    pub fn current_operation(&self) -> Option<&Operation> {
        self.program.operation_at_or_after(self.pc)
    }

    pub fn current_line(&self) -> Option<usize> {
        self.current_operation().map(|operation| operation.line)
    }

    pub fn register(&self, name: &str) -> Option<f64> {
        direct_register_index(name).map(|index| self.read_register(index))
    }

    pub fn set_register(&mut self, name: &str, value: f64) -> Result<(), String> {
        let index =
            direct_register_index(name).ok_or_else(|| format!("unknown register `{name}`"))?;
        self.write_register(index, value);
        Ok(())
    }

    fn read_register(&self, index: usize) -> f64 {
        let value = self.registers[index];
        if !self.journal_enabled.get() {
            return value;
        }
        let cpu = actor_cpu(self.journal_actor.get());
        self.journal.borrow_mut().read(
            self.journal_actor.get(),
            EffectTarget::Register {
                cpu,
                register: index as u8,
            },
            value,
        );
        value
    }

    fn write_register(&mut self, index: usize, value: f64) {
        let before = self.registers[index];
        self.registers[index] = value;
        if !self.journal_enabled.get() {
            return;
        }
        let cpu = actor_cpu(self.journal_actor.get());
        self.journal.borrow_mut().write(
            self.journal_actor.get(),
            EffectTarget::Register {
                cpu,
                register: index as u8,
            },
            before,
            value,
        );
    }

    fn read_stack(&self, address: usize) -> f64 {
        let value = self.stack[address];
        if !self.journal_enabled.get() {
            return value;
        }
        let cpu = actor_cpu(self.journal_actor.get());
        self.journal.borrow_mut().read(
            self.journal_actor.get(),
            EffectTarget::Stack {
                cpu,
                address: address as u16,
            },
            value,
        );
        value
    }

    fn write_stack(&mut self, address: usize, value: f64) {
        let before = self.stack[address];
        self.stack[address] = value;
        if !self.journal_enabled.get() {
            return;
        }
        let cpu = actor_cpu(self.journal_actor.get());
        self.journal.borrow_mut().write(
            self.journal_actor.get(),
            EffectTarget::Stack {
                cpu,
                address: address as u16,
            },
            before,
            value,
        );
    }

    fn write_pc(&mut self, value: usize) {
        let before = self.pc;
        self.pc = value;
        if !self.journal_enabled.get() {
            return;
        }
        self.journal.borrow_mut().write_bits(
            self.journal_actor.get(),
            EffectTarget::CpuPc {
                cpu: actor_cpu(self.journal_actor.get()),
            },
            before as u64,
            value as u64,
        );
    }

    fn write_state(&mut self, value: CpuState) {
        let before = cpu_state_bits(&self.state);
        let after = cpu_state_bits(&value);
        self.state = value;
        if !self.journal_enabled.get() {
            return;
        }
        self.journal.borrow_mut().write_bits(
            self.journal_actor.get(),
            EffectTarget::CpuState {
                cpu: actor_cpu(self.journal_actor.get()),
            },
            before,
            after,
        );
    }

    fn write_random_state(&mut self, value: u64) {
        let before = self.random_state;
        self.random_state = value;
        if !self.journal_enabled.get() {
            return;
        }
        self.journal.borrow_mut().write_bits(
            self.journal_actor.get(),
            EffectTarget::CpuRandom {
                cpu: actor_cpu(self.journal_actor.get()),
            },
            before,
            value,
        );
    }

    fn configure_journal(&self, enabled: bool, actor: EffectActor) {
        self.journal_enabled.set(enabled);
        self.journal_actor.set(actor);
        self.journal.borrow_mut().set_enabled(enabled);
        self.journal.borrow_mut().take();
    }

    fn take_effects(&self) -> EffectBatch {
        self.journal.borrow_mut().take()
    }
}

fn actor_cpu(actor: EffectActor) -> usize {
    match actor {
        EffectActor::Ic { cpu, .. } => cpu,
        _ => 0,
    }
}

#[derive(Debug)]
pub struct Simulator {
    pub knowledge: KnowledgeBase,
    pub world: World,
    pub cpus: Vec<Cpu>,
    lua_programs: Vec<LuaProgramRuntime>,
    pub tick: u64,
    /// Non-fatal compatibility notices suitable for a debugger console.
    pub compatibility_warnings: Vec<String>,
    schedule: VmSchedule,
    scheduler_slot: usize,
    scheduler_error: Option<String>,
    behaviours: BehaviourRuntime,
    driver_state: BTreeMap<String, Vec<u8>>,
    journal: EffectJournal,
}

/// A checkpoint of only the mutable simulator state.
///
/// Programs and Stationpedia metadata are intentionally not copied. Debuggers
/// can therefore checkpoint periodically and retain compact execution records
/// between checkpoints instead of cloning the complete simulation on every
/// instruction.
#[derive(Clone, Debug)]
pub struct SimulatorSnapshot {
    world: World,
    runtimes: Vec<VmRuntimeSnapshot>,
    tick: u64,
    scheduler_slot: usize,
    scheduler_error: Option<String>,
    behaviours: BehaviourRuntime,
    driver_state: BTreeMap<String, Vec<u8>>,
    journal_write_sequence: u64,
}

#[derive(Clone, Debug)]
pub struct StepEvent {
    pub cpu: usize,
    pub line: usize,
    pub state: CpuState,
}

impl Simulator {
    /// Capture mutable state for a debugger checkpoint.
    pub fn snapshot(&self) -> SimulatorSnapshot {
        SimulatorSnapshot {
            world: self.world.clone(),
            runtimes: self
                .schedule
                .adapters()
                .map(|adapter| {
                    adapter
                        .snapshot(self)
                        .expect("validated VM adapter must have runtime state")
                })
                .collect(),
            tick: self.tick,
            scheduler_slot: self.scheduler_slot,
            scheduler_error: self.scheduler_error.clone(),
            behaviours: self.behaviours.clone(),
            driver_state: self.driver_state.clone(),
            journal_write_sequence: self.journal.write_sequence(),
        }
    }

    /// Restore a checkpoint while retaining compiled programs and metadata.
    pub fn restore(&mut self, snapshot: &SimulatorSnapshot) -> Result<(), String> {
        if self.schedule.adapter_count() != snapshot.runtimes.len()
            || self.world.devices.len() != snapshot.world.devices.len()
            || self.world.networks.len() != snapshot.world.networks.len()
        {
            return Err("checkpoint belongs to a different simulation".to_owned());
        }
        self.world = snapshot.world.clone();
        self.tick = snapshot.tick;
        self.scheduler_slot = snapshot.scheduler_slot;
        self.scheduler_error.clone_from(&snapshot.scheduler_error);
        self.behaviours = snapshot.behaviours.clone();
        self.driver_state = snapshot.driver_state.clone();
        self.journal
            .restore_write_sequence(snapshot.journal_write_sequence);
        let adapters: Vec<_> = self.schedule.adapters().collect();
        for (adapter, saved) in adapters.into_iter().zip(&snapshot.runtimes) {
            adapter.restore(self, saved)?;
        }
        Ok(())
    }

    /// Stable hash of all mutable state, used to verify deterministic replay.
    pub fn state_hash(&self) -> u64 {
        fn add(hash: &mut u64, bytes: &[u8]) {
            for byte in bytes {
                *hash ^= u64::from(*byte);
                *hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
        fn add_blob(hash: &mut u64, bytes: &[u8]) {
            add(hash, &(bytes.len() as u64).to_le_bytes());
            add(hash, bytes);
        }
        let mut hash = 0xcbf2_9ce4_8422_2325;
        add(&mut hash, &self.tick.to_le_bytes());
        add(&mut hash, &(self.scheduler_slot as u64).to_le_bytes());
        if let Some(error) = &self.scheduler_error {
            add(&mut hash, error.as_bytes());
        }
        for cpu in &self.cpus {
            add(&mut hash, &(cpu.pc as u64).to_le_bytes());
            add(&mut hash, &cpu.operations_this_tick.to_le_bytes());
            add(&mut hash, &cpu.random_state.to_le_bytes());
            add(&mut hash, format!("{:?}", cpu.state).as_bytes());
            if let Some(error) = &cpu.error {
                add(&mut hash, error.as_bytes());
            }
            for value in cpu.registers.iter().chain(&cpu.stack) {
                add(&mut hash, &value.to_bits().to_le_bytes());
            }
        }
        for device in &self.world.devices {
            add(&mut hash, device.id.as_bytes());
            for (field, value) in &device.fields {
                add(&mut hash, field.as_bytes());
                add(&mut hash, &value.to_bits().to_le_bytes());
            }
            for (slot, fields) in &device.slots {
                add(&mut hash, &(*slot as u64).to_le_bytes());
                for (field, value) in fields {
                    add(&mut hash, field.as_bytes());
                    add(&mut hash, &value.to_bits().to_le_bytes());
                }
            }
            for value in &device.memory {
                add(&mut hash, &value.to_bits().to_le_bytes());
            }
        }
        for network in &self.world.networks {
            add(&mut hash, network.id.as_bytes());
            for value in &network.channels {
                add(&mut hash, &value.to_bits().to_le_bytes());
            }
        }
        add_blob(&mut hash, &self.behaviours.deterministic_bytes());
        add(&mut hash, &(self.driver_state.len() as u64).to_le_bytes());
        for (driver, state) in &self.driver_state {
            add_blob(&mut hash, driver.as_bytes());
            add_blob(&mut hash, state);
        }
        hash
    }

    /// Stable replay hash for one instruction's mutable footprint.
    ///
    /// Only the scheduled CPU can change CPU-local state during an
    /// instruction. Hashing that CPU plus shared world/network/scheduler state
    /// keeps per-event tracing independent of the number of other ICs.
    pub fn event_state_hash(
        &self,
        cpu_index: usize,
        include_world: bool,
        include_stack: bool,
    ) -> u64 {
        fn add(hash: &mut u64, bytes: &[u8]) {
            for byte in bytes {
                *hash ^= u64::from(*byte);
                *hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
        let mut hash = 0xcbf2_9ce4_8422_2325;
        add(&mut hash, &self.tick.to_le_bytes());
        add(&mut hash, &(self.scheduler_slot as u64).to_le_bytes());
        if let Some(cpu) = self.cpus.get(cpu_index) {
            add(&mut hash, &(cpu.pc as u64).to_le_bytes());
            add(&mut hash, &cpu.operations_this_tick.to_le_bytes());
            add(&mut hash, &cpu.random_state.to_le_bytes());
            let state = match cpu.state {
                CpuState::Ready => 0_u64,
                CpuState::WaitingUntil(wake) => wake ^ 0x1000_0000_0000_0000,
                CpuState::Halted => 2,
                CpuState::Error => 3,
            };
            add(&mut hash, &state.to_le_bytes());
            for value in &cpu.registers {
                add(&mut hash, &value.to_bits().to_le_bytes());
            }
            if include_stack {
                for value in &cpu.stack {
                    add(&mut hash, &value.to_bits().to_le_bytes());
                }
            }
        }
        if include_world {
            for device in &self.world.devices {
                for value in device.fields.values() {
                    add(&mut hash, &value.to_bits().to_le_bytes());
                }
                for fields in device.slots.values() {
                    for value in fields.values() {
                        add(&mut hash, &value.to_bits().to_le_bytes());
                    }
                }
                for value in &device.memory {
                    add(&mut hash, &value.to_bits().to_le_bytes());
                }
            }
            for network in &self.world.networks {
                for value in &network.channels {
                    add(&mut hash, &value.to_bits().to_le_bytes());
                }
            }
        }
        hash
    }

    pub fn from_scenario_path(path: &Path) -> Result<Self, SimulatorError> {
        let scenario = Scenario::load(path)?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        Self::from_scenario(scenario, base)
    }

    pub fn from_scenario(scenario: Scenario, base: &Path) -> Result<Self, SimulatorError> {
        if scenario.schema_version != 1 {
            return Err(SimulatorError::Message(format!(
                "unsupported scenario schema version {}; expected 1",
                scenario.schema_version
            )));
        }
        let schedule = VmSchedule::plan(&scenario).map_err(SimulatorError::Message)?;
        let knowledge = KnowledgeBase::load_embedded()
            .map_err(|error| SimulatorError::Message(format!("invalid embedded data: {error}")))?;
        let compatibility_warnings = scenario
            .game_version
            .as_deref()
            .filter(|version| is_newer_game_version(version, &knowledge.language.game_version))
            .map(|version| {
                vec![format!(
                    "scenario targets Stationeers {version}, newer than bundled game data {}; \
                     simulation will continue using the bundled compatibility model",
                    knowledge.language.game_version
                )]
            })
            .unwrap_or_default();
        let world = World::build(&scenario.networks, &scenario.devices, &knowledge)?;
        let mut cpus = Vec::new();
        let mut lua_programs = Vec::new();
        for specification in &scenario.devices {
            let (program_id, program_path, language, ic) = if let Some(program_id) =
                &specification.program
            {
                let program = scenario
                    .programs
                    .iter()
                    .find(|program| &program.id == program_id)
                    .ok_or_else(|| {
                        SimulatorError::Message(format!(
                            "device `{}` references unknown program `{program_id}`",
                            specification.id
                        ))
                    })?;
                (
                    program.id.clone(),
                    program.path.clone(),
                    program.language,
                    specification.ic.as_ref(),
                )
            } else if let Some(ic) = &specification.ic {
                let program_path = ic.program.clone().ok_or_else(|| {
                        SimulatorError::Message(format!(
                            "device `{}` has IC10 state but no legacy inline program or canonical programId",
                            specification.id
                        ))
                    })?;
                (
                    specification.id.clone(),
                    program_path,
                    ProgramLanguage::Ic10,
                    Some(ic),
                )
            } else {
                continue;
            };
            let housing = world
                .device_index(&specification.id)
                .ok_or_else(|| SimulatorError::Message("missing IC housing".to_owned()))?;
            let source_path = resolve_path(base, &program_path);
            let source = fs::read_to_string(&source_path).map_err(|source| SimulatorError::Io {
                path: source_path.clone(),
                source,
            })?;
            if language == ProgramLanguage::Lua {
                lua_programs.push(
                    LuaProgramRuntime::new(
                        specification.id.clone(),
                        program_id,
                        source_path,
                        source,
                        world.clone(),
                        KnowledgeBase::load_embedded().map_err(|error| {
                            SimulatorError::Message(format!("invalid embedded data: {error}"))
                        })?,
                        ic.map(|ic| ic.pins.clone()).unwrap_or_default(),
                        housing,
                    )
                    .map_err(|error| SimulatorError::Message(error.to_string()))?,
                );
                continue;
            }
            let ic_enabled = ic.map(|ic| ic.enabled).unwrap_or(true);
            let program = Program::compile(source_path, source, &knowledge)?;
            let mut registers = [0.0; REGISTER_COUNT];
            for (register, value) in ic.into_iter().flat_map(|ic| &ic.registers) {
                let index = direct_register_index(register).ok_or_else(|| {
                    SimulatorError::Message(format!(
                        "IC `{}` has invalid register `{register}`",
                        specification.id
                    ))
                })?;
                registers[index] = value.as_f64().map_err(SimulatorError::Message)?;
            }
            let mut stack = vec![0.0; STACK_SIZE];
            for (address, value) in ic.into_iter().flat_map(|ic| &ic.stack) {
                let address = address.parse::<usize>().map_err(|_| {
                    SimulatorError::Message(format!(
                        "IC `{}` has invalid stack address `{address}`",
                        specification.id
                    ))
                })?;
                if address >= STACK_SIZE {
                    return Err(SimulatorError::Message(format!(
                        "IC `{}` stack address {address} exceeds {}",
                        specification.id,
                        STACK_SIZE - 1
                    )));
                }
                stack[address] = value.as_f64().map_err(SimulatorError::Message)?;
            }
            let mut pins = [None; 6];
            let housing_metadata = knowledge
                .device_by_name(&world.devices[housing].prefab)
                .ok_or_else(|| {
                    SimulatorError::Message(format!(
                        "missing metadata for IC housing `{}`",
                        specification.id
                    ))
                })?;
            let housing_data_network = housing_metadata
                .connections
                .iter()
                .enumerate()
                .filter(|(_, connection)| {
                    connection
                        .connection_type
                        .as_str()
                        .is_some_and(|value| value.to_ascii_lowercase().contains("data"))
                })
                .filter_map(|(index, _)| world.devices[housing].connections.get(&index))
                .copied()
                .find(|network| {
                    let network = &world.networks[*network];
                    network.kind.eq_ignore_ascii_case("cable")
                        && matches!(network.cable_role.as_str(), "data" | "powerAndData")
                });
            for (pin, device) in ic.into_iter().flat_map(|ic| &ic.pins) {
                let index = pin
                    .strip_prefix('d')
                    .and_then(|value| value.parse::<usize>().ok())
                    .filter(|value| *value < 6)
                    .ok_or_else(|| {
                        SimulatorError::Message(format!(
                            "IC `{}` has invalid pin `{pin}`",
                            specification.id
                        ))
                    })?;
                let target = world.device_index(device).ok_or_else(|| {
                    SimulatorError::Message(format!(
                        "IC `{}` pin `{pin}` references unknown device `{device}`",
                        specification.id
                    ))
                })?;
                let data_network = housing_data_network.ok_or_else(|| {
                    SimulatorError::Message(format!(
                        "IC `{}` pin `{pin}` cannot be assigned because the housing has no data cable connection",
                        specification.id
                    ))
                })?;
                let target_metadata = knowledge
                    .device_by_name(&world.devices[target].prefab)
                    .ok_or_else(|| {
                        SimulatorError::Message(format!(
                            "missing metadata for pinned device `{device}`"
                        ))
                    })?;
                let shares_data_network = target_metadata.connections.iter().enumerate().any(
                    |(connection_index, connection)| {
                        connection
                            .connection_type
                            .as_str()
                            .is_some_and(|value| value.to_ascii_lowercase().contains("data"))
                            && world.devices[target]
                                .connections
                                .get(&connection_index)
                                .is_some_and(|network| *network == data_network)
                    },
                );
                if !shares_data_network {
                    return Err(SimulatorError::Message(format!(
                        "IC `{}` pin `{pin}` references `{device}`, which is not on the housing's data cable",
                        specification.id
                    )));
                }
                pins[index] = Some(target);
            }
            cpus.push(Cpu {
                id: specification.id.clone(),
                program_id,
                name: world.devices[housing].name.clone(),
                housing,
                program,
                registers,
                stack,
                pins,
                pc: 0,
                state: if ic_enabled {
                    CpuState::Ready
                } else {
                    CpuState::Halted
                },
                error: None,
                operations_this_tick: 0,
                random_state: 0x9E37_79B9_7F4A_7C15 ^ (housing as u64 + 1),
                journal: RefCell::new(EffectJournal::default()),
                journal_enabled: Cell::new(false),
                journal_actor: Cell::new(EffectActor::Scheduler),
            });
        }
        if cpus.is_empty() && lua_programs.is_empty() {
            return Err(SimulatorError::Message(
                "the scenario does not contain a world program".to_owned(),
            ));
        }
        let behaviours = BehaviourRuntime::build(&world);
        Ok(Self {
            knowledge,
            world,
            cpus,
            lua_programs,
            tick: 0,
            compatibility_warnings,
            schedule,
            scheduler_slot: 0,
            scheduler_error: None,
            behaviours,
            driver_state: BTreeMap::new(),
            journal: EffectJournal::default(),
        })
    }

    pub fn set_journaling(&mut self, enabled: bool) {
        self.journal.set_enabled(enabled);
    }

    pub fn lua_programs(&self) -> Vec<LuaProgramStatus> {
        self.lua_programs
            .iter()
            .map(LuaProgramRuntime::status)
            .collect()
    }

    pub fn journaling_enabled(&self) -> bool {
        self.journal.is_enabled()
    }

    pub fn take_effects(&mut self) -> EffectBatch {
        self.journal.take()
    }

    pub fn write_sequence(&self) -> u64 {
        self.journal.write_sequence()
    }

    pub fn writes_after(&self, sequence: u64) -> Vec<crate::SequencedWriteEffect> {
        self.journal.writes_after(sequence)
    }

    pub fn acknowledge_writes_through(&mut self, sequence: u64) {
        self.journal.acknowledge_writes_through(sequence);
    }

    pub fn scripted_driver_actor(&mut self, driver: &str, rule: usize) -> EffectActor {
        EffectActor::ScriptedDriver {
            driver: self.journal.intern(driver),
            rule: rule as u32,
        }
    }

    pub fn record_external_effects(&mut self, before: &SimulatorSnapshot, actor: EffectActor) {
        if self.journal.is_enabled() {
            self.record_snapshot_effects(before, actor);
        }
    }

    pub fn apply_write_effect(&mut self, write: &crate::WriteEffect) -> Result<(), String> {
        let value = f64::from_bits(write.after_bits);
        match &write.target {
            EffectTarget::Register { cpu, register } => {
                self.cpus
                    .get_mut(*cpu)
                    .and_then(|cpu| cpu.registers.get_mut(*register as usize))
                    .map(|target| *target = value)
                    .ok_or_else(|| "trace register target is out of range".to_owned())?;
            }
            EffectTarget::Stack { cpu, address } => {
                self.cpus
                    .get_mut(*cpu)
                    .and_then(|cpu| cpu.stack.get_mut(*address as usize))
                    .map(|target| *target = value)
                    .ok_or_else(|| "trace stack target is out of range".to_owned())?;
            }
            EffectTarget::CpuPc { cpu } => self.cpus[*cpu].pc = write.after_bits as usize,
            EffectTarget::CpuState { cpu } => {
                self.cpus[*cpu].state = cpu_state_from_bits(write.after_bits)?;
            }
            EffectTarget::CpuOperations { cpu } => {
                self.cpus[*cpu].operations_this_tick = write.after_bits as u32;
            }
            EffectTarget::CpuRandom { cpu } => self.cpus[*cpu].random_state = write.after_bits,
            EffectTarget::SchedulerCpu => self.scheduler_slot = write.after_bits as usize,
            EffectTarget::Tick => self.tick = write.after_bits,
            EffectTarget::DeviceField { device, field } => {
                let name = self
                    .journal
                    .resolve(*field)
                    .ok_or_else(|| "trace field symbol is unavailable".to_owned())?
                    .to_owned();
                let before = self.world.devices[*device]
                    .fields
                    .insert(name.clone(), value)
                    .unwrap_or(0.0);
                self.behaviours
                    .notify_field_write(*device, &name, before, value);
            }
            EffectTarget::DeviceSlot {
                device,
                slot,
                field,
            } => {
                let name = self
                    .journal
                    .resolve(*field)
                    .ok_or_else(|| "trace slot-field symbol is unavailable".to_owned())?
                    .to_owned();
                self.world.devices[*device]
                    .slots
                    .get_mut(&(*slot as usize))
                    .ok_or_else(|| "trace slot target is unavailable".to_owned())?
                    .insert(name, value);
            }
            EffectTarget::DeviceMemory { device, address } => {
                *self.world.devices[*device]
                    .memory
                    .get_mut(*address as usize)
                    .ok_or_else(|| "trace memory target is unavailable".to_owned())? = value;
            }
            EffectTarget::NetworkChannel { network, channel } => {
                self.world.networks[*network].channels[*channel as usize] = value;
            }
            EffectTarget::CpuError { .. }
            | EffectTarget::BehaviourState { .. }
            | EffectTarget::DriverState { .. } => {
                return Err(
                    "trace effect cannot be applied without its structured state".to_owned(),
                );
            }
        }
        Ok(())
    }

    pub fn behaviour(&self, device: usize) -> Option<&crate::BehaviourDescriptor> {
        self.behaviours.descriptor(device)
    }

    pub fn behaviour_state(&self, device: usize) -> Option<&crate::BehaviourState> {
        self.behaviours.state(device)
    }

    pub fn behaviour_runtime(&self) -> &BehaviourRuntime {
        &self.behaviours
    }

    pub fn behaviour_runtime_mut(&mut self) -> &mut BehaviourRuntime {
        &mut self.behaviours
    }

    pub fn resolve_journal_symbol(&self, id: crate::SymbolId) -> Option<&str> {
        self.journal.resolve(id)
    }

    pub fn effect_target_name(&self, target: &EffectTarget) -> String {
        match target {
            EffectTarget::Register { cpu, register } => {
                format!("cpu(\"{}\").r{register}", self.cpus[*cpu].id)
            }
            EffectTarget::Stack { cpu, address } => {
                format!("cpu(\"{}\").stack[{address}]", self.cpus[*cpu].id)
            }
            EffectTarget::CpuPc { cpu } => format!("cpu(\"{}\").pc", self.cpus[*cpu].id),
            EffectTarget::CpuState { cpu } => format!("cpu(\"{}\").state", self.cpus[*cpu].id),
            EffectTarget::CpuError { cpu } => format!("cpu(\"{}\").error", self.cpus[*cpu].id),
            EffectTarget::CpuOperations { cpu } => {
                format!("cpu(\"{}\").operations", self.cpus[*cpu].id)
            }
            EffectTarget::CpuRandom { cpu } => format!("cpu(\"{}\").random", self.cpus[*cpu].id),
            EffectTarget::SchedulerCpu => "scheduler.cpu".to_owned(),
            EffectTarget::Tick => "tick".to_owned(),
            EffectTarget::DeviceField { device, field } => format!(
                "device(\"{}\").{}",
                self.world.devices[*device].id,
                self.journal.resolve(*field).unwrap_or("<unknown>")
            ),
            EffectTarget::DeviceSlot {
                device,
                slot,
                field,
            } => format!(
                "device(\"{}\").slot[{slot}].{}",
                self.world.devices[*device].id,
                self.journal.resolve(*field).unwrap_or("<unknown>")
            ),
            EffectTarget::DeviceMemory { device, address } => format!(
                "device(\"{}\").memory[{address}]",
                self.world.devices[*device].id
            ),
            EffectTarget::NetworkChannel { network, channel } => format!(
                "network(\"{}\").Channel{channel}",
                self.world.networks[*network].id
            ),
            EffectTarget::BehaviourState { device, key } => format!(
                "device(\"{}\").behaviour.{}",
                self.world.devices[*device].id,
                self.journal.resolve(*key).unwrap_or("<unknown>")
            ),
            EffectTarget::DriverState { driver } => format!(
                "driver(\"{}\").state",
                self.journal.resolve(*driver).unwrap_or("<unknown>")
            ),
        }
    }

    pub fn step_instruction(&mut self, cpu_index: usize) -> Result<StepEvent, String> {
        let cpu = self
            .cpus
            .get(cpu_index)
            .ok_or_else(|| format!("unknown IC thread {}", cpu_index + 1))?;
        if matches!(cpu.state, CpuState::Halted | CpuState::Error) {
            return Err(format!("IC `{}` is not runnable", cpu.name));
        }
        let Some(operation) = self.cpus[cpu_index].current_operation().cloned() else {
            self.cpus[cpu_index].state = CpuState::Halted;
            return Ok(StepEvent {
                cpu: cpu_index,
                line: self.cpus[cpu_index].pc,
                state: CpuState::Halted,
            });
        };
        let source = self.journal.intern(
            self.cpus[cpu_index]
                .program
                .debug_source_path
                .to_string_lossy()
                .as_ref(),
        );
        let actor = EffectActor::Ic {
            cpu: cpu_index,
            source,
            line: operation.line,
        };
        self.cpus[cpu_index].configure_journal(self.journal.is_enabled(), actor);
        self.cpus[cpu_index].write_pc(operation.line);
        self.cpus[cpu_index].write_state(CpuState::Ready);
        self.world.devices[self.cpus[cpu_index].housing]
            .fields
            .insert("LineNumber".to_owned(), operation.line as f64);

        let result = execute_operation(
            &mut self.cpus[cpu_index],
            &mut self.world,
            &self.knowledge,
            self.tick,
            &operation,
            &mut self.journal,
            actor,
        );
        for (device, field, before, after) in self.world.take_field_writes() {
            self.behaviours
                .notify_field_write(device, &field, before, after);
        }
        let before_operations = self.cpus[cpu_index].operations_this_tick;
        self.cpus[cpu_index].operations_this_tick += 1;
        self.journal.write_bits(
            actor,
            EffectTarget::CpuOperations { cpu: cpu_index },
            before_operations as u64,
            self.cpus[cpu_index].operations_this_tick as u64,
        );
        if let Err(message) = result {
            self.cpus[cpu_index].write_state(CpuState::Error);
            self.cpus[cpu_index].error = Some(message.clone());
            self.world.devices[self.cpus[cpu_index].housing]
                .fields
                .insert("Error".to_owned(), 1.0);
            self.journal.extend(self.cpus[cpu_index].take_effects());
            return Err(message);
        }
        self.journal.extend(self.cpus[cpu_index].take_effects());
        Ok(StepEvent {
            cpu: cpu_index,
            line: operation.line,
            state: self.cpus[cpu_index].state.clone(),
        })
    }

    pub fn scheduler_step(&mut self) -> Result<Option<StepEvent>, String> {
        if let Some(error) = self.scheduler_error.take() {
            return Err(error);
        }
        let Some((adapter, _)) = self.prepare_next_adapter()? else {
            return Ok(None);
        };
        let result = adapter.step(self)?;
        if !adapter.lifecycle(self)?.retains_slot() {
            self.set_scheduler_slot(self.scheduler_slot + 1);
        }
        Ok(Some(result.into_ic10_event()))
    }

    pub fn next_scheduled_location(&mut self) -> Option<(usize, usize)> {
        match self.prepare_next_adapter() {
            Ok(Some((_, location))) => Some((location.runtime_index, location.line)),
            Ok(None) => None,
            Err(error) => {
                self.scheduler_error = Some(error);
                None
            }
        }
    }

    fn prepare_next_adapter(&mut self) -> Result<Option<(VmAdapter, VmSourceLocation)>, String> {
        loop {
            if self.scheduler_slot >= self.schedule.len() {
                self.advance_tick()?;
                return Ok(None);
            }
            let adapter = self.schedule.adapter(self.scheduler_slot).ok_or_else(|| {
                "unsupported VM reached the scheduler after construction validation".to_owned()
            })?;
            let lifecycle = adapter.lifecycle(self)?;
            if !lifecycle.can_step(self.tick) {
                self.set_scheduler_slot(self.scheduler_slot + 1);
                continue;
            }
            let Some(location) = lifecycle.current_location else {
                adapter.halt(self)?;
                self.set_scheduler_slot(self.scheduler_slot + 1);
                continue;
            };
            return Ok(Some((adapter, location)));
        }
    }

    pub fn step_world_tick(&mut self) -> Result<Vec<StepEvent>, String> {
        let start = self.tick;
        let mut events = Vec::new();
        while self.tick == start {
            if let Some(event) = self.scheduler_step()? {
                events.push(event);
            }
        }
        Ok(events)
    }

    pub fn set_device_field(
        &mut self,
        device_id: &str,
        field: &str,
        value: f64,
    ) -> Result<(), String> {
        self.set_device_field_as(device_id, field, value, EffectActor::Scenario)
    }

    pub fn set_register_as(
        &mut self,
        cpu: usize,
        register: usize,
        value: f64,
        actor: EffectActor,
    ) -> Result<(), String> {
        let target = self
            .cpus
            .get_mut(cpu)
            .and_then(|cpu| cpu.registers.get_mut(register))
            .ok_or_else(|| "register target is out of range".to_owned())?;
        let before = *target;
        *target = value;
        self.journal.write(
            actor,
            EffectTarget::Register {
                cpu,
                register: register as u8,
            },
            before,
            value,
        );
        Ok(())
    }

    pub fn set_stack_as(
        &mut self,
        cpu: usize,
        address: usize,
        value: f64,
        actor: EffectActor,
    ) -> Result<(), String> {
        let target = self
            .cpus
            .get_mut(cpu)
            .and_then(|cpu| cpu.stack.get_mut(address))
            .ok_or_else(|| "stack target is out of range".to_owned())?;
        let before = *target;
        *target = value;
        self.journal.write(
            actor,
            EffectTarget::Stack {
                cpu,
                address: address as u16,
            },
            before,
            value,
        );
        Ok(())
    }

    pub fn set_device_field_as(
        &mut self,
        device_id: &str,
        field: &str,
        value: f64,
        actor: EffectActor,
    ) -> Result<(), String> {
        let index = self
            .world
            .device_index(device_id)
            .ok_or_else(|| format!("unknown device `{device_id}`"))?;
        let target = self.world.devices[index]
            .fields
            .get_mut(field)
            .ok_or_else(|| format!("device `{device_id}` has no field `{field}`"))?;
        let before = *target;
        *target = value;
        if self.journal.is_enabled() {
            let field_id = self.journal.intern(field);
            self.journal.write(
                actor,
                EffectTarget::DeviceField {
                    device: index,
                    field: field_id,
                },
                before,
                value,
            );
        }
        self.behaviours
            .notify_field_write(index, field, before, value);
        Ok(())
    }

    pub fn set_device_slot_as(
        &mut self,
        device: usize,
        slot: usize,
        field: &str,
        value: f64,
        actor: EffectActor,
    ) -> Result<(), String> {
        let target = self.world.devices[device]
            .slots
            .get_mut(&slot)
            .and_then(|fields| fields.get_mut(field))
            .ok_or_else(|| "device slot target is unavailable".to_owned())?;
        let before = *target;
        *target = value;
        if self.journal.is_enabled() {
            let field = self.journal.intern(field);
            self.journal.write(
                actor,
                EffectTarget::DeviceSlot {
                    device,
                    slot: slot as u16,
                    field,
                },
                before,
                value,
            );
        }
        Ok(())
    }

    pub fn set_device_memory_as(
        &mut self,
        device: usize,
        address: usize,
        value: f64,
        actor: EffectActor,
    ) -> Result<(), String> {
        let target = self.world.devices[device]
            .memory
            .get_mut(address)
            .ok_or_else(|| "device memory target is unavailable".to_owned())?;
        let before = *target;
        *target = value;
        self.journal.write(
            actor,
            EffectTarget::DeviceMemory {
                device,
                address: address as u32,
            },
            before,
            value,
        );
        Ok(())
    }

    pub fn set_network_channel_as(
        &mut self,
        network: usize,
        channel: usize,
        value: f64,
        actor: EffectActor,
    ) -> Result<(), String> {
        let before = self.world.networks[network].channels[channel];
        self.world.networks[network].channels[channel] = value;
        self.journal.write(
            actor,
            EffectTarget::NetworkChannel {
                network,
                channel: channel as u8,
            },
            before,
            value,
        );
        Ok(())
    }

    /// Persist deterministic state owned by an external test driver.
    ///
    /// The bytes are included in checkpoints and state hashes, making this a
    /// DAP-neutral seam for declarative scenario drivers and future debugger
    /// integrations.
    pub fn set_test_driver_state(&mut self, driver: &str, state: Vec<u8>, actor: EffectActor) {
        let before = self
            .driver_state
            .get(driver)
            .map_or(0, |bytes| stable_bytes_hash(bytes));
        let after = stable_bytes_hash(&state);
        self.driver_state.insert(driver.to_owned(), state);
        if self.journal.is_enabled() {
            let driver = self.journal.intern(driver);
            self.journal
                .write_bits(actor, EffectTarget::DriverState { driver }, before, after);
        }
    }

    pub fn test_driver_state(&self, driver: &str) -> Option<&[u8]> {
        self.driver_state.get(driver).map(Vec::as_slice)
    }

    /// Move every exposed slot field as one deterministic test-driver action.
    pub fn move_slot_item_as(
        &mut self,
        from_device: usize,
        from_slot: usize,
        to_device: usize,
        to_slot: usize,
        actor: EffectActor,
    ) -> Result<(), String> {
        let item = self
            .world
            .devices
            .get(from_device)
            .and_then(|device| device.slots.get(&from_slot))
            .cloned()
            .ok_or_else(|| "source device slot is unavailable".to_owned())?;
        if self
            .world
            .devices
            .get(to_device)
            .and_then(|device| device.slots.get(&to_slot))
            .is_none()
        {
            return Err("destination device slot is unavailable".to_owned());
        }
        for (field_name, value) in item {
            let Some(before) = self.world.devices[to_device].slots[&to_slot]
                .get(&field_name)
                .copied()
            else {
                continue;
            };
            let source = self.world.devices[from_device].slots[&from_slot]
                .get(&field_name)
                .copied()
                .unwrap_or(0.0);
            self.world.devices[from_device]
                .slots
                .get_mut(&from_slot)
                .expect("validated source slot")
                .insert(field_name.clone(), 0.0);
            self.world.devices[to_device]
                .slots
                .get_mut(&to_slot)
                .expect("validated destination slot")
                .insert(field_name.clone(), value);
            if self.journal.is_enabled() {
                let field = self.journal.intern(&field_name);
                self.journal.write(
                    actor,
                    EffectTarget::DeviceSlot {
                        device: from_device,
                        slot: from_slot as u16,
                        field,
                    },
                    source,
                    0.0,
                );
                self.journal.write(
                    actor,
                    EffectTarget::DeviceSlot {
                        device: to_device,
                        slot: to_slot as u16,
                        field,
                    },
                    before,
                    value,
                );
            }
        }
        Ok(())
    }

    /// Reset each IC's deterministic random stream from a scenario-test seed.
    pub fn set_seed(&mut self, seed: u64) {
        for cpu in &mut self.cpus {
            cpu.random_state = seed
                .wrapping_add(cpu.housing as u64 + 1)
                .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }

    pub fn is_finished(&self) -> bool {
        self.schedule.adapters().all(|adapter| {
            adapter
                .lifecycle(self)
                .is_ok_and(crate::vm::VmLifecycle::is_finished)
        })
    }

    fn set_scheduler_slot(&mut self, value: usize) {
        let before = self.scheduler_slot;
        self.scheduler_slot = value;
        self.journal.write_bits(
            EffectActor::Scheduler,
            // Keep the established journal target stable while the private
            // scheduler cursor now addresses VM-neutral slots.
            EffectTarget::SchedulerCpu,
            before as u64,
            value as u64,
        );
    }

    fn advance_tick(&mut self) -> Result<(), String> {
        self.behaviours
            .tick_end(&mut self.world, &mut self.journal)
            .map_err(|error| error.to_string())?;
        let before_tick = self.tick;
        self.tick += 1;
        self.journal.write_bits(
            EffectActor::Scheduler,
            EffectTarget::Tick,
            before_tick,
            self.tick,
        );
        self.behaviours
            .tick_start(&mut self.world, &mut self.journal, self.tick)
            .map_err(|error| error.to_string())?;
        self.set_scheduler_slot(0);
        let adapters: Vec<_> = self.schedule.adapters().collect();
        for adapter in adapters {
            adapter.begin_tick(self, self.tick)?;
        }
        Ok(())
    }

    #[allow(clippy::clone_on_copy)]
    fn record_snapshot_effects(&mut self, before: &SimulatorSnapshot, actor: EffectActor) {
        let saved_cpus: Vec<_> = self
            .schedule
            .adapters()
            .zip(&before.runtimes)
            .filter_map(|(adapter, snapshot)| match (adapter, snapshot) {
                (VmAdapter::Ic10 { cpu_index }, VmRuntimeSnapshot::Ic10(saved)) => {
                    Some((cpu_index, saved.clone()))
                }
                _ => None,
            })
            .collect();
        for (cpu_index, saved) in saved_cpus {
            let cpu = &self.cpus[cpu_index];
            for (register, (before, after)) in
                saved.registers.iter().zip(&cpu.registers).enumerate()
            {
                if before.to_bits() != after.to_bits() {
                    self.journal.write(
                        actor.clone(),
                        EffectTarget::Register {
                            cpu: cpu_index,
                            register: register as u8,
                        },
                        *before,
                        *after,
                    );
                }
            }
            for (address, (before, after)) in saved.stack.iter().zip(&cpu.stack).enumerate() {
                if before.to_bits() != after.to_bits() {
                    self.journal.write(
                        actor.clone(),
                        EffectTarget::Stack {
                            cpu: cpu_index,
                            address: address as u16,
                        },
                        *before,
                        *after,
                    );
                }
            }
            if saved.pc != cpu.pc {
                self.journal.write_bits(
                    actor.clone(),
                    EffectTarget::CpuPc { cpu: cpu_index },
                    saved.pc as u64,
                    cpu.pc as u64,
                );
            }
            let before_state = cpu_state_bits(&saved.state);
            let after_state = cpu_state_bits(&cpu.state);
            if before_state != after_state {
                self.journal.write_bits(
                    actor.clone(),
                    EffectTarget::CpuState { cpu: cpu_index },
                    before_state,
                    after_state,
                );
            }
            if saved.operations_this_tick != cpu.operations_this_tick {
                self.journal.write_bits(
                    actor.clone(),
                    EffectTarget::CpuOperations { cpu: cpu_index },
                    saved.operations_this_tick as u64,
                    cpu.operations_this_tick as u64,
                );
            }
            if saved.random_state != cpu.random_state {
                self.journal.write_bits(
                    actor.clone(),
                    EffectTarget::CpuRandom { cpu: cpu_index },
                    saved.random_state,
                    cpu.random_state,
                );
            }
            let before_error = stable_text_bits(saved.error.as_deref());
            let after_error = stable_text_bits(cpu.error.as_deref());
            if before_error != after_error {
                self.journal.write_bits(
                    actor.clone(),
                    EffectTarget::CpuError { cpu: cpu_index },
                    before_error,
                    after_error,
                );
            }
        }
        for (device, (saved, current)) in before
            .world
            .devices
            .iter()
            .zip(&self.world.devices)
            .enumerate()
        {
            for (name, after) in &current.fields {
                let before = saved.fields.get(name).copied().unwrap_or(0.0);
                if before.to_bits() != after.to_bits() {
                    let field = self.journal.intern(name);
                    self.journal.write(
                        actor.clone(),
                        EffectTarget::DeviceField { device, field },
                        before,
                        *after,
                    );
                    self.behaviours
                        .notify_field_write(device, name, before, *after);
                }
            }
            for (slot, fields) in &current.slots {
                for (name, after) in fields {
                    let before = saved
                        .slots
                        .get(slot)
                        .and_then(|values| values.get(name))
                        .copied()
                        .unwrap_or(0.0);
                    if before.to_bits() != after.to_bits() {
                        let field = self.journal.intern(name);
                        self.journal.write(
                            actor.clone(),
                            EffectTarget::DeviceSlot {
                                device,
                                slot: *slot as u16,
                                field,
                            },
                            before,
                            *after,
                        );
                    }
                }
            }
            for (address, (before, after)) in saved.memory.iter().zip(&current.memory).enumerate() {
                if before.to_bits() != after.to_bits() {
                    self.journal.write(
                        actor.clone(),
                        EffectTarget::DeviceMemory {
                            device,
                            address: address as u32,
                        },
                        *before,
                        *after,
                    );
                }
            }
        }
        for (network, (saved, current)) in before
            .world
            .networks
            .iter()
            .zip(&self.world.networks)
            .enumerate()
        {
            for (channel, (before, after)) in
                saved.channels.iter().zip(&current.channels).enumerate()
            {
                if before.to_bits() != after.to_bits() {
                    self.journal.write(
                        actor.clone(),
                        EffectTarget::NetworkChannel {
                            network,
                            channel: channel as u8,
                        },
                        *before,
                        *after,
                    );
                }
            }
        }
    }
}

impl VmHost for Simulator {
    fn ic10_lifecycle(&self, cpu_index: usize) -> Result<crate::vm::VmLifecycle, String> {
        let cpu = self
            .cpus
            .get(cpu_index)
            .ok_or_else(|| "IC10 adapter runtime index is out of range".to_owned())?;
        Ok(lifecycle(
            cpu_index,
            &cpu.state,
            cpu.current_line(),
            cpu.operations_this_tick,
            self.knowledge
                .language
                .architecture
                .maximum_instructions_per_tick,
        ))
    }

    fn ic10_step(&mut self, cpu_index: usize) -> Result<crate::vm::VmStepResult, String> {
        self.step_instruction(cpu_index).map(step_result)
    }

    fn ic10_halt(&mut self, cpu_index: usize) -> Result<(), String> {
        let cpu = self
            .cpus
            .get_mut(cpu_index)
            .ok_or_else(|| "IC10 adapter runtime index is out of range".to_owned())?;
        let before = cpu_state_bits(&cpu.state);
        cpu.state = CpuState::Halted;
        self.journal.write_bits(
            EffectActor::Scheduler,
            EffectTarget::CpuState { cpu: cpu_index },
            before,
            cpu_state_bits(&CpuState::Halted),
        );
        Ok(())
    }

    fn ic10_begin_tick(&mut self, cpu_index: usize, tick: u64) -> Result<(), String> {
        let cpu = self
            .cpus
            .get_mut(cpu_index)
            .ok_or_else(|| "IC10 adapter runtime index is out of range".to_owned())?;
        let before_operations = cpu.operations_this_tick;
        cpu.operations_this_tick = 0;
        self.journal.write_bits(
            EffectActor::Scheduler,
            EffectTarget::CpuOperations { cpu: cpu_index },
            before_operations as u64,
            0,
        );
        if matches!(cpu.state, CpuState::WaitingUntil(wake) if wake <= tick) {
            let before = cpu_state_bits(&cpu.state);
            cpu.state = CpuState::Ready;
            self.journal.write_bits(
                EffectActor::Scheduler,
                EffectTarget::CpuState { cpu: cpu_index },
                before,
                cpu_state_bits(&CpuState::Ready),
            );
        }
        Ok(())
    }

    fn ic10_snapshot(&self, cpu_index: usize) -> Result<Ic10RuntimeSnapshot, String> {
        let cpu = self
            .cpus
            .get(cpu_index)
            .ok_or_else(|| "IC10 adapter runtime index is out of range".to_owned())?;
        Ok(Ic10RuntimeSnapshot {
            registers: cpu.registers,
            stack: cpu.stack.clone(),
            pins: cpu.pins,
            pc: cpu.pc,
            state: cpu.state.clone(),
            error: cpu.error.clone(),
            operations_this_tick: cpu.operations_this_tick,
            random_state: cpu.random_state,
        })
    }

    fn ic10_restore(
        &mut self,
        cpu_index: usize,
        snapshot: &Ic10RuntimeSnapshot,
    ) -> Result<(), String> {
        let cpu = self
            .cpus
            .get_mut(cpu_index)
            .ok_or_else(|| "IC10 adapter runtime index is out of range".to_owned())?;
        cpu.registers = snapshot.registers;
        cpu.stack.clone_from(&snapshot.stack);
        cpu.pins = snapshot.pins;
        cpu.pc = snapshot.pc;
        cpu.state = snapshot.state.clone();
        cpu.error.clone_from(&snapshot.error);
        cpu.operations_this_tick = snapshot.operations_this_tick;
        cpu.random_state = snapshot.random_state;
        Ok(())
    }

    fn lua_lifecycle(&self, lua_index: usize) -> Result<crate::vm::VmLifecycle, String> {
        self.lua_programs
            .get(lua_index)
            .map(|runtime| runtime.lifecycle(self.cpus.len() + lua_index, self.tick))
            .ok_or_else(|| "Lua adapter runtime index is out of range".to_owned())
    }

    fn lua_step(&mut self, lua_index: usize) -> Result<VmStepResult, String> {
        let runtime_index = self.cpus.len() + lua_index;
        let runtime = self
            .lua_programs
            .get_mut(lua_index)
            .ok_or_else(|| "Lua adapter runtime index is out of range".to_owned())?;
        runtime.step(&mut self.world, self.tick)?;
        Ok(VmStepResult {
            location: VmSourceLocation {
                runtime_index,
                line: 1,
            },
            state: VmState::Ready,
        })
    }

    fn lua_halt(&mut self, lua_index: usize) -> Result<(), String> {
        self.lua_programs
            .get(lua_index)
            .map(|_| ())
            .ok_or_else(|| "Lua adapter runtime index is out of range".to_owned())
    }

    fn lua_begin_tick(&mut self, lua_index: usize, tick: u64) -> Result<(), String> {
        self.lua_programs
            .get_mut(lua_index)
            .map(|runtime| runtime.begin_tick(tick))
            .ok_or_else(|| "Lua adapter runtime index is out of range".to_owned())
    }

    fn lua_snapshot(&self, lua_index: usize) -> Result<LuaRuntimeSnapshot, String> {
        self.lua_programs
            .get(lua_index)
            .map(LuaProgramRuntime::snapshot)
            .ok_or_else(|| "Lua adapter runtime index is out of range".to_owned())
    }

    fn lua_restore(
        &mut self,
        lua_index: usize,
        snapshot: &LuaRuntimeSnapshot,
    ) -> Result<(), String> {
        self.lua_programs
            .get_mut(lua_index)
            .map(|runtime| runtime.restore(snapshot, &self.world))
            .ok_or_else(|| "Lua adapter runtime index is out of range".to_owned())
    }
}

fn stable_bytes_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn cpu_state_bits(state: &CpuState) -> u64 {
    match state {
        CpuState::Ready => 0,
        CpuState::WaitingUntil(tick) => 0x1000_0000_0000_0000 | tick,
        CpuState::Halted => 2,
        CpuState::Error => 3,
    }
}

fn cpu_state_from_bits(value: u64) -> Result<CpuState, String> {
    Ok(match value {
        0 => CpuState::Ready,
        2 => CpuState::Halted,
        3 => CpuState::Error,
        value if value & 0x1000_0000_0000_0000 != 0 => {
            CpuState::WaitingUntil(value & !0x1000_0000_0000_0000)
        }
        _ => return Err("invalid CPU state in trace effect".to_owned()),
    })
}

fn stable_text_bits(value: Option<&str>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.unwrap_or_default().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn is_newer_game_version(candidate: &str, bundled: &str) -> bool {
    fn components(version: &str) -> Option<Vec<u64>> {
        version
            .split('.')
            .map(str::parse)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    }
    matches!(
        (components(candidate), components(bundled)),
        (Some(candidate), Some(bundled)) if candidate > bundled
    )
}

#[cfg(test)]
mod compatibility_tests {
    use super::is_newer_game_version;

    #[test]
    fn compares_numeric_game_version_components() {
        assert!(is_newer_game_version("0.2.6404.1", "0.2.6403.27689"));
        assert!(!is_newer_game_version("0.2.6403.27689", "0.2.6403.27689"));
        assert!(!is_newer_game_version("0.2.6402.99", "0.2.6403.27689"));
        assert!(!is_newer_game_version("future", "0.2.6403.27689"));
    }
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

#[derive(Clone, Copy, Debug)]
struct DeviceReference {
    device: usize,
    connection: Option<usize>,
    base: bool,
}

fn execute_operation(
    cpu: &mut Cpu,
    world: &mut World,
    knowledge: &KnowledgeBase,
    tick: u64,
    operation: &Operation,
    journal: &mut EffectJournal,
    actor: EffectActor,
) -> Result<(), String> {
    let operands = &operation.operands;
    let next = operation.line + 1;
    cpu.write_pc(next);
    match operation.mnemonic.as_str() {
        "move" => write_register(cpu, &operands[0], value(cpu, knowledge, &operands[1])?)?,
        "add" => binary(cpu, knowledge, operands, |a, b| a + b)?,
        "sub" => binary(cpu, knowledge, operands, |a, b| a - b)?,
        "mul" => binary(cpu, knowledge, operands, |a, b| a * b)?,
        "div" => binary(cpu, knowledge, operands, |a, b| a / b)?,
        "mod" => binary(cpu, knowledge, operands, |a, b| ((a % b) + b) % b)?,
        "max" => binary(cpu, knowledge, operands, f64::max)?,
        "min" => binary(cpu, knowledge, operands, f64::min)?,
        "pow" => binary(cpu, knowledge, operands, f64::powf)?,
        "atan2" => binary(cpu, knowledge, operands, f64::atan2)?,
        "abs" => unary(cpu, knowledge, operands, f64::abs)?,
        "acos" => unary(cpu, knowledge, operands, f64::acos)?,
        "asin" => unary(cpu, knowledge, operands, f64::asin)?,
        "atan" => unary(cpu, knowledge, operands, f64::atan)?,
        "ceil" => unary(cpu, knowledge, operands, f64::ceil)?,
        "cos" => unary(cpu, knowledge, operands, f64::cos)?,
        "exp" => unary(cpu, knowledge, operands, f64::exp)?,
        "floor" => unary(cpu, knowledge, operands, f64::floor)?,
        "log" => unary(cpu, knowledge, operands, f64::ln)?,
        "round" => unary(cpu, knowledge, operands, f64::round)?,
        "sgn" => unary(cpu, knowledge, operands, |value| {
            if value.is_nan() || value == 0.0 {
                0.0
            } else {
                value.signum()
            }
        })?,
        "sin" => unary(cpu, knowledge, operands, f64::sin)?,
        "sqrt" => unary(cpu, knowledge, operands, f64::sqrt)?,
        "tan" => unary(cpu, knowledge, operands, f64::tan)?,
        "trunc" => unary(cpu, knowledge, operands, f64::trunc)?,
        "clamp" => {
            let target = &operands[0];
            let source = value(cpu, knowledge, &operands[1])?;
            let minimum = value(cpu, knowledge, &operands[2])?;
            let maximum = value(cpu, knowledge, &operands[3])?;
            let clamped = if source.is_nan() || minimum.is_nan() || maximum.is_nan() {
                f64::NAN
            } else {
                source.max(minimum).min(maximum)
            };
            write_register(cpu, target, clamped)?;
        }
        "lerp" => {
            let a = value(cpu, knowledge, &operands[1])?;
            let b = value(cpu, knowledge, &operands[2])?;
            let ratio = value(cpu, knowledge, &operands[3])?.clamp(0.0, 1.0);
            write_register(cpu, &operands[0], a + (b - a) * ratio)?;
        }
        "rand" => {
            let mut random_state = cpu.random_state;
            random_state ^= random_state << 13;
            random_state ^= random_state >> 7;
            random_state ^= random_state << 17;
            cpu.write_random_state(random_state);
            let random = (random_state >> 11) as f64 / ((1_u64 << 53) as f64);
            write_register(cpu, &operands[0], random)?;
        }
        "and" => bitwise_binary(cpu, knowledge, operands, |a, b| a & b)?,
        "or" => bitwise_binary(cpu, knowledge, operands, |a, b| a | b)?,
        "xor" => bitwise_binary(cpu, knowledge, operands, |a, b| a ^ b)?,
        "nor" => bitwise_binary(cpu, knowledge, operands, |a, b| !(a | b))?,
        "not" => {
            let source = value(cpu, knowledge, &operands[1])? as i64;
            write_register(cpu, &operands[0], (!source) as f64)?;
        }
        "sll" | "sla" => {
            let source = value(cpu, knowledge, &operands[1])? as i64;
            let amount = (value(cpu, knowledge, &operands[2])? as u32) & 63;
            write_register(cpu, &operands[0], source.wrapping_shl(amount) as f64)?;
        }
        "srl" => {
            let source = value(cpu, knowledge, &operands[1])? as u64;
            let amount = (value(cpu, knowledge, &operands[2])? as u32) & 63;
            write_register(cpu, &operands[0], source.wrapping_shr(amount) as f64)?;
        }
        "sra" => {
            let source = value(cpu, knowledge, &operands[1])? as i64;
            let amount = (value(cpu, knowledge, &operands[2])? as u32) & 63;
            write_register(cpu, &operands[0], source.wrapping_shr(amount) as f64)?;
        }
        "rol" => {
            let source = value(cpu, knowledge, &operands[1])? as u64;
            let amount = value(cpu, knowledge, &operands[2])? as u32;
            write_register(cpu, &operands[0], source.rotate_left(amount) as i64 as f64)?;
        }
        "ror" => {
            let source = value(cpu, knowledge, &operands[1])? as u64;
            let amount = value(cpu, knowledge, &operands[2])? as u32;
            write_register(cpu, &operands[0], source.rotate_right(amount) as i64 as f64)?;
        }
        "ext" => {
            let source = value(cpu, knowledge, &operands[1])? as u64;
            let start = value(cpu, knowledge, &operands[2])? as u32;
            let length = (value(cpu, knowledge, &operands[3])? as u32).min(53);
            let mask = if length == 64 {
                u64::MAX
            } else {
                (1_u64 << length) - 1
            };
            write_register(cpu, &operands[0], ((source >> start) & mask) as f64)?;
        }
        "ins" => {
            let target_index = register_index(cpu, &cpu.program, &operands[0])?;
            let source = value(cpu, knowledge, &operands[1])? as u64;
            let start = value(cpu, knowledge, &operands[2])? as u32;
            let length = (value(cpu, knowledge, &operands[3])? as u32).min(53);
            let field_mask = ((1_u64 << length) - 1) << start;
            let target = cpu.read_register(target_index) as u64;
            cpu.write_register(
                target_index,
                ((target & !field_mask) | ((source << start) & field_mask)) as f64,
            );
        }
        "select" => {
            let condition = value(cpu, knowledge, &operands[1])?;
            let selected = if condition != 0.0 {
                value(cpu, knowledge, &operands[2])?
            } else {
                value(cpu, knowledge, &operands[3])?
            };
            write_register(cpu, &operands[0], selected)?;
        }
        "seq" => selection_binary(cpu, knowledge, operands, |a, b| a == b)?,
        "sne" => selection_binary(cpu, knowledge, operands, |a, b| a != b)?,
        "sge" => selection_binary(cpu, knowledge, operands, |a, b| a >= b)?,
        "sgt" => selection_binary(cpu, knowledge, operands, |a, b| a > b)?,
        "sle" => selection_binary(cpu, knowledge, operands, |a, b| a <= b)?,
        "slt" => selection_binary(cpu, knowledge, operands, |a, b| a < b)?,
        "seqz" => selection_unary(cpu, knowledge, operands, |a| a == 0.0)?,
        "snez" => selection_unary(cpu, knowledge, operands, |a| a != 0.0)?,
        "sgez" => selection_unary(cpu, knowledge, operands, |a| a >= 0.0)?,
        "sgtz" => selection_unary(cpu, knowledge, operands, |a| a > 0.0)?,
        "slez" => selection_unary(cpu, knowledge, operands, |a| a <= 0.0)?,
        "sltz" => selection_unary(cpu, knowledge, operands, |a| a < 0.0)?,
        "snan" => selection_unary(cpu, knowledge, operands, f64::is_nan)?,
        "snanz" => selection_unary(cpu, knowledge, operands, |a| !a.is_nan())?,
        "sap" | "sna" => {
            let a = value(cpu, knowledge, &operands[1])?;
            let b = value(cpu, knowledge, &operands[2])?;
            let tolerance = value(cpu, knowledge, &operands[3])?;
            let equal = approximately(a, b, tolerance);
            write_register(
                cpu,
                &operands[0],
                bool_value(if operation.mnemonic == "sap" {
                    equal
                } else {
                    !equal
                }),
            )?;
        }
        "sapz" | "snaz" => {
            let a = value(cpu, knowledge, &operands[1])?;
            let tolerance = value(cpu, knowledge, &operands[2])?;
            let equal = approximately(a, 0.0, tolerance);
            write_register(
                cpu,
                &operands[0],
                bool_value(if operation.mnemonic == "sapz" {
                    equal
                } else {
                    !equal
                }),
            )?;
        }
        "j" | "jal" | "jr" => {
            if operation.mnemonic == "jal" {
                cpu.write_register(RETURN_ADDRESS_REGISTER, next as f64);
            }
            let target = value(cpu, knowledge, &operands[0])?;
            cpu.write_pc(if operation.mnemonic == "jr" {
                relative_target(operation.line, target)?
            } else {
                line_target(target)?
            });
        }
        mnemonic if is_comparison_branch(mnemonic) => {
            execute_comparison_branch(cpu, knowledge, operation)?;
        }
        "bnan" | "brnan" => {
            if value(cpu, knowledge, &operands[0])?.is_nan() {
                branch_to(
                    cpu,
                    operation,
                    value(cpu, knowledge, &operands[1])?,
                    operation.mnemonic.starts_with("br"),
                    false,
                )?;
            }
        }
        "bap" | "bapal" | "bna" | "bnaal" | "brap" | "brna" => {
            let a = value(cpu, knowledge, &operands[0])?;
            let b = value(cpu, knowledge, &operands[1])?;
            let tolerance = value(cpu, knowledge, &operands[2])?;
            let equal = approximately(a, b, tolerance);
            let take = if operation.mnemonic.contains("bna") || operation.mnemonic == "brna" {
                !equal
            } else {
                equal
            };
            if take {
                let target = value(cpu, knowledge, operands.last().expect("branch target"))?;
                branch_to(
                    cpu,
                    operation,
                    target,
                    operation.mnemonic.starts_with("br"),
                    operation.mnemonic.ends_with("al"),
                )?;
            }
        }
        "bapz" | "bapzal" | "bnaz" | "bnazal" | "brapz" | "brnaz" => {
            let a = value(cpu, knowledge, &operands[0])?;
            let tolerance = value(cpu, knowledge, &operands[1])?;
            let equal = approximately(a, 0.0, tolerance);
            let take = if operation.mnemonic.contains("bnaz") || operation.mnemonic == "brnaz" {
                !equal
            } else {
                equal
            };
            if take {
                let target = value(cpu, knowledge, operands.last().expect("branch target"))?;
                branch_to(
                    cpu,
                    operation,
                    target,
                    operation.mnemonic.starts_with("br"),
                    operation.mnemonic.ends_with("al"),
                )?;
            }
        }
        "push" => {
            let address = stack_pointer(cpu)?;
            cpu.write_stack(address, value(cpu, knowledge, &operands[0])?);
            let pointer = cpu.read_register(STACK_POINTER_REGISTER);
            cpu.write_register(STACK_POINTER_REGISTER, pointer + 1.0);
        }
        "pop" => {
            let pointer = cpu.read_register(STACK_POINTER_REGISTER) - 1.0;
            cpu.write_register(STACK_POINTER_REGISTER, pointer);
            let address = checked_address(pointer, cpu.stack.len(), "stack")?;
            write_register(cpu, &operands[0], cpu.read_stack(address))?;
        }
        "peek" => {
            let address = checked_address(
                cpu.read_register(STACK_POINTER_REGISTER) - 1.0,
                cpu.stack.len(),
                "stack",
            )?;
            write_register(cpu, &operands[0], cpu.read_stack(address))?;
        }
        "poke" => {
            let address = checked_address(
                value(cpu, knowledge, &operands[0])?,
                cpu.stack.len(),
                "stack",
            )?;
            cpu.write_stack(address, value(cpu, knowledge, &operands[1])?);
        }
        "get" | "put" | "clr" => {
            execute_memory(cpu, world, knowledge, operation, false, journal, actor)?;
        }
        "getd" | "putd" | "clrd" => {
            execute_memory(cpu, world, knowledge, operation, true, journal, actor)?;
        }
        "l" => {
            let target = device_reference(cpu, world, knowledge, &operands[1])?;
            let field = logic_name(cpu, knowledge, &operands[2])?;
            let loaded = world.read_field(
                target.device,
                target.connection,
                &field,
                knowledge,
                journal,
                actor,
            )?;
            write_register(cpu, &operands[0], loaded)?;
        }
        "s" => {
            let target = device_reference(cpu, world, knowledge, &operands[0])?;
            let field = logic_name(cpu, knowledge, &operands[1])?;
            let stored = value(cpu, knowledge, &operands[2])?;
            world.write_field(
                target.device,
                target.connection,
                &field,
                stored,
                knowledge,
                journal,
                actor,
            )?;
        }
        "ld" => {
            let reference_id = value(cpu, knowledge, &operands[1])? as i32;
            let target = world
                .device_by_reference(reference_id)
                .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?;
            let field = logic_name(cpu, knowledge, &operands[2])?;
            let loaded = world.read_field(target, None, &field, knowledge, journal, actor)?;
            write_register(cpu, &operands[0], loaded)?;
        }
        "sd" => {
            let reference_id = value(cpu, knowledge, &operands[0])? as i32;
            let target = world
                .device_by_reference(reference_id)
                .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?;
            let field = logic_name(cpu, knowledge, &operands[1])?;
            let stored = value(cpu, knowledge, &operands[2])?;
            world.write_field(target, None, &field, stored, knowledge, journal, actor)?;
        }
        "lb" | "lbn" | "lbs" | "lbns" => {
            execute_batch_load(cpu, world, knowledge, operation, journal, actor)?;
        }
        "sb" | "sbn" | "sbs" => {
            execute_batch_store(cpu, world, knowledge, operation, journal, actor)?;
        }
        "ls" | "ss" => {
            execute_slot(cpu, world, knowledge, operation, journal, actor)?;
        }
        "bdns" | "bdnsal" | "brdns" | "bdse" | "bdseal" | "brdse" => {
            let exists = device_reference(cpu, world, knowledge, &operands[0]).is_ok();
            let take = if operation.mnemonic.contains("dns") {
                !exists
            } else {
                exists
            };
            if take {
                let target = value(cpu, knowledge, operands.last().expect("branch target"))?;
                branch_to(
                    cpu,
                    operation,
                    target,
                    operation.mnemonic.starts_with("br"),
                    operation.mnemonic.ends_with("al"),
                )?;
            }
        }
        "bdnvl" | "bdnvs" => {
            let target = device_reference(cpu, world, knowledge, &operands[0]);
            let field = logic_name(cpu, knowledge, &operands[1]);
            let valid = match (target, field) {
                (Ok(target), Ok(field)) if operation.mnemonic == "bdnvl" => world
                    .read_field(
                        target.device,
                        target.connection,
                        &field,
                        knowledge,
                        journal,
                        actor,
                    )
                    .is_ok(),
                (Ok(target), Ok(field)) => knowledge
                    .device_by_name(&world.devices[target.device].prefab)
                    .and_then(|device| device.logic_types.get(&field))
                    .is_some_and(|access| access.write),
                _ => false,
            };
            if !valid {
                let target = value(cpu, knowledge, &operands[2])?;
                branch_to(cpu, operation, target, false, false)?;
            }
        }
        "sdns" | "sdse" => {
            let exists = device_reference(cpu, world, knowledge, &operands[1]).is_ok();
            write_register(
                cpu,
                &operands[0],
                bool_value(if operation.mnemonic == "sdns" {
                    !exists
                } else {
                    exists
                }),
            )?;
        }
        "yield" => cpu.write_state(CpuState::WaitingUntil(tick + 1)),
        "sleep" => {
            let seconds = value(cpu, knowledge, &operands[0])?.max(0.0);
            let ticks = (seconds / TICK_SECONDS).ceil().max(1.0) as u64;
            cpu.write_state(CpuState::WaitingUntil(tick + ticks));
        }
        "hcf" => {
            cpu.write_state(CpuState::Halted);
        }
        "label" => {}
        "lr" | "rmap" => {
            return Err(format!(
                "`{}` requires an active reagent/device behaviour model",
                operation.mnemonic
            ));
        }
        mnemonic => return Err(format!("instruction `{mnemonic}` is not implemented")),
    }
    Ok(())
}

fn unary(
    cpu: &mut Cpu,
    knowledge: &KnowledgeBase,
    operands: &[String],
    operation: impl FnOnce(f64) -> f64,
) -> Result<(), String> {
    let source = value(cpu, knowledge, &operands[1])?;
    write_register(cpu, &operands[0], operation(source))
}

fn binary(
    cpu: &mut Cpu,
    knowledge: &KnowledgeBase,
    operands: &[String],
    operation: impl FnOnce(f64, f64) -> f64,
) -> Result<(), String> {
    let a = value(cpu, knowledge, &operands[1])?;
    let b = value(cpu, knowledge, &operands[2])?;
    write_register(cpu, &operands[0], operation(a, b))
}

fn bitwise_binary(
    cpu: &mut Cpu,
    knowledge: &KnowledgeBase,
    operands: &[String],
    operation: impl FnOnce(i64, i64) -> i64,
) -> Result<(), String> {
    let a = value(cpu, knowledge, &operands[1])? as i64;
    let b = value(cpu, knowledge, &operands[2])? as i64;
    write_register(cpu, &operands[0], operation(a, b) as f64)
}

fn selection_unary(
    cpu: &mut Cpu,
    knowledge: &KnowledgeBase,
    operands: &[String],
    condition: impl FnOnce(f64) -> bool,
) -> Result<(), String> {
    let source = value(cpu, knowledge, &operands[1])?;
    write_register(cpu, &operands[0], bool_value(condition(source)))
}

fn selection_binary(
    cpu: &mut Cpu,
    knowledge: &KnowledgeBase,
    operands: &[String],
    condition: impl FnOnce(f64, f64) -> bool,
) -> Result<(), String> {
    let a = value(cpu, knowledge, &operands[1])?;
    let b = value(cpu, knowledge, &operands[2])?;
    write_register(cpu, &operands[0], bool_value(condition(a, b)))
}

fn bool_value(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

fn approximately(a: f64, b: f64, tolerance: f64) -> bool {
    let floor = f32::from_bits(1) as f64 * 8.0;
    (a - b).abs() <= (tolerance.abs() * a.abs().max(b.abs())).max(floor)
}

fn is_comparison_branch(value: &str) -> bool {
    let normalized;
    let value = if let Some(relative) = value.strip_prefix("br") {
        normalized = format!("b{relative}");
        normalized.as_str()
    } else {
        value
    };
    matches!(
        value,
        "beq"
            | "beqal"
            | "beqz"
            | "beqzal"
            | "bne"
            | "bneal"
            | "bnez"
            | "bnezal"
            | "bge"
            | "bgeal"
            | "bgez"
            | "bgezal"
            | "bgt"
            | "bgtal"
            | "bgtz"
            | "bgtzal"
            | "ble"
            | "bleal"
            | "blez"
            | "blezal"
            | "blt"
            | "bltal"
            | "bltz"
            | "bltzal"
    )
}

fn execute_comparison_branch(
    cpu: &mut Cpu,
    knowledge: &KnowledgeBase,
    operation: &Operation,
) -> Result<(), String> {
    let relative = operation.mnemonic.starts_with("br");
    let normalized;
    let name = if let Some(relative_name) = operation.mnemonic.strip_prefix("br") {
        normalized = format!("b{relative_name}");
        normalized.as_str()
    } else {
        operation.mnemonic.as_str()
    };
    let link = name.ends_with("al");
    let base = name.strip_suffix("al").unwrap_or(name);
    let zero = base.ends_with('z');
    let comparison = base.strip_suffix('z').unwrap_or(base);
    let a = value(cpu, knowledge, &operation.operands[0])?;
    let (b, target_index) = if zero {
        (0.0, 1)
    } else {
        (value(cpu, knowledge, &operation.operands[1])?, 2)
    };
    let take = match comparison {
        "beq" => a == b,
        "bne" => a != b,
        "bge" => a >= b,
        "bgt" => a > b,
        "ble" => a <= b,
        "blt" => a < b,
        _ => false,
    };
    if take {
        let target = value(cpu, knowledge, &operation.operands[target_index])?;
        branch_to(cpu, operation, target, relative, link)?;
    }
    Ok(())
}

fn branch_to(
    cpu: &mut Cpu,
    operation: &Operation,
    target: f64,
    relative: bool,
    link: bool,
) -> Result<(), String> {
    if link {
        cpu.write_register(RETURN_ADDRESS_REGISTER, (operation.line + 1) as f64);
    }
    cpu.write_pc(if relative {
        relative_target(operation.line, target)?
    } else {
        line_target(target)?
    });
    Ok(())
}

fn line_target(value: f64) -> Result<usize, String> {
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid branch target {value}"));
    }
    Ok(value.trunc() as usize)
}

fn relative_target(line: usize, offset: f64) -> Result<usize, String> {
    if !offset.is_finite() {
        return Err(format!("invalid relative branch offset {offset}"));
    }
    let target = line as i64 + offset.trunc() as i64;
    if target < 0 {
        return Err(format!("relative branch targets line {target}"));
    }
    Ok(target as usize)
}

fn execute_memory(
    cpu: &mut Cpu,
    world: &mut World,
    knowledge: &KnowledgeBase,
    operation: &Operation,
    direct: bool,
    journal: &mut EffectJournal,
    actor: EffectActor,
) -> Result<(), String> {
    let mnemonic = operation.mnemonic.as_str();
    let (target, address_index, value_index) = if direct {
        let id_index = usize::from(mnemonic == "getd");
        let reference_id = value(cpu, knowledge, &operation.operands[id_index])? as i32;
        let device = world
            .device_by_reference(reference_id)
            .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?;
        (
            DeviceReference {
                device,
                connection: None,
                base: false,
            },
            match mnemonic {
                "getd" => Some(2),
                "putd" => Some(1),
                _ => None,
            },
            (mnemonic == "putd").then_some(2),
        )
    } else {
        let target_index = usize::from(mnemonic == "get");
        (
            device_reference(cpu, world, knowledge, &operation.operands[target_index])?,
            match mnemonic {
                "get" => Some(2),
                "put" => Some(1),
                _ => None,
            },
            (mnemonic == "put").then_some(2),
        )
    };
    if mnemonic == "clr" || mnemonic == "clrd" {
        if target.base {
            for address in 0..cpu.stack.len() {
                cpu.write_stack(address, 0.0);
            }
        } else {
            for address in 0..world.devices[target.device].memory.len() {
                let before = world.devices[target.device].memory[address];
                world.devices[target.device].memory[address] = 0.0;
                journal.write(
                    actor,
                    EffectTarget::DeviceMemory {
                        device: target.device,
                        address: address as u32,
                    },
                    before,
                    0.0,
                );
            }
        }
        return Ok(());
    }
    let address = value(
        cpu,
        knowledge,
        &operation.operands[address_index.expect("memory address")],
    )?;
    let length = if target.base {
        cpu.stack.len()
    } else {
        world.devices[target.device].memory.len()
    };
    let address = checked_address(address, length, "device memory")?;
    if mnemonic == "get" || mnemonic == "getd" {
        let loaded = if target.base {
            cpu.read_stack(address)
        } else {
            let loaded = world.devices[target.device].memory[address];
            journal.read(
                actor,
                EffectTarget::DeviceMemory {
                    device: target.device,
                    address: address as u32,
                },
                loaded,
            );
            loaded
        };
        write_register(cpu, &operation.operands[0], loaded)?;
    } else {
        let stored = value(
            cpu,
            knowledge,
            &operation.operands[value_index.expect("memory value")],
        )?;
        if target.base {
            cpu.write_stack(address, stored);
        } else {
            let before = world.devices[target.device].memory[address];
            world.devices[target.device].memory[address] = stored;
            journal.write(
                actor,
                EffectTarget::DeviceMemory {
                    device: target.device,
                    address: address as u32,
                },
                before,
                stored,
            );
        }
    }
    Ok(())
}

fn execute_slot(
    cpu: &mut Cpu,
    world: &mut World,
    knowledge: &KnowledgeBase,
    operation: &Operation,
    journal: &mut EffectJournal,
    actor: EffectActor,
) -> Result<(), String> {
    let load = operation.mnemonic == "ls";
    let target_index = usize::from(load);
    let target = device_reference(cpu, world, knowledge, &operation.operands[target_index])?;
    let slot_index = target_index + 1;
    let slot = value(cpu, knowledge, &operation.operands[slot_index])? as usize;
    let field = logic_name(cpu, knowledge, &operation.operands[slot_index + 1])?;
    if load {
        let loaded = world.devices[target.device]
            .slots
            .get(&slot)
            .and_then(|fields| fields.get(&field))
            .copied()
            .ok_or_else(|| {
                format!(
                    "{} slot {slot} does not expose `{field}`",
                    world.devices[target.device].name
                )
            })?;
        if journal.is_enabled() {
            let field_id = journal.intern(&field);
            journal.read(
                actor,
                EffectTarget::DeviceSlot {
                    device: target.device,
                    slot: slot as u16,
                    field: field_id,
                },
                loaded,
            );
        }
        write_register(cpu, &operation.operands[0], loaded)?;
    } else {
        let stored = value(cpu, knowledge, &operation.operands[slot_index + 2])?;
        let device_name = world.devices[target.device].name.clone();
        let fields = world.devices[target.device]
            .slots
            .get_mut(&slot)
            .ok_or_else(|| format!("{device_name} does not have slot {slot}"))?;
        let before = fields.get(&field).copied().unwrap_or(0.0);
        fields.insert(field.clone(), stored);
        if journal.is_enabled() {
            let field = journal.intern(&field);
            journal.write(
                actor,
                EffectTarget::DeviceSlot {
                    device: target.device,
                    slot: slot as u16,
                    field,
                },
                before,
                stored,
            );
        }
    }
    Ok(())
}

fn execute_batch_load(
    cpu: &mut Cpu,
    world: &mut World,
    knowledge: &KnowledgeBase,
    operation: &Operation,
    journal: &mut EffectJournal,
    actor: EffectActor,
) -> Result<(), String> {
    let named = operation.mnemonic == "lbn" || operation.mnemonic == "lbns";
    let slotted = operation.mnemonic == "lbs" || operation.mnemonic == "lbns";
    let prefab_hash = value(cpu, knowledge, &operation.operands[1])? as i32;
    let mut cursor = 2;
    let name_hash = if named {
        let value = value(cpu, knowledge, &operation.operands[cursor])? as i32;
        cursor += 1;
        Some(value)
    } else {
        None
    };
    let slot = if slotted {
        let value = value(cpu, knowledge, &operation.operands[cursor])? as usize;
        cursor += 1;
        Some(value)
    } else {
        None
    };
    let field = logic_name(cpu, knowledge, &operation.operands[cursor])?;
    cursor += 1;
    let mode = batch_mode(cpu, knowledge, &operation.operands[cursor])?;
    let network = world.devices[cpu.housing]
        .connections
        .get(&0)
        .copied()
        .ok_or_else(|| format!("IC `{}` has no data network on connection 0", cpu.name))?;
    let mut values = Vec::new();
    for index in world.devices_on_network(network) {
        let device = &world.devices[index];
        if device.prefab_hash != prefab_hash
            || name_hash.is_some_and(|hash| device.name_hash != hash)
        {
            continue;
        }
        let loaded = if let Some(slot) = slot {
            let loaded = device
                .slots
                .get(&slot)
                .and_then(|fields| fields.get(&field))
                .copied();
            if let Some(value) = loaded
                && journal.is_enabled()
            {
                let field_id = journal.intern(&field);
                journal.read(
                    actor,
                    EffectTarget::DeviceSlot {
                        device: index,
                        slot: slot as u16,
                        field: field_id,
                    },
                    value,
                );
            }
            loaded
        } else {
            world
                .read_field(index, None, &field, knowledge, journal, actor)
                .ok()
        };
        if let Some(value) = loaded {
            values.push(value);
        }
    }
    let result = aggregate(&values, mode)?;
    write_register(cpu, &operation.operands[0], result)
}

fn execute_batch_store(
    cpu: &mut Cpu,
    world: &mut World,
    knowledge: &KnowledgeBase,
    operation: &Operation,
    journal: &mut EffectJournal,
    actor: EffectActor,
) -> Result<(), String> {
    let named = operation.mnemonic == "sbn";
    let slotted = operation.mnemonic == "sbs";
    let prefab_hash = value(cpu, knowledge, &operation.operands[0])? as i32;
    let mut cursor = 1;
    let name_hash = if named {
        let value = value(cpu, knowledge, &operation.operands[cursor])? as i32;
        cursor += 1;
        Some(value)
    } else {
        None
    };
    let slot = if slotted {
        let value = value(cpu, knowledge, &operation.operands[cursor])? as usize;
        cursor += 1;
        Some(value)
    } else {
        None
    };
    let field = logic_name(cpu, knowledge, &operation.operands[cursor])?;
    cursor += 1;
    let stored = value(cpu, knowledge, &operation.operands[cursor])?;
    let network = world.devices[cpu.housing]
        .connections
        .get(&0)
        .copied()
        .ok_or_else(|| format!("IC `{}` has no data network on connection 0", cpu.name))?;
    let targets: Vec<_> = world.devices_on_network(network).collect();
    for index in targets {
        let device = &world.devices[index];
        if device.prefab_hash != prefab_hash
            || name_hash.is_some_and(|hash| device.name_hash != hash)
        {
            continue;
        }
        if let Some(slot) = slot {
            if let Some(fields) = world.devices[index].slots.get_mut(&slot) {
                let before = fields.get(&field).copied().unwrap_or(0.0);
                fields.insert(field.clone(), stored);
                if journal.is_enabled() {
                    let field_id = journal.intern(&field);
                    journal.write(
                        actor,
                        EffectTarget::DeviceSlot {
                            device: index,
                            slot: slot as u16,
                            field: field_id,
                        },
                        before,
                        stored,
                    );
                }
            }
        } else {
            world.write_field(index, None, &field, stored, knowledge, journal, actor)?;
        }
    }
    Ok(())
}

fn batch_mode(cpu: &Cpu, knowledge: &KnowledgeBase, value: &str) -> Result<i32, String> {
    let resolved = cpu.program.resolve_alias(value);
    match resolved {
        "Average" => Ok(0),
        "Sum" => Ok(1),
        "Minimum" => Ok(2),
        "Maximum" => Ok(3),
        "Count" => Ok(4),
        _ => Ok(cpu.program.resolve_number(resolved, knowledge)? as i32),
    }
}

fn aggregate(values: &[f64], mode: i32) -> Result<f64, String> {
    match mode {
        0 if values.is_empty() => Ok(f64::NAN),
        0 => Ok(values.iter().sum::<f64>() / values.len() as f64),
        1 => Ok(values.iter().sum()),
        2 if values.is_empty() => Ok(f64::NAN),
        2 => Ok(values.iter().copied().fold(f64::INFINITY, f64::min)),
        3 if values.is_empty() => Ok(f64::NAN),
        3 => Ok(values.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
        4 => Ok(values.len() as f64),
        _ => Err(format!("unknown batch mode {mode}")),
    }
}

fn value(cpu: &Cpu, knowledge: &KnowledgeBase, source: &str) -> Result<f64, String> {
    if let Ok(index) = register_index(cpu, &cpu.program, source) {
        return Ok(cpu.read_register(index));
    }
    cpu.program.resolve_number(source, knowledge)
}

fn write_register(cpu: &mut Cpu, target: &str, value: f64) -> Result<(), String> {
    let index = register_index(cpu, &cpu.program, target)?;
    cpu.write_register(index, value);
    Ok(())
}

fn register_index(cpu: &Cpu, program: &Program, value: &str) -> Result<usize, String> {
    let value = program.resolve_alias(value);
    if let Some(index) = direct_register_index(value) {
        return Ok(index);
    }
    if let Some(register) = value.strip_prefix('r')
        && let Some(base) = direct_register_index(register)
    {
        let index = cpu.read_register(base).trunc() as isize;
        if (0..REGISTER_COUNT as isize).contains(&index) {
            return Ok(index as usize);
        }
        return Err(format!("indirect register index {index} is out of range"));
    }
    Err(format!("`{value}` is not a register"))
}

pub fn direct_register_index(value: &str) -> Option<usize> {
    match value {
        "ra" => Some(RETURN_ADDRESS_REGISTER),
        "sp" => Some(STACK_POINTER_REGISTER),
        _ => value
            .strip_prefix('r')
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value < REGISTER_COUNT),
    }
}

fn device_reference(
    cpu: &Cpu,
    world: &World,
    knowledge: &KnowledgeBase,
    source: &str,
) -> Result<DeviceReference, String> {
    let resolved = cpu.program.resolve_alias(source);
    let (device, connection) = resolved
        .split_once(':')
        .map_or((resolved, None), |(device, connection)| {
            (device, connection.parse::<usize>().ok())
        });
    let base = device == "db";
    let index = if base {
        cpu.housing
    } else if let Some(pin) = device
        .strip_prefix('d')
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value < 6)
    {
        cpu.pins[pin].ok_or_else(|| format!("device pin d{pin} is not set"))?
    } else if let Some(register) = device.strip_prefix('d') {
        let register = register_index(cpu, &cpu.program, register)?;
        let reference_id = cpu.read_register(register) as i32;
        world
            .device_by_reference(reference_id)
            .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?
    } else if register_index(cpu, &cpu.program, device).is_ok() {
        let reference_id = value(cpu, knowledge, device)? as i32;
        world
            .device_by_reference(reference_id)
            .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?
    } else {
        let reference_id = cpu.program.resolve_number(device, knowledge)? as i32;
        world
            .device_by_reference(reference_id)
            .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?
    };
    Ok(DeviceReference {
        device: index,
        connection,
        base,
    })
}

fn logic_name(cpu: &Cpu, knowledge: &KnowledgeBase, source: &str) -> Result<String, String> {
    let resolved = cpu.program.resolve_alias(source);
    if ["LogicType", "LogicSlotType"].iter().any(|name| {
        knowledge
            .language
            .enums
            .get(*name)
            .is_some_and(|listing| listing.values.contains_key(resolved))
    }) || resolved.starts_with("Channel")
    {
        return Ok(resolved.to_owned());
    }
    let numeric = value(cpu, knowledge, resolved)?;
    let listing = knowledge
        .language
        .enums
        .get("LogicType")
        .ok_or_else(|| "embedded LogicType enum is missing".to_owned())?;
    listing
        .values
        .iter()
        .find(|(_, candidate)| {
            candidate
                .value
                .as_f64()
                .or_else(|| candidate.value.as_i64().map(|value| value as f64))
                == Some(numeric)
        })
        .map(|(name, _)| name.clone())
        .ok_or_else(|| format!("{numeric} is not a LogicType"))
}

fn stack_pointer(cpu: &Cpu) -> Result<usize, String> {
    checked_address(
        cpu.read_register(STACK_POINTER_REGISTER),
        cpu.stack.len(),
        "stack",
    )
}

fn checked_address(value: f64, length: usize, label: &str) -> Result<usize, String> {
    if !value.is_finite() || value < 0.0 {
        return Err(format!("invalid {label} address {value}"));
    }
    let address = value.trunc() as usize;
    if address >= length {
        return Err(format!(
            "{label} address {address} exceeds {}",
            length.saturating_sub(1)
        ));
    }
    Ok(address)
}

#[derive(Debug)]
pub enum SimulatorError {
    Scenario(ScenarioError),
    World(WorldError),
    Compile(CompileError),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Message(String),
}

impl fmt::Display for SimulatorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scenario(error) => error.fmt(formatter),
            Self::World(error) => error.fmt(formatter),
            Self::Compile(error) => error.fmt(formatter),
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "could not read IC10 program {}: {source}",
                    path.display()
                )
            }
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for SimulatorError {}

impl From<ScenarioError> for SimulatorError {
    fn from(value: ScenarioError) -> Self {
        Self::Scenario(value)
    }
}

impl From<WorldError> for SimulatorError {
    fn from(value: WorldError) -> Self {
        Self::World(value)
    }
}

impl From<CompileError> for SimulatorError {
    fn from(value: CompileError) -> Self {
        Self::Compile(value)
    }
}
