//! Sandboxed local Lua 5.2 execution.
//!
//! The pure-module runner and the world-attached program adapter deliberately
//! share the same sandbox and host mock surface without sharing lifecycle
//! state.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::chunk::ChunkMode;
use mlua::debug::DebugEvent;
use mlua::{
    Error as MluaError, HookTriggers, Lua, LuaOptions, StdLib, Table, Value, Variadic, VmState,
};

use ic10_data::KnowledgeBase;

use crate::journal::{EffectActor, EffectJournal};
use crate::world::World;

pub const LUA_PROFILE_ID: &str = "stationeerslua-0.9.5.0-lua5.2-pure-module-v1";
pub const LUA_WORLD_PROFILE_ID: &str = "stationeerslua-0.9.5.0-lua5.2-core-world-program-v1";
pub const LUA_MAX_INSTRUCTIONS: u64 = 10_000_000;
pub const LUA_MAX_WALL_TIME: Duration = Duration::from_secs(30);
pub const LUA_MAX_MEMORY_BYTES: usize = 64 * 1024 * 1024;
pub const LUA_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const LUA_MAX_MODULES: usize = 256;
pub const LUA_MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
pub const LUA_MAX_RECURSION_DEPTH: usize = 512;

const HOOK_INSTRUCTION_INTERVAL: u32 = 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LuaCapabilityStatus {
    PureModules,
    WorldPrograms,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaProfile {
    pub id: &'static str,
    pub lua_version: &'static str,
    pub stationeers_lua_version: &'static str,
    pub runtime: LuaCapabilityStatus,
}

pub const LUA_PROFILE: LuaProfile = LuaProfile {
    id: LUA_PROFILE_ID,
    lua_version: "5.2",
    stationeers_lua_version: "0.9.5.0",
    runtime: LuaCapabilityStatus::PureModules,
};

pub const LUA_WORLD_PROFILE: LuaProfile = LuaProfile {
    id: LUA_WORLD_PROFILE_ID,
    lua_version: "5.2",
    stationeers_lua_version: "0.9.5.0",
    runtime: LuaCapabilityStatus::WorldPrograms,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub source_path: PathBuf,
    pub line: Option<usize>,
}

impl fmt::Display for LuaDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: {}{}: {}",
            self.code,
            self.source_path.display(),
            self.line.map(|line| format!(":{line}")).unwrap_or_default(),
            self.message
        )
    }
}

impl std::error::Error for LuaDiagnostic {}

#[derive(Clone, Debug)]
pub struct LuaRunLimits {
    pub max_instructions: u64,
    pub wall_time: Duration,
    pub memory_bytes: usize,
    pub max_output_bytes: usize,
    pub max_modules: usize,
    pub max_source_bytes: usize,
    pub max_recursion_depth: usize,
}

