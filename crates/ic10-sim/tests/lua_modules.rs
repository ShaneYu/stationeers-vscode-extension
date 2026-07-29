use std::fs;
use std::path::{Path, PathBuf};

use ic10_sim::{LUA_MAX_MEMORY_BYTES, LuaDiagnostic, LuaModuleRunner, LuaRunLimits, LuaRunResult};
use tempfile::TempDir;

struct LuaWorkspace {
    root: TempDir,
}

impl LuaWorkspace {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("create temporary Lua workspace"),
        }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn write(&self, relative: &str, source: &str) -> PathBuf {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create Lua fixture directory");
        }
        fs::write(&path, source).expect("write Lua fixture");
        path
    }

    fn run(&self, entry: &Path, limits: &LuaRunLimits) -> Result<LuaRunResult, LuaDiagnostic> {
        LuaModuleRunner::new().run(entry, &[self.root.path().to_path_buf()], limits)
    }
}

fn default_limits() -> LuaRunLimits {
    LuaRunLimits::default()
}

fn assert_diagnostic(
    result: Result<LuaRunResult, LuaDiagnostic>,
    expected_code: &str,
) -> LuaDiagnostic {
    let diagnostic = result.expect_err("Lua execution should fail");
    assert_eq!(diagnostic.code, expected_code, "{diagnostic}");
    diagnostic
}

#[test]
fn require_caches_modules_and_captures_output() {
    let workspace = LuaWorkspace::new();
    let module = workspace.write(
        "modules/counter.lua",
        "print('loading counter')\nreturn { value = 42 }\n",
    );
    let entry = workspace.write(
        "entry.lua",
        "local first = require('modules.counter')\n\
         local second = require('modules.counter')\n\
         assert(first == second)\n\
         assert(bit32.band(7, 3) == 3)\n\
         print('value', first.value)\n",
    );

    let result = workspace
        .run(&entry, &default_limits())
        .expect("pure module execution should pass");

    assert_eq!(result.output, ["loading counter", "value\t42"]);
    assert_eq!(
        result.loaded_modules,
        [module.canonicalize().expect("canonical module path")]
    );
}

#[test]
fn repeated_runs_are_deterministic() {
    let workspace = LuaWorkspace::new();
    workspace.write("lib/value.lua", "return { answer = 6 * 7 }\n");
    let entry = workspace.write(
        "entry.lua",
        "local value = require('lib.value')\nprint(value.answer)\n",
    );

    let first = workspace
        .run(&entry, &default_limits())
        .expect("first run should pass");
    let second = workspace
        .run(&entry, &default_limits())
        .expect("second run should pass");

    assert_eq!(first, second);
}

#[test]
fn syntax_error_reports_entry_source_location() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write("entry.lua", "local value = 1\nlocal broken =\n");

    let diagnostic =
        assert_diagnostic(workspace.run(&entry, &default_limits()), "lua-syntax-error");

    assert_eq!(
        diagnostic.source_path,
        entry.canonicalize().expect("canonical entry path")
    );
    assert_eq!(diagnostic.line, Some(3));
}

#[test]
fn assertion_reports_required_module_source_location() {
    let workspace = LuaWorkspace::new();
    let module = workspace.write(
        "spec/failing.lua",
        "local value = 41\nassert(value == 42, 'expected answer')\nreturn true\n",
    );
    let entry = workspace.write("entry.lua", "require('spec.failing')\n");

    let diagnostic = assert_diagnostic(
        workspace.run(&entry, &default_limits()),
        "lua-runtime-error",
    );

    assert_eq!(
        diagnostic.source_path,
        module.canonicalize().expect("canonical module path"),
        "{diagnostic:?}"
    );
    assert_eq!(diagnostic.line, Some(2));
    assert!(diagnostic.message.contains("expected answer"));
}

#[test]
fn missing_invalid_ambiguous_and_cyclic_modules_are_rejected() {
    let missing_workspace = LuaWorkspace::new();
    let missing_entry = missing_workspace.write("entry.lua", "require('not.present')\n");
    assert_diagnostic(
        missing_workspace.run(&missing_entry, &default_limits()),
        "lua-module-not-found",
    );

    let invalid_workspace = LuaWorkspace::new();
    let invalid_entry = invalid_workspace.write("entry.lua", "require('../escape')\n");
    assert_diagnostic(
        invalid_workspace.run(&invalid_entry, &default_limits()),
        "lua-module-path",
    );

    let ambiguous_workspace = LuaWorkspace::new();
    let second_root = tempfile::tempdir().expect("create second module root");
    ambiguous_workspace.write("shared.lua", "return 'first'\n");
    fs::write(second_root.path().join("shared.lua"), "return 'second'\n")
        .expect("write ambiguous module");
    let ambiguous_entry = ambiguous_workspace.write("entry.lua", "require('shared')\n");
    assert_diagnostic(
        LuaModuleRunner::new().run(
            &ambiguous_entry,
            &[
                ambiguous_workspace.root.path().to_path_buf(),
                second_root.path().to_path_buf(),
            ],
            &default_limits(),
        ),
        "lua-module-ambiguous",
    );

    let cyclic_workspace = LuaWorkspace::new();
    cyclic_workspace.write("cycle/a.lua", "return require('cycle.b')\n");
    cyclic_workspace.write("cycle/b.lua", "return require('cycle.a')\n");
    let cyclic_entry = cyclic_workspace.write("entry.lua", "require('cycle.a')\n");
    assert_diagnostic(
        cyclic_workspace.run(&cyclic_entry, &default_limits()),
        "lua-module-cycle",
    );
}

