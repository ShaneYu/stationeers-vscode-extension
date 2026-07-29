use std::path::{Path, PathBuf};

use crate::scenario::{ProgramLanguage, Scenario};
use crate::simulator::{CpuState, REGISTER_COUNT, StepEvent};

/// VM-neutral execution state observed by the shared scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmState {
    Ready,
    WaitingUntil(u64),
    Halted,
    Faulted,
}

impl VmState {
    pub(crate) fn runnable_at(self, tick: u64) -> bool {
        matches!(self, Self::Ready) || matches!(self, Self::WaitingUntil(wake) if wake <= tick)
    }
}

/// A source location owned by one scheduled runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmSourceLocation {
    pub(crate) runtime_index: usize,
    pub(crate) line: usize,
}

/// The complete scheduler-facing lifecycle view of one VM.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmLifecycle {
    pub(crate) state: VmState,
    pub(crate) current_location: Option<VmSourceLocation>,
    pub(crate) operations_this_tick: u32,
    pub(crate) operation_budget: u32,
}

impl VmLifecycle {
    pub(crate) fn can_step(self, tick: u64) -> bool {
        self.state.runnable_at(tick) && self.operations_this_tick < self.operation_budget
    }

    pub(crate) fn retains_slot(self) -> bool {
        self.state == VmState::Ready && self.operations_this_tick < self.operation_budget
    }

    pub(crate) fn is_finished(self) -> bool {
        matches!(self.state, VmState::Halted | VmState::Faulted)
    }
}

/// Language-neutral result of one adapter step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmStepResult {
    pub(crate) location: VmSourceLocation,
    pub(crate) state: VmState,
}

impl VmStepResult {
    pub(crate) fn into_ic10_event(self) -> StepEvent {
        StepEvent {
            cpu: self.location.runtime_index,
            line: self.location.line,
            state: cpu_state(self.state),
        }
    }
}

/// Complete mutable IC10 state retained behind the adapter snapshot contract.
#[derive(Clone, Debug)]
pub(crate) struct Ic10RuntimeSnapshot {
    pub(crate) registers: [f64; REGISTER_COUNT],
    pub(crate) stack: Vec<f64>,
    pub(crate) pins: [Option<usize>; 6],
    pub(crate) pc: usize,
    pub(crate) state: CpuState,
    pub(crate) error: Option<String>,
    pub(crate) operations_this_tick: u32,
    pub(crate) random_state: u64,
}

/// Extensible per-VM snapshot payload. Lua receives its own variant when its
/// evidence-backed world lifecycle is implemented.
#[derive(Clone, Debug)]
pub(crate) enum VmRuntimeSnapshot {
    Ic10(Ic10RuntimeSnapshot),
}

/// Services implemented by the simulator and consumed only through adapter
/// dispatch. Scheduler control flow never indexes `Cpu` directly.
pub(crate) trait VmHost {
    fn ic10_lifecycle(&self, cpu_index: usize) -> Result<VmLifecycle, String>;
    fn ic10_step(&mut self, cpu_index: usize) -> Result<VmStepResult, String>;
    fn ic10_halt(&mut self, cpu_index: usize) -> Result<(), String>;
    fn ic10_begin_tick(&mut self, cpu_index: usize, tick: u64) -> Result<(), String>;
    fn ic10_snapshot(&self, cpu_index: usize) -> Result<Ic10RuntimeSnapshot, String>;
    fn ic10_restore(
        &mut self,
        cpu_index: usize,
        snapshot: &Ic10RuntimeSnapshot,
    ) -> Result<(), String>;
}

/// One bound language adapter in the deterministic world scheduler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmAdapter {
    Ic10 { cpu_index: usize },
}

impl VmAdapter {
    pub(crate) fn lifecycle(self, host: &impl VmHost) -> Result<VmLifecycle, String> {
        match self {
            Self::Ic10 { cpu_index } => host.ic10_lifecycle(cpu_index),
        }
    }