impl Default for LuaRunLimits {
    fn default() -> Self {
        Self {
            max_instructions: 1_000_000,
            wall_time: Duration::from_secs(5),
            memory_bytes: 16 * 1024 * 1024,
            max_output_bytes: 64 * 1024,
            max_modules: 64,
            max_source_bytes: 1024 * 1024,
            max_recursion_depth: 128,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LuaRunResult {
    pub output: Vec<String>,
    pub instructions: u64,
    pub loaded_modules: Vec<PathBuf>,
}

/// The opt-in, deterministic Stationeers host surface used by focused Lua
/// fixtures. It deliberately does not own scheduling or chip lifecycle.
#[derive(Clone)]
pub struct LuaHostMock {
    world: Rc<RefCell<World>>,
    knowledge: Rc<KnowledgeBase>,
    pins: BTreeMap<String, String>,
    journal: Rc<RefCell<EffectJournal>>,
    logs: Rc<RefCell<Vec<String>>>,
}

impl LuaHostMock {
    pub fn new(world: World, knowledge: KnowledgeBase, pins: BTreeMap<String, String>) -> Self {
        Self {
            world: Rc::new(RefCell::new(world)),
            knowledge: Rc::new(knowledge),
            pins,
            journal: Rc::new(RefCell::new(EffectJournal::default())),
            logs: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn world(&self) -> Rc<RefCell<World>> {
        Rc::clone(&self.world)
    }
    pub fn logs(&self) -> Vec<String> {
        self.logs.borrow().clone()
    }

    pub(crate) fn replace_world(&self, world: World) {
        *self.world.borrow_mut() = world;
    }

    pub(crate) fn world_snapshot(&self) -> World {
        self.world.borrow().clone()
    }

    fn device(&self, name: &str) -> Result<usize, String> {
        let world = self.world.borrow();
        world
            .device_index(name)
            .or_else(|| {
                name.parse()
                    .ok()
                    .and_then(|id| world.device_by_reference(id))
            })
            .ok_or_else(|| format!("[lua-missing-device] device `{name}` was not found"))
    }

    fn field(&self, device: usize, field: &str) -> Result<f64, String> {
        self.world.borrow().read_field(
            device,
            None,
            field,
            &self.knowledge,
            &mut self.journal.borrow_mut(),
            EffectActor::Scenario,
        )
    }

    fn set_field(&self, device: usize, field: &str, value: f64) -> Result<(), String> {
        self.world.borrow_mut().write_field(
            device,
            None,
            field,
            value,
            &self.knowledge,
            &mut self.journal.borrow_mut(),
            EffectActor::Scenario,
        )
    }

    fn pin_device(&self, pin: &str) -> Result<usize, String> {
        let name = self
            .pins
            .get(pin)
            .ok_or_else(|| format!("[lua-invalid-pin] pin `{pin}` is not configured"))?;
        self.device(name)
    }

    fn install_device(&self, lua: &Lua, name: String) -> mlua::Result<Table> {
        let host = self.clone();
        let table = lua.create_table()?;
        let get_host = host.clone();
        let get_name = name.clone();
        table.set(
            "get",
            lua.create_function(move |_, (_self, field): (Table, String)| {
                get_host
                    .field(
                        get_host.device(&get_name).map_err(MluaError::external)?,
                        &field,
                    )
                    .map_err(MluaError::external)
            })?,
        )?;
        let set_host = host.clone();
        let set_name = name.clone();
        table.set(
            "set",
            lua.create_function(move |_, (_self, field, value): (Table, String, f64)| {
                set_host
                    .set_field(
                        set_host.device(&set_name).map_err(MluaError::external)?,
                        &field,
                        value,
                    )
                    .map_err(MluaError::external)
            })?,
        )?;
        let slot_host = host.clone();
        let slot_name = name.clone();
        table.set("slot", lua.create_function(move |lua, (_self, slot): (Table, u64)| {
            let device = slot_host.device(&slot_name).map_err(MluaError::external)?;
            let proxy = lua.create_table()?;
            let get_host = slot_host.clone();
            proxy.set("get", lua.create_function(move |_, (_self, field): (Table, String)| {
                get_host.world.borrow().devices[device].slots.get(&(slot as usize))
                    .and_then(|values| values.get(&field)).copied()
                    .ok_or_else(|| MluaError::external(format!("[lua-invalid-slot] device `{}` slot {slot} does not expose `{field}`", get_host.world.borrow().devices[device].id)))
            })?)?;
            let set_host = slot_host.clone();
            proxy.set("set", lua.create_function(move |_, (_self, field, value): (Table, String, f64)| {
                let mut world = set_host.world.borrow_mut();
                let values = world.devices[device].slots.get_mut(&(slot as usize)).ok_or_else(|| MluaError::external(format!("[lua-invalid-slot] slot {slot} is unavailable")))?;
                if !values.contains_key(&field) { return Err(MluaError::external(format!("[lua-invalid-slot] slot {slot} does not expose `{field}`"))); }
                values.insert(field, value);
                Ok(())
            })?)?;
            Ok(proxy)
        })?)?;
        let memory_host = host;
        let memory_name = name.clone();
        table.set(
            "memory",
            lua.create_function(move |_, (_self, address): (Table, u64)| {
                let device = memory_host
                    .device(&memory_name)
                    .map_err(MluaError::external)?;
                memory_host.world.borrow().devices[device]
                    .memory
                    .get(address as usize)
                    .copied()
                    .ok_or_else(|| {
                        MluaError::external(format!(
                            "[lua-invalid-memory] device memory address {address} is unavailable"
                        ))
                    })
            })?,
        )?;
        let memory_host = self.clone();
        table.set(
            "setMemory",
            lua.create_function(move |_, (_self, address, value): (Table, u64, f64)| {
                let device = memory_host.device(&name).map_err(MluaError::external)?;
                let mut world = memory_host.world.borrow_mut();
                let cell = world.devices[device]
                    .memory
                    .get_mut(address as usize)
                    .ok_or_else(|| {
                        MluaError::external(format!(
                            "[lua-invalid-memory] device memory address {address} is unavailable"
                        ))
                    })?;
                *cell = value;
                Ok(())
            })?,
        )?;
        Ok(table)
    }

    fn install(&self, lua: &Lua) -> mlua::Result<()> {
        let device = lua.create_table()?;
        let host = self.clone();
        device.set(
            "get",
            lua.create_function(move |lua, name: String| {
                host.device(&name).map_err(MluaError::external)?;
                host.install_device(lua, name)
            })?,
        )?;
        let host = self.clone();
        device.set(
            "getReferenceId",
            lua.create_function(move |_, name: String| {
                let index = host.device(&name).map_err(MluaError::external)?;
                Ok(host.world.borrow().devices[index].reference_id)
            })?,
        )?;
        lua.globals().set("device", device)?;

        let ic = lua.create_table()?;
        let host = self.clone();
        ic.set(
            "get",
            lua.create_function(move |_, (pin, field): (String, String)| {
                host.field(host.pin_device(&pin).map_err(MluaError::external)?, &field)
                    .map_err(MluaError::external)
            })?,
        )?;
        let host = self.clone();
        ic.set(
            "set",
            lua.create_function(move |_, (pin, field, value): (String, String, f64)| {
                host.set_field(
                    host.pin_device(&pin).map_err(MluaError::external)?,
                    &field,
                    value,
                )
                .map_err(MluaError::external)
            })?,
        )?;
        lua.globals().set("ic", ic)
    }
}

/// Public, language-neutral inspection data for one attached Lua program.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LuaProgramStatus {
    pub id: String,
    pub program_id: String,
    pub invocations: u64,
    pub waiting_until: Option<u64>,
    pub faulted: bool,
    pub error: Option<String>,
    pub output: Vec<String>,
}

pub(crate) struct LuaProgramRuntime {
    pub(crate) id: String,
    pub(crate) program_id: String,
    pub(crate) source_path: PathBuf,
    pub(crate) housing: usize,
    source: String,
    host: LuaHostMock,
    limits: LuaRunLimits,
    invocations: u64,
    faulted: bool,
    error: Option<String>,
    operations_this_tick: u32,
}

// Lua program runtimes are moved only with the simulator behind the DAP
// worker's mutex; their Rc/RefCell host state is never shared concurrently.
unsafe impl Send for LuaProgramRuntime {}

impl fmt::Debug for LuaProgramRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LuaProgramRuntime")
            .field("id", &self.id)
            .field("program_id", &self.program_id)
            .field("source_path", &self.source_path)
            .field("housing", &self.housing)
            .field("invocations", &self.invocations)
            .field("faulted", &self.faulted)
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl LuaProgramRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: String,
        program_id: String,
        source_path: PathBuf,
        source: String,
        world: World,
        knowledge: KnowledgeBase,
        pins: BTreeMap<String, String>,
        housing: usize,
    ) -> Result<Self, LuaDiagnostic> {
        let limits = LuaRunLimits::default();
        validate_limits(&source_path, &limits)?;
        if source.len() > limits.max_source_bytes {
            return Err(LuaDiagnostic {
                code: "lua-source-limit",
                message: format!(
                    "Lua source is {} bytes, exceeding the {}-byte limit",
                    source.len(),
                    limits.max_source_bytes
                ),
                source_path,
                line: None,
            });
        }
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT,
            LuaOptions::default(),
        )
        .map_err(|error| diagnostic_from_mlua(error, &source_path, &[]))?;
        lua.set_memory_limit(limits.memory_bytes)
            .map_err(|error| diagnostic_from_mlua(error, &source_path, &[]))?;
        lua.load(source.as_str())
            .set_name(lua_chunk_name(&source_path))
            .set_mode(ChunkMode::Text)
            .into_function()
            .map_err(|error| diagnostic_from_mlua(error, &source_path, &[]))?;

        Ok(Self {
            id,
            program_id,
            source_path,
            housing,
            source,
            host: LuaHostMock::new(world, knowledge, pins),
            limits,
            invocations: 0,
            faulted: false,
            error: None,
            operations_this_tick: 0,
        })
    }

    pub(crate) fn status(&self) -> LuaProgramStatus {
        LuaProgramStatus {
            id: self.id.clone(),
            program_id: self.program_id.clone(),
            invocations: self.invocations,
            waiting_until: None,
            faulted: self.faulted,
            error: self.error.clone(),
            output: self.host.logs(),
        }
    }

    pub(crate) fn lifecycle(&self, runtime_index: usize, _tick: u64) -> crate::vm::VmLifecycle {
        let state = if self.faulted {
            crate::vm::VmState::Faulted
        } else {
            crate::vm::VmState::Ready
        };
        crate::vm::VmLifecycle {
            state,
            current_location: Some(crate::vm::VmSourceLocation {
                runtime_index,
                line: 1,
            }),
            operations_this_tick: self.operations_this_tick,
            operation_budget: 1,
        }
    }

    pub(crate) fn step(&mut self, world: &mut World, _tick: u64) -> Result<(), String> {
        self.host.replace_world(world.clone());
        match self.execute_once() {
            Ok(()) => {
                *world = self.host.world_snapshot();
                Ok(())
            }
            Err(error) => {
                self.faulted = true;
                self.error = Some(error.to_string());
                Err(error.to_string())
            }
        }
    }

    fn execute_once(&mut self) -> Result<(), LuaDiagnostic> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT,
            LuaOptions::default(),
        )
        .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        lua.set_memory_limit(self.limits.memory_bytes)
            .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        let output = Rc::new(RefCell::new(OutputCapture::default()));
        install_output_capture(&lua, Rc::clone(&output), self.limits.max_output_bytes)
            .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        install_denied_apis(&lua)
            .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        self.host
            .install(&lua)
            .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        let instruction_count = Rc::new(Cell::new(0_u64));
        install_limits(
            &lua,
            instruction_count,
            self.limits.max_instructions,
            self.limits.max_recursion_depth,
            self.limits.wall_time,
        )
        .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        lua.load(self.source.as_str())
            .set_name(lua_chunk_name(&self.source_path))
            .set_mode(ChunkMode::Text)
            .exec()
            .map_err(|error| diagnostic_from_mlua(error, &self.source_path, &[]))?;
        self.host
            .logs
            .borrow_mut()
            .extend(output.borrow().lines.clone());
        self.invocations += 1;
        self.operations_this_tick = 1;
        Ok(())
    }

    pub(crate) fn begin_tick(&mut self, _tick: u64) {
        self.operations_this_tick = 0;
    }

    pub(crate) fn snapshot(&self) -> crate::vm::LuaRuntimeSnapshot {
        crate::vm::LuaRuntimeSnapshot {
            invocations: self.invocations,
            faulted: self.faulted,
            error: self.error.clone(),
            operations_this_tick: self.operations_this_tick,
            output: self.host.logs(),
        }
    }

    pub(crate) fn restore(
        &mut self,
        snapshot: &crate::vm::LuaRuntimeSnapshot,
        current_world: &World,
    ) {
        self.host.replace_world(current_world.clone());
        self.invocations = snapshot.invocations;
        self.faulted = snapshot.faulted;
        self.error.clone_from(&snapshot.error);
        self.operations_this_tick = snapshot.operations_this_tick;
        *self.host.logs.borrow_mut() = snapshot.output.clone();
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LuaRuntimeBoundary;

impl LuaRuntimeBoundary {
    pub const fn new() -> Self {
        Self
    }

    pub const fn profile(&self) -> &'static LuaProfile {
        &LUA_PROFILE
    }

    /// Full Lua chip execution is introduced after the pure-module slice.
    pub fn unsupported(&self, program_id: &str, source_path: &Path) -> LuaDiagnostic {
        LuaDiagnostic {
            code: "lua-runtime-unavailable",
            message: format!(
                "unsupported runtime: Lua chip program `{program_id}` requires Stationeers host profile `{}`; the local Lua 5.2 runtime currently supports pure workspace module tests only, so no source was executed",
                LUA_PROFILE.id
            ),
            source_path: source_path.to_owned(),
            line: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LuaModuleRunner;

impl LuaModuleRunner {
    pub const fn new() -> Self {
        Self
    }

    pub fn check_syntax(
        &self,
        entry_path: &Path,
        limits: &LuaRunLimits,
    ) -> Result<(), LuaDiagnostic> {
        validate_limits(entry_path, limits)?;
        let entry_path = canonical_file(entry_path, "lua-entry-not-found")?;
        let entry_source = read_source(&entry_path, limits.max_source_bytes)?;
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT,
            LuaOptions::default(),
        )
        .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;
        lua.set_memory_limit(limits.memory_bytes)
            .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;
        lua.load(entry_source)
            .set_name(lua_chunk_name(&entry_path))
            .set_mode(ChunkMode::Text)
            .into_function()
            .map(|_| ())
            .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))
    }

    pub fn run(
        &self,
        entry_path: &Path,
        module_roots: &[PathBuf],
        limits: &LuaRunLimits,
    ) -> Result<LuaRunResult, LuaDiagnostic> {
        self.run_internal(entry_path, module_roots, limits, None)
    }

    pub fn run_with_host(
        &self,
        entry_path: &Path,
        module_roots: &[PathBuf],
        limits: &LuaRunLimits,
        host: &LuaHostMock,
    ) -> Result<LuaRunResult, LuaDiagnostic> {
        self.run_internal(entry_path, module_roots, limits, Some(host))
    }

    fn run_internal(
        &self,
        entry_path: &Path,
        module_roots: &[PathBuf],
        limits: &LuaRunLimits,
        host: Option<&LuaHostMock>,
    ) -> Result<LuaRunResult, LuaDiagnostic> {
        validate_limits(entry_path, limits)?;
        let entry_path = canonical_file(entry_path, "lua-entry-not-found")?;
        let mut roots = Vec::new();
        if let Some(parent) = entry_path.parent() {
            roots.push(parent.to_path_buf());
        }
        for root in module_roots {
            let root = root.canonicalize().map_err(|error| LuaDiagnostic {
                code: "lua-module-root-invalid",
                message: format!("could not resolve module root: {error}"),
                source_path: root.clone(),
                line: None,
            })?;
            if !root.is_dir() {
                return Err(LuaDiagnostic {
                    code: "lua-module-root-invalid",
                    message: "module root is not a directory".to_owned(),
                    source_path: root,
                    line: None,
                });
            }
            if !roots.contains(&root) {
                roots.push(root);
            }
        }

        let entry_source = read_source(&entry_path, limits.max_source_bytes)?;
        let entry_source_bytes = entry_source.len();
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::BIT,
            LuaOptions::default(),
        )
        .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;
        lua.set_memory_limit(limits.memory_bytes)
            .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;

        let output = Rc::new(RefCell::new(OutputCapture::default()));
        install_output_capture(&lua, Rc::clone(&output), limits.max_output_bytes)
            .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;
        install_denied_apis(&lua).map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;
        if let Some(host) = host {
            host.install(&lua)
                .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;
        }

        let resolver = Rc::new(RefCell::new(ModuleResolver {
            roots,
            loading: BTreeSet::new(),
            loaded_paths: Vec::new(),
            source_bytes: entry_source_bytes,
            max_modules: limits.max_modules,
            max_source_bytes: limits.max_source_bytes,
        }));
        install_require(&lua, Rc::clone(&resolver))
            .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;

        let instruction_count = Rc::new(Cell::new(0_u64));
        install_limits(
            &lua,
            Rc::clone(&instruction_count),
            limits.max_instructions,
            limits.max_recursion_depth,
            limits.wall_time,
        )
        .map_err(|error| diagnostic_from_mlua(error, &entry_path, &[]))?;

        let chunk_name = lua_chunk_name(&entry_path);
        let execution = lua
            .load(entry_source)
            .set_name(chunk_name)
            .set_mode(ChunkMode::Text)
            .exec();
        let loaded_paths = resolver.borrow().loaded_paths.clone();
        if let Err(error) = execution {
            return Err(diagnostic_from_mlua(error, &entry_path, &loaded_paths));
        }

        if let Some(host) = host {
            *host.logs.borrow_mut() = output.borrow().lines.clone();
        }

        Ok(LuaRunResult {
            output: output.borrow().lines.clone(),
            instructions: instruction_count.get(),
            loaded_modules: loaded_paths,
        })
    }
}

