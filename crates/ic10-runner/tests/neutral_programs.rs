use std::fs;

use ic10_runner::{RunLimits, RunRequest, ScenarioTest, Status, run_files};
use tempfile::tempdir;

#[test]
fn neutral_lua_selector_is_an_explicit_unsupported_runtime() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("world.stationeerssim.json"),
        r#"{"schemaVersion":1,"programs":[{"id":"lua-main","path":"main.lua","language":"lua"}],"devices":[{"id":"housing","prefab":"StructureCircuitHousing","program":"lua-main"}]}"#,
    ).unwrap();
    fs::write(directory.path().join("main.lua"), "not IC10").unwrap();
    fs::write(
        directory.path().join("world.stationeerstest.json"),
        r#"{"schemaVersion":1,"scenario":"world.stationeerssim.json","cases":[{"name":"lua","program":"lua-main","expectError":{"kind":"runtime","messageContains":"unsupported runtime"}}]}"#,
    ).unwrap();
    let result = run_files(&RunRequest {
        paths: vec![directory.path().join("world.stationeerstest.json")],
        name_filter: None,
        limits: RunLimits::default(),
    });
    assert_eq!(result.passed, 1);
    assert_eq!(result.files[0].cases[0].status, Status::Passed);
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
