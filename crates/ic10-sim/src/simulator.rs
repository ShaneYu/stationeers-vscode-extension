use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use ic10_data::KnowledgeBase;

use crate::program::{CompileError, Operation, Program};
use crate::scenario::{Scenario, ScenarioError};
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
}

impl Cpu {
    pub fn current_operation(&self) -> Option<&Operation> {
        self.program.operation_at_or_after(self.pc)
    }

    pub fn current_line(&self) -> Option<usize> {
        self.current_operation().map(|operation| operation.line)
    }

    pub fn register(&self, name: &str) -> Option<f64> {
        direct_register_index(name).map(|index| self.registers[index])
    }

    pub fn set_register(&mut self, name: &str, value: f64) -> Result<(), String> {
        let index =
            direct_register_index(name).ok_or_else(|| format!("unknown register `{name}`"))?;
        self.registers[index] = value;
        Ok(())
    }
}

#[derive(Debug)]
pub struct Simulator {
    pub knowledge: KnowledgeBase,
    pub world: World,
    pub cpus: Vec<Cpu>,
    pub tick: u64,
    /// Non-fatal compatibility notices suitable for a debugger console.
    pub compatibility_warnings: Vec<String>,
    scheduler_cpu: usize,
}

#[derive(Clone, Debug)]
pub struct StepEvent {
    pub cpu: usize,
    pub line: usize,
    pub state: CpuState,
}

impl Simulator {
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
        for specification in &scenario.devices {
            let Some(ic) = &specification.ic else {
                continue;
            };
            let housing = world
                .device_index(&specification.id)
                .ok_or_else(|| SimulatorError::Message("missing IC housing".to_owned()))?;
            let source_path = resolve_path(base, &ic.program);
            let source = fs::read_to_string(&source_path).map_err(|source| SimulatorError::Io {
                path: source_path.clone(),
                source,
            })?;
            let program = Program::compile(source_path, source, &knowledge)?;
            let mut registers = [0.0; REGISTER_COUNT];
            for (register, value) in &ic.registers {
                let index = direct_register_index(register).ok_or_else(|| {
                    SimulatorError::Message(format!(
                        "IC `{}` has invalid register `{register}`",
                        specification.id
                    ))
                })?;
                registers[index] = value.as_f64().map_err(SimulatorError::Message)?;
            }
            let mut stack = vec![0.0; STACK_SIZE];
            for (address, value) in &ic.stack {
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
            for (pin, device) in &ic.pins {
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
                name: world.devices[housing].name.clone(),
                housing,
                program,
                registers,
                stack,
                pins,
                pc: 0,
                state: if ic.enabled {
                    CpuState::Ready
                } else {
                    CpuState::Halted
                },
                error: None,
                operations_this_tick: 0,
                random_state: 0x9E37_79B9_7F4A_7C15 ^ (housing as u64 + 1),
            });
        }
        if cpus.is_empty() {
            return Err(SimulatorError::Message(
                "the scenario does not contain an IC program".to_owned(),
            ));
        }
        Ok(Self {
            knowledge,
            world,
            cpus,
            tick: 0,
            compatibility_warnings,
            scheduler_cpu: 0,
        })
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
        self.cpus[cpu_index].pc = operation.line;
        self.cpus[cpu_index].state = CpuState::Ready;
        self.world.devices[self.cpus[cpu_index].housing]
            .fields
            .insert("LineNumber".to_owned(), operation.line as f64);