fn validate_limits(entry_path: &Path, limits: &LuaRunLimits) -> Result<(), LuaDiagnostic> {
    if limits.max_instructions == 0
        || limits.wall_time.is_zero()
        || limits.memory_bytes == 0
        || limits.max_output_bytes == 0
        || limits.max_modules == 0
        || limits.max_source_bytes == 0
        || limits.max_recursion_depth == 0
    {
        return Err(LuaDiagnostic {
            code: "lua-invalid-limits",
            message: "all Lua execution limits must be positive".to_owned(),
            source_path: entry_path.to_path_buf(),
            line: None,
        });
    }
    if limits.max_instructions > LUA_MAX_INSTRUCTIONS
        || limits.wall_time > LUA_MAX_WALL_TIME
        || limits.memory_bytes > LUA_MAX_MEMORY_BYTES
        || limits.max_output_bytes > LUA_MAX_OUTPUT_BYTES
        || limits.max_modules > LUA_MAX_MODULES
        || limits.max_source_bytes > LUA_MAX_SOURCE_BYTES
        || limits.max_recursion_depth > LUA_MAX_RECURSION_DEPTH
    {
        return Err(LuaDiagnostic {
            code: "lua-invalid-limits",
            message: format!(
                "Lua execution limits exceed the sandbox ceilings: instructions {LUA_MAX_INSTRUCTIONS}, wall time {} ms, memory {LUA_MAX_MEMORY_BYTES} bytes, output {LUA_MAX_OUTPUT_BYTES} bytes, modules {LUA_MAX_MODULES}, source {LUA_MAX_SOURCE_BYTES} bytes, recursion {LUA_MAX_RECURSION_DEPTH}",
                LUA_MAX_WALL_TIME.as_millis()
            ),
            source_path: entry_path.to_path_buf(),
            line: None,
        });
    }
    Ok(())
}

