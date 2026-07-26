use std::path::PathBuf;
use std::time::Duration;
use std::{fs, process};

use ic10_runner::{RunLimits, RunRequest, Status, run_files};

fn repository(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn scripted_fixture(test_json: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "ic10-scripted-driver-{}-{}",
        process::id(),
        test_json.len()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("program.ic10"), "s db:0 Channel0 1\nyield\n").unwrap();
    fs::write(
        root.join("scenario.ic10sim.json"),
        r#"{
          "schemaVersion": 1,
          "networks": [{"id":"data","kind":"cable","cableRole":"data"}],
          "devices": [{
            "id":"controller","prefab":"StructureCircuitHousing",
            "connections":{"0":"data"},"ic":{"program":"program.ic10"}
          }]
        }"#,
    )
    .unwrap();
    let path = root.join("driver.ic10test.json");
    fs::write(&path, test_json).unwrap();
    (root, path)
}

#[test]
fn parameterized_results_are_byte_deterministic() {
    let request = RunRequest {
        paths: vec![repository(
            "examples/scenario-tests/solar/solar.ic10test.json",
        )],
        name_filter: None,
        limits: RunLimits::default(),
    };
    let first = serde_json::to_vec(&run_files(&request)).unwrap();
    let second = serde_json::to_vec(&run_files(&request)).unwrap();
    assert_eq!(first, second);
    let summary: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(summary["passed"], 3);
    assert_eq!(summary["files"][0]["cases"][0]["durationMs"], 0);
}

#[test]
fn scripted_driver_reacts_and_schedules_deterministically() {
    let (root, path) = scripted_fixture(
        r#"{
          "schemaVersion":1,"scenario":"scenario.ic10sim.json",
          "cases":[{"name":"delayed response","maxTicks":3,
            "drivers":[{"id":"mock-machine","model":"example.machine","version":2,
              "rules":[{"name":"request","when":{"target":"network(\"data\").Channel0","equals":1},
                "actions":[{"action":"schedule","afterTicks":1,"actions":[
                  {"action":"publish","network":"data","channel":1,"value":42}
                ]}]}]}],
            "expect":[{"eventually":"network(\"data\").Channel1 == 42","withinTicks":2}]
          }]
        }"#,
    );
    let request = RunRequest {
        paths: vec![path],
        name_filter: None,
        limits: RunLimits::default(),
    };
    let first = serde_json::to_vec(&run_files(&request)).unwrap();
    let second = serde_json::to_vec(&run_files(&request)).unwrap();
    assert_eq!(first, second);
    let result: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(result["passed"], 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn scripted_driver_cycles_fail_with_model_provenance() {
    let (root, path) = scripted_fixture(
        r#"{
          "schemaVersion":1,"scenario":"scenario.ic10sim.json",
          "cases":[{"name":"cycle","maxTicks":2,
            "drivers":[{"id":"bad","model":"example.loop","version":7,"rules":[
              {"name":"down","when":{"target":"network(\"data\").Channel0","equals":1},
               "actions":[{"action":"publish","network":"data","channel":0,"value":0}]},
              {"name":"up","when":{"target":"network(\"data\").Channel0","equals":0},
               "actions":[{"action":"publish","network":"data","channel":0,"value":1}]}
            ]}]
          }]
        }"#,
    );
    let summary = run_files(&RunRequest {
        paths: vec![path],
        name_filter: None,
        limits: RunLimits::default(),
    });
    assert_eq!(summary.failed, 1);
    assert!(summary.files[0].cases[0].failures.iter().any(|failure| {
        failure.message.contains("example.loop@7") && failure.message.contains("cascade exceeded")
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failures_have_expression_tick_values_and_source_context() {
    let summary = run_files(&RunRequest {
        paths: vec![repository(
            "examples/scenario-tests/failures/assertion-failure.ic10test.json",
        )],
        name_filter: None,
        limits: RunLimits::default(),
    });
    assert_eq!(summary.failed, 1);
    let failure = &summary.files[0].cases[0].failures[0];
    assert!(failure.expression.is_some());
    assert!(failure.tick.is_some());
    assert!(failure.expected.is_some());
    assert!(failure.actual.is_some());
    assert!(failure.source.is_some());
    assert!(failure.line.is_some());
}

#[test]
fn operation_limit_stops_inside_a_world_tick() {
    let summary = run_files(&RunRequest {
        paths: vec![repository(
            "examples/scenario-tests/solar/solar.ic10test.json",
        )],
        name_filter: Some("sunrise".to_owned()),
        limits: RunLimits {
            max_ticks: 100,
            max_operations: 2,
            wall_time: Duration::from_secs(10),
        },
    });
    let case = &summary.files[0].cases[0];
    assert_eq!(case.operations, 2);
    assert_eq!(case.status, Status::Failed);
    assert!(
        case.failures
            .iter()
            .any(|failure| failure.message == "operation limit exceeded")
    );
}

#[test]
fn file_and_case_filters_are_deterministic() {
    let summary = run_files(&RunRequest {
        paths: vec![repository("examples/scenario-tests")],
        name_filter: Some("opens after".to_owned()),
        limits: RunLimits::default(),
    });
    let names: Vec<_> = summary
        .files
        .iter()
        .flat_map(|file| file.cases.iter().map(|case| case.name.as_str()))
        .collect();
    assert_eq!(names, ["opens after the chamber is depressurised"]);
}

#[test]
fn expected_compile_and_runtime_errors_are_first_class_results() {
    let root = std::env::temp_dir().join(format!("ic10-errors-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("scenario.ic10sim.json"),
        r#"{
          "schemaVersion": 1,
          "devices": [{
            "id": "controller",
            "prefab": "StructureCircuitHousing",
            "ic": {"program": "program.ic10"}
          }]
        }"#,
    )
    .unwrap();
    fs::write(root.join("program.ic10"), "notAnInstruction r0\n").unwrap();
    fs::write(
        root.join("compile.ic10test.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "scenario.ic10sim.json",
          "cases": [{
            "name": "compile error",
            "expectError": {"kind": "compile", "messageContains": "Unknown IC10 instruction"}
          }]
        }"#,
    )
    .unwrap();
    let compile = run_files(&RunRequest {
        paths: vec![root.join("compile.ic10test.json")],
        name_filter: None,
        limits: RunLimits::default(),
    });
    assert_eq!(compile.passed, 1);

    fs::write(root.join("program.ic10"), "l r0 d0 On\n").unwrap();
    fs::write(
        root.join("runtime.ic10test.json"),
        r#"{
          "schemaVersion": 1,
          "scenario": "scenario.ic10sim.json",
          "cases": [{
            "name": "runtime error",
            "maxTicks": 2,
            "expectError": {"kind": "runtime", "messageContains": "pin"}
          }]
        }"#,
    )
    .unwrap();
    let runtime = run_files(&RunRequest {
        paths: vec![root.join("runtime.ic10test.json")],
        name_filter: None,
        limits: RunLimits::default(),
    });
    assert_eq!(runtime.passed, 1);
    let _ = fs::remove_dir_all(root);
}