        let result = execute_operation(
            &mut self.cpus[cpu_index],
            &mut self.world,
            &self.knowledge,
            self.tick,
            &operation,
        );
        self.cpus[cpu_index].operations_this_tick += 1;
        if let Err(message) = result {
            self.cpus[cpu_index].state = CpuState::Error;
            self.cpus[cpu_index].error = Some(message.clone());
            self.world.devices[self.cpus[cpu_index].housing]
                .fields
                .insert("Error".to_owned(), 1.0);
            return Err(message);
        }
        Ok(StepEvent {
            cpu: cpu_index,
            line: operation.line,
            state: self.cpus[cpu_index].state.clone(),
        })
    }

    pub fn scheduler_step(&mut self) -> Result<Option<StepEvent>, String> {
        let Some((index, _)) = self.next_scheduled_location() else {
            return Ok(None);
        };
        let event = self.step_instruction(index)?;
        if self.cpus[index].state != CpuState::Ready
            || self.cpus[index].operations_this_tick
                >= self
                    .knowledge
                    .language
                    .architecture
                    .maximum_instructions_per_tick
        {
            self.scheduler_cpu += 1;
        }
        Ok(Some(event))
    }

    pub fn next_scheduled_location(&mut self) -> Option<(usize, usize)> {
        loop {
            if self.scheduler_cpu >= self.cpus.len() {
                self.advance_tick();
                return None;
            }
            let index = self.scheduler_cpu;
            let runnable = match self.cpus[index].state {
                CpuState::Ready => true,
                CpuState::WaitingUntil(wake) => wake <= self.tick,
                CpuState::Halted | CpuState::Error => false,
            };
            if !runnable
                || self.cpus[index].operations_this_tick
                    >= self
                        .knowledge
                        .language
                        .architecture
                        .maximum_instructions_per_tick
            {
                self.scheduler_cpu += 1;
                continue;
            }
            let Some(line) = self.cpus[index].current_line() else {
                self.cpus[index].state = CpuState::Halted;
                self.scheduler_cpu += 1;
                continue;
            };
            return Some((index, line));
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
        let index = self
            .world
            .device_index(device_id)
            .ok_or_else(|| format!("unknown device `{device_id}`"))?;
        self.world.devices[index]
            .fields
            .insert(field.to_owned(), value);
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
        self.cpus
            .iter()
            .all(|cpu| matches!(cpu.state, CpuState::Halted | CpuState::Error))
    }

    fn advance_tick(&mut self) {
        self.tick += 1;
        self.scheduler_cpu = 0;
        for cpu in &mut self.cpus {
            cpu.operations_this_tick = 0;
            if matches!(cpu.state, CpuState::WaitingUntil(wake) if wake <= self.tick) {
                cpu.state = CpuState::Ready;
            }
        }
    }
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
) -> Result<(), String> {
    let operands = &operation.operands;
    let next = operation.line + 1;
    cpu.pc = next;
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
            write_register(cpu, target, source.clamp(minimum, maximum))?;
        }
        "lerp" => {
            let a = value(cpu, knowledge, &operands[1])?;
            let b = value(cpu, knowledge, &operands[2])?;
            let ratio = value(cpu, knowledge, &operands[3])?.clamp(0.0, 1.0);
            write_register(cpu, &operands[0], a + (b - a) * ratio)?;
        }
        "rand" => {
            cpu.random_state ^= cpu.random_state << 13;
            cpu.random_state ^= cpu.random_state >> 7;
            cpu.random_state ^= cpu.random_state << 17;
            let random = (cpu.random_state >> 11) as f64 / ((1_u64 << 53) as f64);
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
            let target = cpu.registers[target_index] as u64;
            cpu.registers[target_index] =
                ((target & !field_mask) | ((source << start) & field_mask)) as f64;
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
                cpu.registers[RETURN_ADDRESS_REGISTER] = next as f64;
            }
            let target = value(cpu, knowledge, &operands[0])?;
            cpu.pc = if operation.mnemonic == "jr" {
                relative_target(operation.line, target)?
            } else {
                line_target(target)?
            };
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
            cpu.stack[address] = value(cpu, knowledge, &operands[0])?;
            cpu.registers[STACK_POINTER_REGISTER] += 1.0;
        }
        "pop" => {
            let pointer = cpu.registers[STACK_POINTER_REGISTER] - 1.0;
            cpu.registers[STACK_POINTER_REGISTER] = pointer;
            let address = checked_address(pointer, cpu.stack.len(), "stack")?;
            write_register(cpu, &operands[0], cpu.stack[address])?;
        }
        "peek" => {
            let address = checked_address(
                cpu.registers[STACK_POINTER_REGISTER] - 1.0,
                cpu.stack.len(),
                "stack",
            )?;
            write_register(cpu, &operands[0], cpu.stack[address])?;
        }
        "poke" => {
            let address = checked_address(
                value(cpu, knowledge, &operands[0])?,
                cpu.stack.len(),
                "stack",
            )?;
            cpu.stack[address] = value(cpu, knowledge, &operands[1])?;
        }
        "get" | "put" | "clr" => {
            execute_memory(cpu, world, knowledge, operation, false)?;
        }
        "getd" | "putd" | "clrd" => {
            execute_memory(cpu, world, knowledge, operation, true)?;
        }
        "l" => {
            let target = device_reference(cpu, world, knowledge, &operands[1])?;
            let field = logic_name(cpu, knowledge, &operands[2])?;
            let loaded = world.read_field(target.device, target.connection, &field, knowledge)?;
            write_register(cpu, &operands[0], loaded)?;
        }
        "s" => {
            let target = device_reference(cpu, world, knowledge, &operands[0])?;
            let field = logic_name(cpu, knowledge, &operands[1])?;
            let stored = value(cpu, knowledge, &operands[2])?;
            world.write_field(target.device, target.connection, &field, stored, knowledge)?;
        }
        "ld" => {
            let reference_id = value(cpu, knowledge, &operands[1])? as i32;
            let target = world
                .device_by_reference(reference_id)
                .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?;
            let field = logic_name(cpu, knowledge, &operands[2])?;
            let loaded = world.read_field(target, None, &field, knowledge)?;
            write_register(cpu, &operands[0], loaded)?;
        }
        "sd" => {
            let reference_id = value(cpu, knowledge, &operands[0])? as i32;
            let target = world
                .device_by_reference(reference_id)
                .ok_or_else(|| format!("no device has ReferenceId {reference_id}"))?;
            let field = logic_name(cpu, knowledge, &operands[1])?;
            let stored = value(cpu, knowledge, &operands[2])?;
            world.write_field(target, None, &field, stored, knowledge)?;
        }
        "lb" | "lbn" | "lbs" | "lbns" => {
            execute_batch_load(cpu, world, knowledge, operation)?;
        }
        "sb" | "sbn" | "sbs" => {
            execute_batch_store(cpu, world, knowledge, operation)?;
        }
        "ls" | "ss" => {
            execute_slot(cpu, world, knowledge, operation)?;
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
                    .read_field(target.device, target.connection, &field, knowledge)
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
        "yield" => cpu.state = CpuState::WaitingUntil(tick + 1),
        "sleep" => {
            let seconds = value(cpu, knowledge, &operands[0])?.max(0.0);
            let ticks = (seconds / TICK_SECONDS).ceil().max(1.0) as u64;
            cpu.state = CpuState::WaitingUntil(tick + ticks);
        }
        "hcf" => {
            cpu.state = CpuState::Halted;
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
        cpu.registers[RETURN_ADDRESS_REGISTER] = (operation.line + 1) as f64;
    }
    cpu.pc = if relative {
        relative_target(operation.line, target)?
    } else {
        line_target(target)?
    };
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
            cpu.stack.fill(0.0);
        } else {
            world.devices[target.device].memory.fill(0.0);
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
            cpu.stack[address]
        } else {
            world.devices[target.device].memory[address]
        };
        write_register(cpu, &operation.operands[0], loaded)?;
    } else {
        let stored = value(
            cpu,
            knowledge,
            &operation.operands[value_index.expect("memory value")],
        )?;
        if target.base {
            cpu.stack[address] = stored;
        } else {
            world.devices[target.device].memory[address] = stored;
        }
    }
    Ok(())
}