fn canonical_file(path: &Path, code: &'static str) -> Result<PathBuf, LuaDiagnostic> {
    let canonical = path.canonicalize().map_err(|error| LuaDiagnostic {
        code,
        message: format!("could not resolve Lua source: {error}"),
        source_path: path.to_path_buf(),
        line: None,
    })?;
    if !canonical.is_file() {
        return Err(LuaDiagnostic {
            code,
            message: "Lua source is not a file".to_owned(),
            source_path: canonical,
            line: None,
        });
    }
    Ok(canonical)
}

fn read_source(path: &Path, limit: usize) -> Result<String, LuaDiagnostic> {
    let bytes = fs::read(path).map_err(|error| LuaDiagnostic {
        code: "lua-source-read",
        message: format!("could not read Lua source: {error}"),
        source_path: path.to_path_buf(),
        line: None,
    })?;
    if bytes.len() > limit {
        return Err(LuaDiagnostic {
            code: "lua-source-limit",
            message: format!(
                "Lua source is {} bytes, exceeding the {limit}-byte limit",
                bytes.len()
            ),
            source_path: path.to_path_buf(),
            line: None,
        });
    }
    String::from_utf8(bytes).map_err(|error| LuaDiagnostic {
        code: "lua-source-encoding",
        message: format!("Lua source must be UTF-8: {error}"),
        source_path: path.to_path_buf(),
        line: None,
    })
}

