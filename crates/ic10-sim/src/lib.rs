//! Deterministic IC10 execution against a shared, source-controlled world.

mod program;
mod scenario;
mod simulator;
mod world;

pub use program::{CompileError, Operation, Program};
pub use scenario::{DeviceSpec, IcSpec, NetworkSpec, Scalar, Scenario, ScenarioError};
pub use simulator::{
    Cpu, CpuState, GENERAL_REGISTER_COUNT, REGISTER_COUNT, RETURN_ADDRESS_REGISTER,
    STACK_POINTER_REGISTER, STACK_SIZE, Simulator, SimulatorError, StepEvent,
    direct_register_index,
};
pub use world::{Device, Network, World, WorldError, channel_index};
