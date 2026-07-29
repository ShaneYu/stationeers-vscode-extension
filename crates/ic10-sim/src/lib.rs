//! Deterministic IC10 execution against a shared, source-controlled world.

mod behaviour;
mod context;
#[path = "generated/lua_api_profile.rs"]
pub mod generated_lua_api_profile;
mod journal;
mod lua;
mod lua_mock;
mod program;
mod scenario;
mod simulator;
mod vm;
mod world;

pub use behaviour::{
    BehaviourCatalogEntry, BehaviourDescriptor, BehaviourError, BehaviourKind, BehaviourRuntime,
    BehaviourSelector, BehaviourState, ScheduledAction, behaviour_catalog,
};
pub use context::{
    AnalysisContext, ContextDiagnostic, EnvironmentTarget, ProgramUri, ScenarioIndex,
    context_device_markdown, valid_logic_fields, valid_operation_logic_fields, validate_context,
};
pub use journal::{
    EffectActor, EffectBatch, EffectJournal, EffectTarget, ReadEffect, SequencedWriteEffect,
    SymbolId, WriteEffect,
};
pub use lua::{
    LUA_MAX_INSTRUCTIONS, LUA_MAX_MEMORY_BYTES, LUA_MAX_MODULES, LUA_MAX_OUTPUT_BYTES,
    LUA_MAX_RECURSION_DEPTH, LUA_MAX_SOURCE_BYTES, LUA_MAX_WALL_TIME, LUA_PROFILE, LUA_PROFILE_ID,
    LUA_WORLD_PROFILE, LUA_WORLD_PROFILE_ID, LuaCapabilityStatus, LuaDiagnostic, LuaHostMock,
    LuaModuleRunner, LuaProfile, LuaProgramStatus, LuaRunLimits, LuaRunResult, LuaRuntimeBoundary,
};
pub use lua_mock::{
    DeterministicRandom, LUA_STATEFUL_MOCK_PROFILE_ID, Lifecycle, LuaMockError, LuaStatefulMock,
    PersistedState, VirtualClock,
};
pub use program::{CompileError, Operation, Program};
pub use scenario::{
    DeviceSpec, IcSpec, NetworkSpec, ProgramLanguage, ProgramSpec, Scalar, Scenario, ScenarioError,
};
pub use simulator::{
    Cpu, CpuState, GENERAL_REGISTER_COUNT, REGISTER_COUNT, RETURN_ADDRESS_REGISTER,
    STACK_POINTER_REGISTER, STACK_SIZE, Simulator, SimulatorError, SimulatorSnapshot, StepEvent,
    direct_register_index,
};
pub use world::{Device, Network, World, WorldError, channel_index};
