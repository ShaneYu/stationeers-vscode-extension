use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use ic10_sim::{
    LUA_MAX_INSTRUCTIONS, LUA_MAX_MEMORY_BYTES, LUA_MAX_MODULES, LUA_MAX_OUTPUT_BYTES,
    LUA_MAX_RECURSION_DEPTH, LUA_MAX_SOURCE_BYTES, LUA_PROFILE_ID, Scalar,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScenarioTest {
    pub schema_version: u32,
    pub scenario: PathBuf,
    #[serde(default)]
    pub seed: u64,
    pub cases: Vec<TestCase>,
}

impl ScenarioTest {
    pub fn load(path: &Path) -> Result<Self, TestFileError> {
        let source = fs::read_to_string(path).map_err(|source| TestFileError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let fixture: Self =
            serde_json::from_str(&source).map_err(|source| TestFileError::Json {
                path: path.to_path_buf(),
                source,
            })?;
        if fixture.schema_version != 1 {
            return Err(TestFileError::Version {
                path: path.to_path_buf(),
                found: fixture.schema_version,
            });
        }
        if is_obsolete_workspace_path(&fixture.scenario) {
            return Err(TestFileError::Validation {
                path: path.to_path_buf(),
                message: format!(
                    "scenario reference `{}` uses an obsolete workspace filename; rename it to .icsim",
                    fixture.scenario.display()
                ),
            });
        }
        if fixture.cases.is_empty() {
            return Err(TestFileError::Validation {
                path: path.to_path_buf(),
                message: "`cases` must contain at least one case".to_owned(),
            });
        }
        for test_case in &fixture.cases {
            if test_case.name.trim().is_empty() {
                return Err(TestFileError::Validation {
                    path: path.to_path_buf(),
                    message: "case names must not be empty".to_owned(),
                });
            }
            if test_case.max_ticks == 0 || test_case.max_operations == 0 {
                return Err(TestFileError::Validation {
                    path: path.to_path_buf(),
                    message: format!(
                        "case `{}` requires positive maxTicks and maxOperations",
                        test_case.name
                    ),
                });
            }
            if let Some(ExecutionSpec::LuaModule {
                profile,
                module_roots,
                memory_limit_bytes,
                max_output_bytes,
                max_modules,
                max_source_bytes,
                max_recursion_depth,
            }) = &test_case.execution
            {
                let fixture_in_testing = path
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("testing"));
                if !(is_portable_relative_path(&fixture.scenario)
                    || (fixture_in_testing && is_workspace_relative_path(&fixture.scenario)))
                {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: "luaModule execution requires a test-relative scenario path without parent traversal".to_owned(),
                    });
                }
                if test_case.program.is_none() {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` requires focusProgram for luaModule execution",
                            test_case.name
                        ),
                    });
                }
                if profile != LUA_PROFILE_ID {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` requests unsupported Lua profile `{profile}`; expected `{LUA_PROFILE_ID}`",
                            test_case.name
                        ),
                    });
                }
                if module_roots.iter().any(|root| {
                    !(is_portable_relative_path(root)
                        || (fixture_in_testing && is_workspace_relative_path(root)))
                }) {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` requires test-relative moduleRoots",
                            test_case.name
                        ),
                    });
                }
                if *memory_limit_bytes == 0
                    || *max_output_bytes == 0
                    || *max_modules == 0
                    || *max_source_bytes == 0
                    || *max_recursion_depth == 0
                {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` requires positive Lua execution limits",
                            test_case.name
                        ),
                    });
                }
                if test_case.max_operations > LUA_MAX_INSTRUCTIONS
                    || *memory_limit_bytes > LUA_MAX_MEMORY_BYTES
                    || *max_output_bytes > LUA_MAX_OUTPUT_BYTES
                    || *max_modules > LUA_MAX_MODULES
                    || *max_source_bytes > LUA_MAX_SOURCE_BYTES
                    || *max_recursion_depth > LUA_MAX_RECURSION_DEPTH
                {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` exceeds a hard Lua sandbox limit",
                            test_case.name
                        ),
                    });
                }
                if !test_case.initial.is_empty()
                    || !test_case.timeline.is_empty()
                    || !test_case.drivers.is_empty()
                    || !test_case.assertions.is_empty()
                    || test_case.snapshot.is_some()
                {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` uses world-only fields with luaModule execution; put assertions in the Lua entry script",
                            test_case.name
                        ),
                    });
                }
            }
            if let Some(entry) = test_case
                .timeline
                .iter()
                .find(|entry| entry.tick > test_case.max_ticks)
            {
                return Err(TestFileError::Validation {
                    path: path.to_path_buf(),
                    message: format!(
                        "case `{}` has timeline tick {} beyond maxTicks {}",
                        test_case.name, entry.tick, test_case.max_ticks
                    ),
                });
            }
            if test_case.drivers.len() > 32 {
                return Err(TestFileError::Validation {
                    path: path.to_path_buf(),
                    message: format!("case `{}` exceeds 32 scripted drivers", test_case.name),
                });
            }
            let mut rule_count = 0usize;
            for driver in &test_case.drivers {
                if driver.id.trim().is_empty()
                    || driver.model.trim().is_empty()
                    || driver.version == 0
                {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` has an invalid scripted driver identity",
                            test_case.name
                        ),
                    });
                }
                rule_count += driver.rules.len();
                for rule in &driver.rules {
                    if rule.when.target.trim().is_empty() || rule.actions.is_empty() {
                        return Err(TestFileError::Validation {
                            path: path.to_path_buf(),
                            message: format!(
                                "case `{}` driver `{}` has an incomplete rule",
                                test_case.name, driver.id
                            ),
                        });
                    }
                    validate_script_actions(&rule.actions).map_err(|message| {
                        TestFileError::Validation {
                            path: path.to_path_buf(),
                            message: format!(
                                "case `{}` driver `{}`: {message}",
                                test_case.name, driver.id
                            ),
                        }
                    })?;
                }
            }
            if rule_count > 256 {
                return Err(TestFileError::Validation {
                    path: path.to_path_buf(),
                    message: format!("case `{}` exceeds 256 scripted rules", test_case.name),
                });
            }
            for assertion in &test_case.assertions {
                assertion
                    .expression()
                    .map_err(|message| TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!("case `{}`: {message}", test_case.name),
                    })?;
                if assertion
                    .at_tick
                    .is_some_and(|tick| tick > test_case.max_ticks)
                    || assertion
                        .within_ticks
                        .is_some_and(|tick| tick > test_case.max_ticks)
                {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` has an assertion deadline beyond maxTicks",
                            test_case.name
                        ),
                    });
                }
                if assertion.tolerance.as_ref().is_some_and(|tolerance| {
                    !tolerance.absolute.is_finite()
                        || !tolerance.relative.is_finite()
                        || tolerance.absolute < 0.0
                        || tolerance.relative < 0.0
                }) {
                    return Err(TestFileError::Validation {
                        path: path.to_path_buf(),
                        message: format!(
                            "case `{}` has an invalid negative or non-finite tolerance",
                            test_case.name
                        ),
                    });
                }
            }
        }
        Ok(fixture)
    }
}

