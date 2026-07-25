use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name)
}

#[test]
fn json_and_junit_are_machine_consumable() {
    let executable = env!("CARGO_BIN_EXE_ic10");
    let fixture = fixture("examples/scenario-tests/solar/solar.ic10test.json");
    let json = Command::new(executable)
        .args(["test", "--format", "json"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(json.status.success());
    let parsed: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(parsed["passed"], 3);

    let junit = Command::new(executable)
        .args(["test", "--format", "junit"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(junit.status.success());
    let xml = String::from_utf8(junit.stdout).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\""));
    assert!(xml.contains("<testsuite"));
    assert!(xml.contains("failures=\"0\""));
}

#[test]
fn failures_and_invalid_fixtures_have_nonzero_status() {
    let executable = env!("CARGO_BIN_EXE_ic10");
    let failure = Command::new(executable)
        .arg("test")
        .arg(fixture(
            "examples/scenario-tests/failures/assertion-failure.ic10test.json",
        ))
        .status()
        .unwrap();
    assert_eq!(failure.code(), Some(1));

    let invalid = Command::new(executable)
        .args(["test", "does-not-exist.ic10test.json"])
        .status()
        .unwrap();
    assert_ne!(invalid.code(), Some(0));
}

#[test]
fn compatibility_json_is_the_versioned_report() {
    let output = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .args(["compatibility", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], 1);
    assert!(report["instructions"].is_object());
}