    pub(crate) fn step(self, host: &mut impl VmHost) -> Result<VmStepResult, String> {
        match self {
            Self::Ic10 { cpu_index } => host.ic10_step(cpu_index),
        }
    }

    pub(crate) fn halt(self, host: &mut impl VmHost) -> Result<(), String> {
        match self {
            Self::Ic10 { cpu_index } => host.ic10_halt(cpu_index),
        }
    }

    pub(crate) fn begin_tick(self, host: &mut impl VmHost, tick: u64) -> Result<(), String> {
        match self {
            Self::Ic10 { cpu_index } => host.ic10_begin_tick(cpu_index, tick),
        }
    }

    pub(crate) fn snapshot(self, host: &impl VmHost) -> Result<VmRuntimeSnapshot, String> {
        match self {
            Self::Ic10 { cpu_index } => host.ic10_snapshot(cpu_index).map(VmRuntimeSnapshot::Ic10),
        }
    }

    pub(crate) fn restore(
        self,
        host: &mut impl VmHost,
        snapshot: &VmRuntimeSnapshot,
    ) -> Result<(), String> {
        match (self, snapshot) {
            (Self::Ic10 { cpu_index }, VmRuntimeSnapshot::Ic10(snapshot)) => {
                host.ic10_restore(cpu_index, snapshot)
            }
        }
    }
}

/// One language-neutral position in the deterministic world scheduler.
#[derive(Clone, Debug)]
pub(crate) enum VmScheduleSlot {
    Adapter(VmAdapter),
    UnsupportedLua { program_id: String, path: PathBuf },
}

#[derive(Clone, Debug)]
pub(crate) struct VmSchedule {
    slots: Vec<VmScheduleSlot>,
}

impl VmSchedule {
    /// Find attached Lua before structural world validation. This keeps the
    /// fail-closed diagnostic true even if an earlier IC10 device is malformed.
    pub(crate) fn first_attached_lua(scenario: &Scenario) -> Option<(&str, &Path)> {
        scenario.devices.iter().find_map(|device| {
            let program_id = device.program.as_deref()?;
            let program = scenario
                .programs
                .iter()
                .find(|program| program.id == program_id)?;
            (program.language == ProgramLanguage::Lua)
                .then_some((program.id.as_str(), program.path.as_path()))
        })
    }

    /// Build slots in scenario device declaration order without reading or
    /// compiling any program source.
    pub(crate) fn plan(scenario: &Scenario) -> Result<Self, String> {
        let mut slots = Vec::new();
        let mut next_cpu = 0;

        for device in &scenario.devices {
            let (program_id, path, language) = if let Some(program_id) = &device.program {
                let program = scenario
                    .programs
                    .iter()
                    .find(|program| &program.id == program_id)
                    .ok_or_else(|| {
                        format!(
                            "device `{}` references unknown program `{program_id}`",
                            device.id
                        )
                    })?;
                (program.id.clone(), program.path.clone(), program.language)
            } else if let Some(ic) = &device.ic {
                let path = ic.program.clone().ok_or_else(|| {
                    format!(
                        "device `{}` has IC10 state but no legacy inline program or canonical programId",
                        device.id
                    )
                })?;
                (device.id.clone(), path, ProgramLanguage::Ic10)
            } else {
                continue;
            };

            match language {
                ProgramLanguage::Ic10 => {
                    slots.push(VmScheduleSlot::Adapter(VmAdapter::Ic10 {
                        cpu_index: next_cpu,
                    }));
                    next_cpu += 1;
                }
                ProgramLanguage::Lua => {
                    slots.push(VmScheduleSlot::UnsupportedLua { program_id, path })
                }
            }
        }

        Ok(Self { slots })
    }

    pub(crate) fn unsupported_lua(&self) -> Option<(&str, &Path)> {
        self.slots.iter().find_map(|slot| match slot {
            VmScheduleSlot::UnsupportedLua { program_id, path } => {
                Some((program_id.as_str(), path.as_path()))
            }
            VmScheduleSlot::Adapter(_) => None,
        })
    }