fn is_obsolete_workspace_path(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".ic10sim.json")
        || name.ends_with(".ic10test.json")
        || name.ends_with(".ic10sim.layout.json")
        || name.ends_with(".stationeerssim.json")
        || name.ends_with(".stationeerstest.json")
        || name.ends_with(".stationeerssim.layout.json")
}

fn validate_script_actions(actions: &[ScriptAction]) -> Result<(), String> {
    fn visit(actions: &[ScriptAction], depth: usize, count: &mut usize) -> Result<(), String> {
        if depth > 8 {
            return Err("scheduled actions exceed nesting limit 8".to_owned());
        }
        for action in actions {
            *count += 1;
            if *count > 1024 {
                return Err("scripted actions exceed limit 1024".to_owned());
            }
            match action {
                ScriptAction::MoveSlot { from, to }
                    if from.trim().is_empty() || to.trim().is_empty() =>
                {
                    return Err("moveSlot requires source and destination slots".to_owned());
                }
                ScriptAction::Publish { channel, .. } if *channel > 7 => {
                    return Err("publish channel must be from 0 to 7".to_owned());
                }
                ScriptAction::Schedule { actions, .. } if actions.is_empty() => {
                    return Err("schedule requires at least one nested action".to_owned());
                }
                ScriptAction::Schedule { actions, .. } => visit(actions, depth + 1, count)?,
                _ => {}
            }
        }
        Ok(())
    }
    visit(actions, 0, &mut 0)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestCase {
    pub name: String,
    #[serde(default = "default_ticks")]
    pub max_ticks: u64,
    #[serde(default = "default_operations")]
    pub max_operations: u64,
    #[serde(default)]
    /// Neutral program/VM selector. `focusIc` remains accepted for legacy
    /// files and the early draft `program` spelling remains readable.
    #[serde(
        rename = "focusProgram",
        alias = "program",
        alias = "focusIc",
        skip_serializing_if = "Option::is_none"
    )]
    pub program: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionSpec>,
    #[serde(default)]
    pub initial: BTreeMap<String, Scalar>,
    #[serde(default)]
    pub timeline: Vec<TimelineEntry>,
    #[serde(default)]
    pub drivers: Vec<ScriptedDriver>,
    #[serde(default, rename = "expect")]
    pub assertions: Vec<Assertion>,
    #[serde(default)]
    pub expect_error: Option<ErrorExpectation>,
    #[serde(default)]
    pub parameters: Vec<ParameterSet>,
    #[serde(default)]
    pub snapshot: Option<Snapshot>,
}

pub fn is_portable_relative_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    !value.trim().is_empty()
        && !path.is_absolute()
        && !value.starts_with(['/', '\\'])
        && !matches!(bytes, [drive, b':', ..] if drive.is_ascii_alphabetic())
        && !value.split(['/', '\\']).any(|component| component == "..")
}

