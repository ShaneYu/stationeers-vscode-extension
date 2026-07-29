use std::fs;

use ic10_sim::{LuaRuntimeBoundary, ProgramLanguage, Scenario, Simulator};
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
fn lua_only_scenario_loads_without_ic10_parsing() {
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
    let simulator =
        Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json")).unwrap();
    assert_eq!(simulator.lua_programs().len(), 1);
}

fn assert_mixed_scenario_loads(lua_first: bool) {
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
    fs::write(
        directory.path().join("future.lua"),
        "function tick(dt) end\n",
    )
    .unwrap();
    let simulator =
        Simulator::from_scenario_path(&directory.path().join("world.stationeerssim.json")).unwrap();
    assert_eq!(simulator.lua_programs().len(), 1);
}

#[test]
fn mixed_scenario_with_lua_first_loads() {
    assert_mixed_scenario_loads(true);
}

#[test]
fn mixed_scenario_with_lua_last_loads() {
    assert_mixed_scenario_loads(false);
}

#[test]
fn attached_lua_reports_unrelated_structural_errors() {
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
    assert!(error.to_string().contains("unknown program"));
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
