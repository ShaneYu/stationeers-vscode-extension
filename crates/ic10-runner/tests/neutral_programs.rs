use std::fs;

use ic10_runner::{
    RunLimits, RunRequest, ScenarioTest, Status, resolve_lua_workspace_path, run_files,
};
use tempfile::tempdir;

#[test]
fn neutral_lua_selector_executes_a_world_lua_program() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("world.stationeerssim.json"),
        r#"{"schemaVersion":1,"programs":[{"id":"lua-main","path":"main.lua","language":"lua"}],"devices":[{"id":"housing","prefab":"StructureCircuitHousing","program":"lua-main"}]}"#,
    ).unwrap();
    fs::write(
        directory.path().join("main.lua"),
        "function tick(dt) print('lua tick', dt) end\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("world.stationeerstest.json"),
        r#"{"schemaVersion":1,"scenario":"world.stationeerssim.json","cases":[{"name":"lua","program":"lua-main"}]}"#,
    ).unwrap();
    let result = run_files(&RunRequest {
        paths: vec![directory.path().join("world.stationeerstest.json")],
        name_filter: None,
        limits: RunLimits::default(),
        lua_library_paths: vec![],
    });
    assert_eq!(result.passed, 1, "{result:#?}");
    assert_eq!(result.files[0].cases[0].status, Status::Passed);
}

#[test]
fn mixed_world_with_lua_program_executes_both_languages() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("world.stationeerssim.json"),
        r#"{
          "schemaVersion": 1,
          "programs": [
            {"id":"ic-main","path":"main.ic10","language":"ic10"},
            {"id":"lua-main","path":"main.lua","language":"lua"}
          ],
          "devices": [
            {"id":"ic-housing","prefab":"StructureCircuitHousing","program":"ic-main"},
            {"id":"lua-housing","prefab":"StructureCircuitHousing","program":"lua-main"}
          ]
        }"#,
    )
    .unwrap();
    fs::write(directory.path().join("main.ic10"), "move r0 42\nyield\n").unwrap();
    fs::write(directory.path().join("main.lua"), "function tick(dt) end\n").unwrap();
    fs::write(
        directory.path().join("world.stationeerstest.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{"name": "mixed world executes", "focusProgram": "ic-main"}]
        }"#,
    )
    .unwrap();

    let result = run_files(&RunRequest {
        paths: vec![directory.path().join("world.stationeerstest.json")],
        name_filter: None,
        limits: RunLimits::default(),
        lua_library_paths: vec![],
    });

    assert_eq!(result.passed, 1, "{result:#?}");
    assert_eq!(result.failed, 0);
    assert!(result.files[0].cases[0].ticks > 0);
    assert!(result.files[0].cases[0].operations > 0);
}

#[test]
fn legacy_focus_ic_is_accepted_and_serializes_as_neutral_program() {
    let fixture: ScenarioTest = serde_json::from_str(
        r#"{"schemaVersion":1,"scenario":"world.stationeerssim.json","cases":[{"name":"legacy","focusIc":"housing"}]}"#,
    )
    .unwrap();
    assert_eq!(fixture.cases[0].program.as_deref(), Some("housing"));
    let serialized = serde_json::to_string(&fixture).unwrap();
    assert!(serialized.contains("\"focusProgram\":\"housing\""));
    assert!(!serialized.contains("focusIc"));
}

#[test]
fn explicit_lua_module_case_runs_without_constructing_a_world_vm() {
    let directory = tempdir().unwrap();
    fs::create_dir_all(directory.path().join("lib")).unwrap();
    fs::write(
        directory.path().join("world.stationeerssim.json"),
        r#"{"schemaVersion":1,"programs":[{"id":"lua-tests","path":"tests.lua","language":"lua"}],"devices":[]}"#,
    )
    .unwrap();
    fs::write(
        directory.path().join("lib/arithmetic.lua"),
        "local module = {}\nfunction module.add(a, b) return a + b end\nreturn module\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("tests.lua"),
        "local arithmetic = require('arithmetic')\nassert(arithmetic.add(20, 22) == 42)\nprint('answer', 42)\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("world.stationeerstest.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{
            "name": "pure arithmetic",
            "focusProgram": "lua-tests",
            "execution": {"kind": "luaModule", "moduleRoots": ["lib"]}
          }]
        }"#,
    )
    .unwrap();

    let result = run_files(&RunRequest {
        paths: vec![directory.path().join("world.stationeerstest.json")],
        name_filter: None,
        limits: RunLimits::default(),
        lua_library_paths: vec![],
    });

    assert_eq!(result.passed, 1, "{result:#?}");
    assert_eq!(result.failed, 0);
    assert_eq!(result.files[0].cases[0].status, Status::Passed);
    assert_eq!(result.files[0].cases[0].captured_output, ["answer\t42"]);
    assert_eq!(result.files[0].cases[0].ticks, 0);
}