#[derive(Default)]
struct OutputCapture {
    lines: Vec<String>,
    bytes: usize,
}

fn install_output_capture(
    lua: &Lua,
    output: Rc<RefCell<OutputCapture>>,
    max_bytes: usize,
) -> mlua::Result<()> {
    let print = lua.create_function(move |lua, values: Variadic<Value>| {
        let tostring: mlua::Function = lua.globals().get("tostring")?;
        let mut parts = Vec::with_capacity(values.len());
        for value in values {
            let rendered: mlua::LuaString = tostring.call(value)?;
            parts.push(rendered.to_string_lossy());
        }
        let line = parts.join("\t");
        let next = line.len().saturating_add(1);
        let mut capture = output.borrow_mut();
        if capture.bytes.saturating_add(next) > max_bytes {
            return Err(MluaError::RuntimeError(format!(
                "[lua-output-limit] captured output exceeds {max_bytes} bytes"
            )));
        }
        capture.bytes += next;
        capture.lines.push(line);
        Ok(())
    })?;
    lua.globals().set("print", print.clone())?;
    lua.globals().set("log", print)
}

fn install_denied_apis(lua: &Lua) -> mlua::Result<()> {
    for name in ["io", "os", "debug", "package", "ic", "device"] {
        lua.globals().set(name, denied_table(lua, name)?)?;
    }
    for name in [
        "dofile",
        "loadfile",
        "load",
        "loadstring",
        "collectgarbage",
        "pcall",
        "xpcall",
        "yield",
        "sleep",
    ] {
        lua.globals().set(name, denied_function(lua, name)?)?;
    }
    let math: Table = lua.globals().get("math")?;
    math.set("random", denied_function(lua, "math.random")?)?;
    math.set("randomseed", denied_function(lua, "math.randomseed")?)?;
    Ok(())
}