#[test]
fn forbidden_host_and_unsafe_apis_are_rejected() {
    let attempts = [
        ("io", "return io.open('secrets.txt')"),
        ("os", "return os.getenv('HOME')"),
        ("debug", "return debug.getinfo(1)"),
        ("package", "return package.loadlib('x', 'y')"),
        ("device", "return device.get('Pressure')"),
        ("ic", "return ic.get('Setting')"),
        ("load", "return load('return 1')"),
        ("pcall", "return pcall(function() return 1 end)"),
        (
            "xpcall",
            "return xpcall(function() return 1 end, function() end)",
        ),
        ("random", "return math.random()"),
    ];

    for (name, source) in attempts {
        let workspace = LuaWorkspace::new();
        let entry = workspace.write("entry.lua", source);
        let diagnostic = assert_diagnostic(
            workspace.run(&entry, &default_limits()),
            "lua-unsupported-api",
        );
        assert!(
            diagnostic.message.contains(name),
            "{name} was not identified in {diagnostic}"
        );
    }
}

#[test]
fn instruction_limit_stops_infinite_loop() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write("entry.lua", "while true do end\n");
    let limits = LuaRunLimits {
        max_instructions: 2_000,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-instruction-limit");
}

#[test]
fn instruction_limit_cannot_be_caught_and_retried() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write(
        "entry.lua",
        "while true do\n  pcall(function() while true do end end)\nend\n",
    );
    let limits = LuaRunLimits {
        max_instructions: 2_000,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-unsupported-api");
}

#[test]
fn recursion_limit_stops_unbounded_calls() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write(
        "entry.lua",
        "local function recurse()\n  return 1 + recurse()\nend\nrecurse()\n",
    );
    let limits = LuaRunLimits {
        max_recursion_depth: 12,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-recursion-limit");
}

#[test]
fn output_limit_bounds_captured_prints() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write("entry.lua", "print('123456789')\n");
    let limits = LuaRunLimits {
        max_output_bytes: 8,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-output-limit");
}

#[test]
fn memory_limit_stops_large_allocations() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write(
        "entry.lua",
        "local values = {}\n\
         for index = 1, 100000 do\n\
           values[index] = string.rep('x', 128)\n\
         end\n",
    );
    let limits = LuaRunLimits {
        max_instructions: 10_000_000,
        memory_bytes: 256 * 1024,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-memory-limit");
}

#[test]
fn sandbox_rejects_limits_above_host_ceilings() {
    let workspace = LuaWorkspace::new();
    let entry = workspace.write("entry.lua", "return true\n");
    let limits = LuaRunLimits {
        memory_bytes: LUA_MAX_MEMORY_BYTES + 1,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-invalid-limits");
}

#[test]
fn module_count_limit_bounds_dependency_graph() {
    let workspace = LuaWorkspace::new();
    workspace.write("chain/one.lua", "return require('chain.two')\n");
    workspace.write("chain/two.lua", "return require('chain.three')\n");
    workspace.write("chain/three.lua", "return true\n");
    let entry = workspace.write("entry.lua", "require('chain.one')\n");
    let limits = LuaRunLimits {
        max_modules: 2,
        ..default_limits()
    };

    assert_diagnostic(workspace.run(&entry, &limits), "lua-module-limit");
}

#[test]
fn source_limit_applies_to_entry_and_required_modules() {
    let entry_workspace = LuaWorkspace::new();
    let oversized_entry = entry_workspace.write("entry.lua", "print('too much source')\n");
    let entry_limits = LuaRunLimits {
        max_source_bytes: 8,
        ..default_limits()
    };
    let entry_diagnostic = assert_diagnostic(
        entry_workspace.run(&oversized_entry, &entry_limits),
        "lua-source-limit",
    );
    assert_eq!(
        entry_diagnostic.source_path,
        oversized_entry
            .canonicalize()
            .expect("canonical oversized entry path")
    );

    let module_workspace = LuaWorkspace::new();
    let oversized_module = module_workspace.write(
        "large.lua",
        "return 'this module is deliberately too large'\n",
    );
    let module_entry = module_workspace.write("entry.lua", "require('large')\n");
    let module_limits = LuaRunLimits {
        max_source_bytes: 20,
        ..default_limits()
    };
    let module_diagnostic = assert_diagnostic(
        module_workspace.run(&module_entry, &module_limits),
        "lua-source-limit",
    );
    assert_eq!(
        module_diagnostic.source_path,
        oversized_module
            .canonicalize()
            .expect("canonical oversized module path")
    );
}