#[test]
fn lua_module_failure_keeps_required_module_location() {
    let directory = tempdir().unwrap();
    fs::create_dir_all(directory.path().join("spec")).unwrap();
    fs::write(
        directory.path().join("world.stationeerssim.json"),
        r#"{"schemaVersion":1,"programs":[{"id":"lua-tests","path":"tests.lua","language":"lua"}],"devices":[]}"#,
    )
    .unwrap();
    let failing_module = directory.path().join("spec/failing.lua");
    fs::write(
        &failing_module,
        "local actual = 41\nassert(actual == 42, 'expected answer')\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("tests.lua"),
        "require('spec.failing')\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("world.stationeerstest.json"),
        r#"{"schemaVersion":1,"scenario":"world.stationeerssim.json","cases":[{"name":"failure location","focusProgram":"lua-tests","execution":{"kind":"luaModule"}}]}"#,
    )
    .unwrap();

    let result = run_files(&RunRequest {
        paths: vec![directory.path().join("world.stationeerstest.json")],
        name_filter: None,
        limits: RunLimits::default(),
        lua_library_paths: vec![],
    });

    assert_eq!(result.failed, 1);
    let failure = &result.files[0].cases[0].failures[0];
    assert_eq!(
        failure.source.as_deref(),
        Some(failing_module.canonicalize().unwrap().as_path())
    );
    assert_eq!(failure.line, Some(2));
    assert!(failure.message.contains("expected answer"));
}

#[test]
fn lua_module_mode_is_explicit_and_rejects_world_fields() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("invalid.stationeerstest.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{
            "name": "mixed boundary",
            "focusProgram": "lua-tests",
            "execution": {"kind": "luaModule"},
            "initial": {"r0": 1}
          }]
        }"#,
    )
    .unwrap();

    let error = ScenarioTest::load(&directory.path().join("invalid.stationeerstest.json"))
        .expect_err("luaModule must not accept shared-world fields");
    assert!(error.to_string().contains("world-only fields"));
}

#[test]
fn lua_module_mode_rejects_absolute_module_roots() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("invalid.stationeerstest.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{
            "name": "non-portable root",
            "focusProgram": "lua-tests",
            "execution": {"kind": "luaModule", "moduleRoots": ["C:\\lua"]}
          }]
        }"#,
    )
    .unwrap();

    let error = ScenarioTest::load(&directory.path().join("invalid.stationeerstest.json"))
        .expect_err("luaModule roots must remain portable");
    assert!(error.to_string().contains("test-relative moduleRoots"));

    fs::write(
        directory.path().join("invalid.stationeerstest.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{
            "name": "drive-relative root",
            "focusProgram": "lua-tests",
            "execution": {"kind": "luaModule", "moduleRoots": ["C:lua"]}
          }]
        }"#,
    )
    .unwrap();
    let error = ScenarioTest::load(&directory.path().join("invalid.stationeerstest.json"))
        .expect_err("Windows drive-relative roots must remain portable");
    assert!(error.to_string().contains("test-relative moduleRoots"));
}

#[test]
fn lua_module_mode_rejects_parent_traversal_and_excessive_limits() {
    let directory = tempdir().unwrap();
    let test_path = directory.path().join("invalid.stationeerstest.json");
    fs::write(
        &test_path,
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{
            "name": "escaping root",
            "focusProgram": "lua-tests",
            "execution": {"kind": "luaModule", "moduleRoots": ["../outside"]}
          }]
        }"#,
    )
    .unwrap();
    let error = ScenarioTest::load(&test_path).expect_err("parent traversal must be rejected");
    assert!(error.to_string().contains("test-relative moduleRoots"));

    fs::write(
        &test_path,
        r#"{
          "schemaVersion": 1,
          "scenario": "world.stationeerssim.json",
          "cases": [{
            "name": "excessive memory",
            "focusProgram": "lua-tests",
            "execution": {"kind": "luaModule", "memoryLimitBytes": 67108865}
          }]
        }"#,
    )
    .unwrap();
    let error = ScenarioTest::load(&test_path).expect_err("hard ceilings must be enforced");
    assert!(error.to_string().contains("hard Lua sandbox limit"));
}

#[test]
fn lua_workspace_paths_reject_program_traversal_and_symlink_escape() {
    let workspace = tempdir().unwrap();
    fs::create_dir_all(workspace.path().join("nested")).unwrap();
    fs::write(workspace.path().join("entry.lua"), "return true\n").unwrap();
    let error = resolve_lua_workspace_path(
        workspace.path(),
        &workspace.path().join("nested"),
        std::path::Path::new("../entry.lua"),
        "Lua entry program",
    )
    .expect_err("parent traversal must be rejected even when it resolves inside the workspace");
    assert!(error.contains("without parent traversal"));

    let outside = tempdir().unwrap();
    fs::create_dir_all(outside.path().join("modules")).unwrap();
    let link = workspace.path().join("linked-modules");
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_dir(outside.path().join("modules"), &link);
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(outside.path().join("modules"), &link);
    if linked.is_ok() {
        let error = resolve_lua_workspace_path(
            workspace.path(),
            workspace.path(),
            std::path::Path::new("linked-modules"),
            "Lua module root",
        )
        .expect_err("canonical symlink escape must be rejected");
        assert!(error.contains("outside the Lua test workspace"));
    }
}