fn denied_table(lua: &Lua, name: &str) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let metatable = lua.create_table()?;
    let namespace = name.to_owned();
    metatable.set(
        "__index",
        lua.create_function(move |_, key: Value| {
            Err::<Value, _>(MluaError::RuntimeError(format!(
                "[lua-unsupported-api] `{namespace}.{}` is unavailable in profile `{LUA_PROFILE_ID}`",
                display_lua_key(&key)
            )))
        })?,
    )?;
    metatable.set("__metatable", "locked")?;
    table.set_metatable(Some(metatable))?;
    Ok(table)
}

fn denied_function(lua: &Lua, name: &str) -> mlua::Result<mlua::Function> {
    let name = name.to_owned();
    lua.create_function(move |_, _: Variadic<Value>| {
        Err::<Value, _>(MluaError::RuntimeError(format!(
            "[lua-unsupported-api] `{name}` is unavailable in profile `{LUA_PROFILE_ID}`"
        )))
    })
}

fn display_lua_key(value: &Value) -> String {
    match value {
        Value::String(value) => value.to_string_lossy(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        _ => value.type_name().to_owned(),
    }
}

struct ModuleResolver {
    roots: Vec<PathBuf>,
    loading: BTreeSet<String>,
    loaded_paths: Vec<PathBuf>,
    source_bytes: usize,
    max_modules: usize,
    max_source_bytes: usize,
}

fn install_require(lua: &Lua, resolver: Rc<RefCell<ModuleResolver>>) -> mlua::Result<()> {
    let loaded = lua.create_table()?;
    let loaded_for_require = loaded.clone();
    let require = lua.create_function(move |lua, name: String| {
        validate_module_name(&name)?;
        let cached: Value = loaded_for_require.raw_get(name.as_str())?;
        if !cached.is_nil() {
            return Ok(cached);
        }

        let path = {
            let mut resolver = resolver.borrow_mut();
            if resolver.loading.contains(&name) {
                return Err(MluaError::RuntimeError(format!(
                    "[lua-module-cycle] cyclic require for `{name}`"
                )));
            }
            let path = resolve_module(&resolver.roots, &name)?;
            if !resolver.loaded_paths.contains(&path)
                && resolver.loaded_paths.len() >= resolver.max_modules
            {
                return Err(MluaError::RuntimeError(format!(
                    "[lua-module-limit] module count exceeds {}",
                    resolver.max_modules
                )));
            }
            resolver.loading.insert(name.clone());
            if !resolver.loaded_paths.contains(&path) {
                resolver.loaded_paths.push(path.clone());
            }
            path
        };
        let source = {
            let mut resolver = resolver.borrow_mut();
            let max_source_bytes = resolver.max_source_bytes;
            match read_module_source(&path, max_source_bytes, &mut resolver.source_bytes) {
                Ok(source) => source,
                Err(error) => {
                    resolver.loading.remove(&name);
                    return Err(error);
                }
            }
        };

        let result = lua
            .load(source)
            .set_name(lua_chunk_name(&path))
            .set_mode(ChunkMode::Text)
            .eval::<Value>();
        resolver.borrow_mut().loading.remove(&name);
        let value = result?;
        let value = if value.is_nil() {
            Value::Boolean(true)
        } else {
            value
        };
        loaded_for_require.raw_set(name, value.clone())?;
        Ok(value)
    })?;
    lua.globals().set("require", require)
}

fn validate_module_name(name: &str) -> mlua::Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 255
        && name.split('.').all(|segment| {
            let mut characters = segment.chars();
            characters
                .next()
                .is_some_and(|character| character == '_' || character.is_ascii_alphabetic())
                && characters.all(|character| character == '_' || character.is_ascii_alphanumeric())
        });
    if valid {
        Ok(())
    } else {
        Err(MluaError::RuntimeError(format!(
            "[lua-module-path] invalid workspace module name `{name}`"
        )))
    }
}

