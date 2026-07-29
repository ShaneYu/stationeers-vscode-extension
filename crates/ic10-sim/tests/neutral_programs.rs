use std::fs;

use ic10_sim::{LuaRuntimeBoundary, ProgramLanguage, Scenario, Simulator, SimulatorError};
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
    fs::write(
        directory.path().join("main.lua"),
        include_str!("fixtures/lua-unsupported.lua"),
    )
    .unwrap();
    let error = Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json"))
        .unwrap_err();
    assert!(
        matches!(error, SimulatorError::Message(message) if message.contains("lua-runtime-unavailable") && message.contains("controller") && message.contains("no source was executed"))
    );
}

fn assert_mixed_scenario_fails_closed(lua_first: bool) {
    let directory = tempdir().unwrap();
    let devices = if lua_first {
        r#"
            {"id": "lua-housing", "prefab": "StructureCircuitHousing", "program": "future"},
            {"id": "ic-housing", "prefab": "StructureCircuitHousing", "program": "controller"}"#
    } else {
        r#"
            {"id": "ic-housing", "prefab": "StructureCircuitHousing", "program": "controller"},
            {"id": "lua-housing", "prefab": "StructureCircuitHousing", "program": "future"}"#
    };
    write_scenario(
        directory.path(),
        &format!(
            r#"{{
          "schemaVersion": 1,
          "programs": [
            {{"id": "controller", "path": "main.ic10", "language": "ic10"}},
            {{"id": "future", "path": "future.lua", "language": "lua"}}
          ],
          "devices": [{devices}
          ]
        }}"#
        ),
    );
    // A missing IC10 source proves that every slot is validated before any
    // program is read or compiled, including when Lua is declared last.
    fs::remove_file(directory.path().join("main.ic10")).unwrap();

    let error = Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json"))
        .unwrap_err();
    assert!(matches!(error, SimulatorError::Message(message) if
            message.contains("lua-runtime-unavailable")
                && message.contains("future")
                && message.contains("no source was executed")));
}

#[test]
fn mixed_scenario_with_lua_first_fails_before_reading_source() {
    assert_mixed_scenario_fails_closed(true);
}

#[test]
fn mixed_scenario_with_lua_last_fails_before_reading_source() {
    assert_mixed_scenario_fails_closed(false);
}

#[test]
fn attached_lua_is_reported_before_unrelated_structural_errors() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("world.stationeerssim.json"),
        r#"{
          "schemaVersion": 1,
          "programs": [
            {"id": "future", "path": "future.lua", "language": "lua"}
          ],
          "devices": [
            {"id": "broken", "prefab": "StructureCircuitHousing", "program": "missing"},
            {"id": "lua-housing", "prefab": "StructureCircuitHousing", "program": "future"}
          ]
        }"#,
    )
    .unwrap();

    let error = Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json"))
        .unwrap_err();
    assert!(matches!(error, SimulatorError::Message(message) if
            message.contains("lua-runtime-unavailable")
                && message.contains("future")
                && message.contains("no source was executed")));
}

#[test]
fn lua_boundary_diagnostic_is_stable_for_fixture_source() {
    let diagnostic = LuaRuntimeBoundary::new().unsupported(
        "fixture",
        std::path::Path::new("tests/fixtures/lua-unsupported.lua"),
    );
    assert_eq!(diagnostic.code, "lua-runtime-unavailable");
    assert!(diagnostic.message.contains("Lua 5.2"));
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
