use std::fs;

use ic10_sim::{ProgramLanguage, Scenario, Simulator, SimulatorError};
use tempfile::tempdir;

fn write_scenario(directory: &std::path::Path, body: &str) {
    fs::write(directory.join("main.ic10"), "yield\n").unwrap();
    fs::write(directory.join("world.stationeerssim.json"), body).unwrap();
}

#[test]
fn canonical_ic10_program_round_trips_without_losing_identity() {
    let directory = tempdir().unwrap();
    write_scenario(
        directory.path(),
        r#"{
          "schemaVersion": 1,
          "programs": [{"id": "controller", "path": "main.ic10", "language": "ic10"}],
          "devices": [{"id": "housing", "prefab": "StructureCircuitHousing", "program": "controller"}]
        }"#,
    );
    let scenario = Scenario::load(&directory.path().join("world.stationeerssim.json")).unwrap();
    assert_eq!(scenario.programs[0].language, ProgramLanguage::Ic10);
    let serialized = serde_json::to_string(&scenario).unwrap();
    let round_trip: Scenario = serde_json::from_str(&serialized).unwrap();
    assert_eq!(round_trip.programs, scenario.programs);
    assert_eq!(round_trip.devices[0].program, Some("controller".to_owned()));
    Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json")).unwrap();
}

#[test]
fn lua_only_scenario_reports_unsupported_runtime_without_ic10_parsing() {
    let directory = tempdir().unwrap();
    write_scenario(
        directory.path(),
        r#"{
          "schemaVersion": 1,
          "programs": [{"id": "controller", "path": "main.lua", "language": "lua"}],
          "devices": [{"id": "housing", "prefab": "StructureCircuitHousing", "program": "controller"}]
        }"#,
    );
    fs::write(directory.path().join("main.lua"), "notAnIc10Instruction\n").unwrap();
    let error = Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json"))
        .unwrap_err();
    assert!(
        matches!(error, SimulatorError::Message(message) if message == "unsupported runtime: Lua program `controller` cannot be executed before P3.09")
    );
}

#[test]
fn mixed_scenario_keeps_ic10_executable_and_does_not_parse_lua() {
    let directory = tempdir().unwrap();
    write_scenario(
        directory.path(),
        r#"{
          "schemaVersion": 1,
          "programs": [
            {"id": "controller", "path": "main.ic10", "language": "ic10"},
            {"id": "future", "path": "future.lua", "language": "lua"}
          ],
          "devices": [
            {"id": "ic-housing", "prefab": "StructureCircuitHousing", "program": "controller"},
            {"id": "lua-housing", "prefab": "StructureCircuitHousing", "program": "future"}
          ]
        }"#,
    );
    fs::write(
        directory.path().join("future.lua"),
        "notAnIc10Instruction\n",
    )
    .unwrap();
    let simulator =
        Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json")).unwrap();
    assert_eq!(simulator.cpus.len(), 1);
    assert_eq!(simulator.cpus[0].program_id, "controller");
}

#[test]
fn legacy_device_program_remains_ic10() {
    let directory = tempdir().unwrap();
    write_scenario(
        directory.path(),
        r#"{
          "schemaVersion": 1,
          "devices": [{"id": "housing", "prefab": "StructureCircuitHousing", "ic": {"program": "main.ic10"}}]
        }"#,
    );
    let simulator =
        Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json")).unwrap();
    assert_eq!(simulator.cpus[0].program_id, "housing");
}