fn resolve_module(roots: &[PathBuf], name: &str) -> mlua::Result<PathBuf> {
    let relative = name.replace('.', std::path::MAIN_SEPARATOR_STR);
    let mut matches = Vec::new();
    for root in roots {
        for candidate in [
            root.join(format!("{relative}.lua")),
            root.join(&relative).join("init.lua"),
        ] {
            if !candidate.is_file() {
                continue;
            }
            let canonical = candidate.canonicalize().map_err(MluaError::external)?;
            if !canonical.starts_with(root) {
                return Err(MluaError::RuntimeError(format!(
                    "[lua-module-path] module `{name}` resolves outside its configured root"
                )));
            }
            if !matches.contains(&canonical) {
                matches.push(canonical);
            }
        }
    }
    match matches.as_slice() {
        [] => Err(MluaError::RuntimeError(format!(
            "[lua-module-not-found] workspace module `{name}` was not found"
        ))),
        [path] => Ok(path.clone()),
        _ => Err(MluaError::RuntimeError(format!(
            "[lua-module-ambiguous] workspace module `{name}` resolves to multiple files"
        ))),
    }
}

fn read_module_source(path: &Path, limit: usize, used: &mut usize) -> mlua::Result<String> {
    let bytes = fs::read(path).map_err(MluaError::external)?;
    let aggregate = used.saturating_add(bytes.len());
    if bytes.len() > limit || aggregate > limit {
        return Err(MluaError::RuntimeError(format!(
            "[lua-source-limit] loading module {} would use {aggregate} source bytes, exceeding the {limit}-byte aggregate limit",
            path.display(),
        )));
    }
    *used = aggregate;
    String::from_utf8(bytes).map_err(|error| {
        MluaError::RuntimeError(format!(
            "[lua-source-encoding] module {} must be UTF-8: {error}",
            path.display()
        ))
    })
}

fn install_limits(
    lua: &Lua,
    instruction_count: Rc<Cell<u64>>,
    max_instructions: u64,
    max_recursion_depth: usize,
    wall_time: Duration,
) -> mlua::Result<()> {
    let depth = Rc::new(Cell::new(0_usize));
    let started = Instant::now();
    lua.set_global_hook(
        HookTriggers::new()
            .on_calls()
            .on_returns()
            .every_nth_instruction(HOOK_INSTRUCTION_INTERVAL),
        move |_, debug| {
            if started.elapsed() > wall_time {
                return Err(MluaError::RuntimeError(format!(
                    "[lua-wall-time-limit] execution exceeded {} ms",
                    wall_time.as_millis()
                )));
            }
            match debug.event() {
                DebugEvent::Call => {
                    let next = depth.get().saturating_add(1);
                    depth.set(next);
                    if next > max_recursion_depth {
                        return Err(MluaError::RuntimeError(format!(
                            "[lua-recursion-limit] call depth exceeds {max_recursion_depth}"
                        )));
                    }
                }
                DebugEvent::Ret => depth.set(depth.get().saturating_sub(1)),
                DebugEvent::Count => {
                    let next = instruction_count
                        .get()
                        .saturating_add(u64::from(HOOK_INSTRUCTION_INTERVAL));
                    instruction_count.set(next);
                    if next > max_instructions {
                        return Err(MluaError::RuntimeError(format!(
                            "[lua-instruction-limit] execution exceeds {max_instructions} instructions"
                        )));
                    }
                }
                DebugEvent::TailCall
                | DebugEvent::Line
                | DebugEvent::Unknown(_) => {}
            }
            Ok(VmState::Continue)
        },
    )
}