fn execute_slot(
    cpu: &mut Cpu,
    world: &mut World,
    knowledge: &KnowledgeBase,
    operation: &Operation,
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
        write_register(cpu, &operation.operands[0], loaded)?;
    } else {
        let stored = value(cpu, knowledge, &operation.operands[slot_index + 2])?;
        let device_name = world.devices[target.device].name.clone();
        let fields = world.devices[target.device]
            .slots
            .get_mut(&slot)
            .ok_or_else(|| format!("{device_name} does not have slot {slot}"))?;
        fields.insert(field, stored);
    }
    Ok(())
}

fn execute_batch_load(
    cpu: &mut Cpu,
    world: &World,
    knowledge: &KnowledgeBase,
    operation: &Operation,
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
            device
                .slots
                .get(&slot)
                .and_then(|fields| fields.get(&field))
                .copied()
        } else {
            world.read_field(index, None, &field, knowledge).ok()
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
                fields.insert(field.clone(), stored);
            }
        } else {
            world.write_field(index, None, &field, stored, knowledge)?;
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
        return Ok(cpu.registers[index]);
    }
    cpu.program.resolve_number(source, knowledge)
}

fn write_register(cpu: &mut Cpu, target: &str, value: f64) -> Result<(), String> {
    let index = register_index(cpu, &cpu.program, target)?;
    cpu.registers[index] = value;
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
        let index = cpu.registers[base].trunc() as isize;
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
        let reference_id = cpu.registers[register] as i32;
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
    if knowledge
        .language
        .enums
        .get("LogicType")
        .is_some_and(|listing| listing.values.contains_key(resolved))
        || resolved.starts_with("Channel")
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
        cpu.registers[STACK_POINTER_REGISTER],
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