    pub(crate) fn adapter(&self, slot: usize) -> Option<VmAdapter> {
        match self.slots.get(slot)? {
            VmScheduleSlot::Adapter(adapter) => Some(*adapter),
            VmScheduleSlot::UnsupportedLua { .. } => None,
        }
    }

    pub(crate) fn adapters(&self) -> impl Iterator<Item = VmAdapter> + '_ {
        self.slots.iter().filter_map(|slot| match slot {
            VmScheduleSlot::Adapter(adapter) => Some(*adapter),
            VmScheduleSlot::UnsupportedLua { .. } => None,
        })
    }

    pub(crate) fn adapter_count(&self) -> usize {
        self.adapters().count()
    }

    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }
}

fn vm_state(state: &CpuState) -> VmState {
    match state {
        CpuState::Ready => VmState::Ready,
        CpuState::WaitingUntil(wake) => VmState::WaitingUntil(*wake),
        CpuState::Halted => VmState::Halted,
        CpuState::Error => VmState::Faulted,
    }
}

fn cpu_state(state: VmState) -> CpuState {
    match state {
        VmState::Ready => CpuState::Ready,
        VmState::WaitingUntil(wake) => CpuState::WaitingUntil(wake),
        VmState::Halted => CpuState::Halted,
        VmState::Faulted => CpuState::Error,
    }
}

pub(crate) fn lifecycle(
    cpu_index: usize,
    state: &CpuState,
    current_line: Option<usize>,
    operations_this_tick: u32,
    operation_budget: u32,
) -> VmLifecycle {
    VmLifecycle {
        state: vm_state(state),
        current_location: current_line.map(|line| VmSourceLocation {
            runtime_index: cpu_index,
            line,
        }),
        operations_this_tick,
        operation_budget,
    }
}