/// A relative path that may use parent traversal; callers must canonicalize it
/// and enforce the workspace boundary before opening it.
pub fn is_workspace_relative_path(path: &Path) -> bool {
    let value = path.to_string_lossy();
    !value.trim().is_empty()
        && !path.is_absolute()
        && !value.starts_with(['/', '\\'])
        && !matches!(value.as_bytes(), [drive, b':', ..] if drive.is_ascii_alphabetic())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum ExecutionSpec {
    LuaModule {
        #[serde(default = "default_lua_profile")]
        profile: String,
        #[serde(default, rename = "moduleRoots")]
        module_roots: Vec<PathBuf>,
        #[serde(default = "default_lua_memory_limit", rename = "memoryLimitBytes")]
        memory_limit_bytes: usize,
        #[serde(default = "default_lua_output_limit", rename = "maxOutputBytes")]
        max_output_bytes: usize,
        #[serde(default = "default_lua_module_limit", rename = "maxModules")]
        max_modules: usize,
        #[serde(default = "default_lua_source_limit", rename = "maxSourceBytes")]
        max_source_bytes: usize,
        #[serde(default = "default_lua_recursion_limit", rename = "maxRecursionDepth")]
        max_recursion_depth: usize,
    },
}

fn default_lua_profile() -> String {
    LUA_PROFILE_ID.to_owned()
}

fn default_lua_memory_limit() -> usize {
    16 * 1024 * 1024
}

fn default_lua_output_limit() -> usize {
    64 * 1024
}

fn default_lua_module_limit() -> usize {
    64
}

fn default_lua_source_limit() -> usize {
    1024 * 1024
}

fn default_lua_recursion_limit() -> usize {
    128
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptedDriver {
    pub id: String,
    #[serde(default = "default_driver_model")]
    pub model: String,
    #[serde(default = "default_driver_version")]
    pub version: u32,
    pub rules: Vec<ScriptedRule>,
}

fn default_driver_model() -> String {
    "scenario.scripted".to_owned()
}

fn default_driver_version() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptedRule {
    #[serde(default)]
    pub name: Option<String>,
    pub when: ScriptTrigger,
    pub actions: Vec<ScriptAction>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptTrigger {
    pub target: String,
    #[serde(default)]
    pub equals: Option<Scalar>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "action", deny_unknown_fields)]
pub enum ScriptAction {
    Set {
        target: String,
        value: Scalar,
    },
    MoveSlot {
        from: String,
        to: String,
    },
    Publish {
        network: String,
        channel: u8,
        value: Scalar,
    },
    Schedule {
        #[serde(rename = "afterTicks")]
        after_ticks: u64,
        actions: Vec<ScriptAction>,
    },
}

fn default_ticks() -> u64 {
    100
}
fn default_operations() -> u64 {
    100_000
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParameterSet {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub values: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimelineEntry {
    pub tick: u64,
    #[serde(default)]
    pub set: BTreeMap<String, Scalar>,
    #[serde(default)]
    pub events: Vec<StateEvent>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateEvent {
    pub target: String,
    pub value: Scalar,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assertion {
    #[serde(default)]
    pub expression: Option<String>,
    #[serde(default)]
    pub eventually: Option<String>,
    #[serde(default)]
    pub always: Option<String>,
    #[serde(default)]
    pub at_tick: Option<u64>,
    #[serde(default)]
    pub within_ticks: Option<u64>,
    #[serde(default)]
    pub expected: Option<Scalar>,
    #[serde(default)]
    pub tolerance: Option<Tolerance>,
}

impl Assertion {
    pub fn expression(&self) -> Result<&str, String> {
        let expressions: Vec<_> = [
            self.expression.as_deref(),
            self.eventually.as_deref(),
            self.always.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        if expressions.len() != 1 {
            return Err(
                "assertion requires exactly one of `expression`, `eventually`, or `always`"
                    .to_owned(),
            );
        }
        if self.eventually.is_none() && self.within_ticks.is_some() {
            return Err("`withinTicks` is only valid with `eventually`".to_owned());
        }
        Ok(expressions[0])
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Tolerance {
    #[serde(default)]
    pub absolute: f64,
    #[serde(default)]
    pub relative: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorExpectation {
    pub kind: ErrorKind,
    #[serde(default)]
    pub message_contains: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorKind {
    Compile,
    Runtime,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub values: BTreeMap<String, Scalar>,
}

#[derive(Debug)]
pub enum TestFileError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    Version {
        path: PathBuf,
        found: u32,
    },
    Validation {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for TestFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Json { path, source } => write!(
                formatter,
                "invalid test fixture {}: {source}",
                path.display()
            ),
            Self::Version { path, found } => write!(
                formatter,
                "{} uses unsupported schemaVersion {found}; migrate to schemaVersion 1",
                path.display()
            ),
            Self::Validation { path, message } => write!(
                formatter,
                "invalid test fixture {}: {message}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for TestFileError {}