fn diagnostic_from_mlua(
    error: MluaError,
    entry_path: &Path,
    loaded_paths: &[PathBuf],
) -> LuaDiagnostic {
    let message = error.to_string();
    let code = if message.contains("[lua-instruction-limit]") {
        "lua-instruction-limit"
    } else if message.contains("[lua-wall-time-limit]") {
        "lua-wall-time-limit"
    } else if message.contains("[lua-recursion-limit]") {
        "lua-recursion-limit"
    } else if message.contains("[lua-output-limit]") {
        "lua-output-limit"
    } else if message.contains("[lua-module-limit]") {
        "lua-module-limit"
    } else if message.contains("[lua-module-cycle]") {
        "lua-module-cycle"
    } else if message.contains("[lua-module-not-found]") {
        "lua-module-not-found"
    } else if message.contains("[lua-module-ambiguous]") {
        "lua-module-ambiguous"
    } else if message.contains("[lua-module-path]") {
        "lua-module-path"
    } else if message.contains("[lua-source-limit]") {
        "lua-source-limit"
    } else if message.contains("[lua-source-encoding]") {
        "lua-source-encoding"
    } else if message.contains("[lua-unsupported-api]") {
        "lua-unsupported-api"
    } else if matches!(error, MluaError::MemoryError(_)) {
        "lua-memory-limit"
    } else if matches!(error, MluaError::SyntaxError { .. }) {
        "lua-syntax-error"
    } else {
        "lua-runtime-error"
    };
    let (source_path, line) = source_location(&message, entry_path, loaded_paths);
    LuaDiagnostic {
        code,
        message,
        source_path,
        line,
    }
}

fn source_location(
    message: &str,
    entry_path: &Path,
    loaded_paths: &[PathBuf],
) -> (PathBuf, Option<usize>) {
    let paths = loaded_paths
        .iter()
        .rev()
        .map(PathBuf::as_path)
        .chain(std::iter::once(entry_path))
        .collect::<Vec<_>>();
    let mut best: Option<(usize, usize, PathBuf, Option<usize>)> = None;
    for path in &paths {
        let displayed = path.display().to_string();
        let portable = displayed.strip_prefix(r"\\?\").unwrap_or(&displayed);
        for candidate in [displayed.as_str(), portable] {
            if let Some(offset) = message.find(candidate) {
                consider_source_match(&mut best, message, offset, candidate.len(), path);
            }
        }
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let unique = paths
            .iter()
            .filter(|candidate| {
                candidate.file_name().and_then(|name| name.to_str()) == Some(file_name)
            })
            .count()
            == 1;
        if unique && let Some(offset) = message.find(file_name) {
            consider_source_match(&mut best, message, offset, file_name.len(), path);
        }
    }
    best.map_or_else(
        || (entry_path.to_path_buf(), None),
        |(_, _, path, line)| (path, line),
    )
}

fn consider_source_match(
    best: &mut Option<(usize, usize, PathBuf, Option<usize>)>,
    message: &str,
    offset: usize,
    matched_length: usize,
    path: &Path,
) {
    let suffix = &message[offset + matched_length..];
    let line = suffix
        .strip_prefix(':')
        .and_then(|suffix| suffix.split(':').next())
        .and_then(|line| line.parse().ok());
    let replace = best
        .as_ref()
        .is_none_or(|(best_offset, best_length, _, _)| {
            offset < *best_offset || (offset == *best_offset && matched_length > *best_length)
        });
    if replace {
        *best = Some((offset, matched_length, path.to_path_buf(), line));
    }
}

fn lua_chunk_name(path: &Path) -> String {
    let displayed = path.display().to_string();
    let portable = displayed.strip_prefix(r"\\?\").unwrap_or(&displayed);
    format!("@{portable}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_keeps_full_chip_execution_fail_closed() {
        let diagnostic = LuaRuntimeBoundary::new().unsupported("controller", Path::new("main.lua"));
        assert_eq!(diagnostic.code, "lua-runtime-unavailable");
        assert_eq!(diagnostic.source_path, Path::new("main.lua"));
        assert!(diagnostic.message.contains("no source was executed"));
        assert_eq!(LuaRuntimeBoundary::new().profile().lua_version, "5.2");
    }
}