pub(crate) fn step_result(event: StepEvent) -> VmStepResult {
    VmStepResult {
        location: VmSourceLocation {
            runtime_index: event.cpu,
            line: event.line,
        },
        state: vm_state(&event.state),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Ic10RuntimeSnapshot, VmAdapter, VmHost, VmRuntimeSnapshot, VmSchedule, VmScheduleSlot,
        VmSourceLocation, VmState, VmStepResult, lifecycle,
    };
    use crate::{CpuState, REGISTER_COUNT, Scenario};

    struct FakeHost {
        runtime: Ic10RuntimeSnapshot,
        line: Option<usize>,
        budget: u32,
    }

    impl VmHost for FakeHost {
        fn ic10_lifecycle(&self, cpu_index: usize) -> Result<super::VmLifecycle, String> {
            Ok(lifecycle(
                cpu_index,
                &self.runtime.state,
                self.line,
                self.runtime.operations_this_tick,
                self.budget,
            ))
        }

        fn ic10_step(&mut self, cpu_index: usize) -> Result<VmStepResult, String> {
            let line = self.line.ok_or_else(|| "missing source line".to_owned())?;
            self.runtime.pc += 1;
            self.runtime.operations_this_tick += 1;
            self.runtime.state = CpuState::WaitingUntil(4);
            Ok(VmStepResult {
                location: VmSourceLocation {
                    runtime_index: cpu_index,
                    line,
                },
                state: VmState::WaitingUntil(4),
            })
        }

        fn ic10_halt(&mut self, _cpu_index: usize) -> Result<(), String> {
            self.runtime.state = CpuState::Halted;
            Ok(())
        }

        fn ic10_begin_tick(&mut self, _cpu_index: usize, tick: u64) -> Result<(), String> {
            self.runtime.operations_this_tick = 0;
            if matches!(self.runtime.state, CpuState::WaitingUntil(wake) if wake <= tick) {
                self.runtime.state = CpuState::Ready;
            }
            Ok(())
        }

        fn ic10_snapshot(&self, _cpu_index: usize) -> Result<Ic10RuntimeSnapshot, String> {
            Ok(self.runtime.clone())
        }

        fn ic10_restore(
            &mut self,
            _cpu_index: usize,
            snapshot: &Ic10RuntimeSnapshot,
        ) -> Result<(), String> {
            self.runtime = snapshot.clone();
            Ok(())
        }
    }

    fn fake_host() -> FakeHost {
        FakeHost {
            runtime: Ic10RuntimeSnapshot {
                registers: [0.0; REGISTER_COUNT],
                stack: vec![0.0; 4],
                pins: [None; 6],
                pc: 0,
                state: CpuState::Ready,
                error: None,
                operations_this_tick: 1,
                random_state: 7,
            },
            line: Some(12),
            budget: 3,
        }
    }

    #[test]
    fn plan_preserves_device_order_across_vm_languages() {
        let scenario: Scenario = serde_json::from_str(
            r#"{
              "schemaVersion": 1,
              "programs": [
                {"id":"first","path":"first.ic10","language":"ic10"},
                {"id":"middle","path":"middle.lua","language":"lua"},
                {"id":"last","path":"last.ic10","language":"ic10"}
              ],
              "devices": [
                {"id":"first-housing","prefab":"StructureCircuitHousing","program":"first"},
                {"id":"middle-housing","prefab":"StructureCircuitHousing","program":"middle"},
                {"id":"last-housing","prefab":"StructureCircuitHousing","program":"last"}
              ]
            }"#,
        )
        .expect("scenario");

        let schedule = VmSchedule::plan(&scenario).expect("schedule");

        assert!(matches!(
            schedule.slots.as_slice(),
            [
                VmScheduleSlot::Adapter(VmAdapter::Ic10 { cpu_index: 0 }),
                VmScheduleSlot::UnsupportedLua { program_id, .. },
                VmScheduleSlot::Adapter(VmAdapter::Ic10 { cpu_index: 1 })
            ] if program_id == "middle"
        ));
    }

    #[test]
    fn attached_lua_scan_uses_the_existing_first_match_program_resolution() {
        let scenario: Scenario = serde_json::from_str(
            r#"{
              "schemaVersion": 1,
              "programs": [
                {"id":"duplicate","path":"first.ic10","language":"ic10"},
                {"id":"duplicate","path":"second.lua","language":"lua"}
              ],
              "devices": [
                {"id":"housing","prefab":"StructureCircuitHousing","program":"duplicate"}
              ]
            }"#,
        )
        .expect("scenario");

        assert!(VmSchedule::first_attached_lua(&scenario).is_none());
        assert!(matches!(
            VmSchedule::plan(&scenario)
                .expect("schedule")
                .slots
                .as_slice(),
            [VmScheduleSlot::Adapter(VmAdapter::Ic10 { cpu_index: 0 })]
        ));
    }

    #[test]
    fn adapter_contract_owns_lifecycle_step_and_runtime_snapshot() {
        let adapter = VmAdapter::Ic10 { cpu_index: 0 };
        let mut host = fake_host();

        let before = adapter.lifecycle(&host).expect("lifecycle");
        assert_eq!(before.state, VmState::Ready);
        assert_eq!(
            before.current_location,
            Some(VmSourceLocation {
                runtime_index: 0,
                line: 12
            })
        );
        assert_eq!(before.operations_this_tick, 1);
        assert_eq!(before.operation_budget, 3);
        assert!(before.can_step(0));

        let snapshot = adapter.snapshot(&host).expect("snapshot");
        let step = adapter.step(&mut host).expect("step");
        assert_eq!(step.location.line, 12);
        assert_eq!(step.state, VmState::WaitingUntil(4));
        assert_eq!(host.runtime.pc, 1);

        adapter.begin_tick(&mut host, 4).expect("begin tick");
        assert_eq!(host.runtime.operations_this_tick, 0);
        assert_eq!(host.runtime.state, CpuState::Ready);

        adapter.restore(&mut host, &snapshot).expect("restore");
        assert_eq!(host.runtime.pc, 0);
        assert_eq!(host.runtime.operations_this_tick, 1);
        assert_eq!(host.runtime.state, CpuState::Ready);
        assert!(matches!(snapshot, VmRuntimeSnapshot::Ic10(_)));
    }
}
