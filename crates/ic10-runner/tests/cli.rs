use std::path::PathBuf;
use std::process::Command;

use ic10_build::{BuildOptions, OptimizationLevel, build};
use ic10_data::KnowledgeBase;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(name)
}

#[test]
fn json_and_junit_are_machine_consumable() {
    let executable = env!("CARGO_BIN_EXE_ic10");
    let fixture = fixture("examples/scenario-tests/solar/solar.stationeerstest.json");
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
fn lua_module_tests_produce_structured_output_and_ci_status() {
    let temporary = tempfile::tempdir().unwrap();
    let scenario = temporary.path().join("lua.stationeerssim.json");
    let entry = temporary.path().join("module-test.lua");
    let fixture = temporary.path().join("lua.stationeerstest.json");
    std::fs::write(
        &scenario,
        r#"{"schemaVersion":1,"programs":[{"id":"module-tests","path":"module-test.lua","language":"lua"}],"devices":[]}"#,
    )
    .unwrap();
    std::fs::write(
        &fixture,
        r#"{"schemaVersion":1,"scenario":"lua.stationeerssim.json","cases":[{"name":"pure module","focusProgram":"module-tests","execution":{"kind":"luaModule"}}]}"#,
    )
    .unwrap();
    std::fs::write(&entry, "print('module ok')\nassert(6 * 7 == 42)\n").unwrap();

    let check = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .arg("check")
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );

    let passing = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .args(["test", "--format", "json"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert!(
        passing.status.success(),
        "{}",
        String::from_utf8_lossy(&passing.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&passing.stdout).unwrap();
    assert_eq!(summary["passed"], 1);
    assert_eq!(
        summary["files"][0]["cases"][0]["capturedOutput"][0],
        "module ok"
    );
    assert_eq!(summary["files"][0]["cases"][0]["ticks"], 0);

    std::fs::write(&entry, "local actual = 41\nassert(actual == 42)\n").unwrap();
    let failing = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .args(["test", "--format", "json"])
        .arg(&fixture)
        .output()
        .unwrap();
    assert_eq!(failing.status.code(), Some(1));
    let summary: serde_json::Value = serde_json::from_slice(&failing.stdout).unwrap();
    assert_eq!(summary["failed"], 1);
    assert_eq!(summary["files"][0]["cases"][0]["failures"][0]["line"], 2);
    assert!(
        summary["files"][0]["cases"][0]["failures"][0]["source"]
            .as_str()
            .is_some_and(|source| source.ends_with("module-test.lua"))
    );
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

#[test]
fn build_stdout_is_identical_to_the_library_for_every_level() {
    let temporary = tempfile::tempdir().unwrap();
    let source_path = temporary.path().join("parity.ic10");
    let source =
        "define Amount 3\nalias _output r0\nstart:\nmove _output Amount # comment\nj start\n";
    std::fs::write(&source_path, source).unwrap();
    let canonical = source_path.canonicalize().unwrap();
    let knowledge = KnowledgeBase::load_embedded().unwrap();

    for (name, optimization) in [
        ("none", OptimizationLevel::None),
        ("readable", OptimizationLevel::Readable),
        ("compact", OptimizationLevel::Compact),
    ] {
        let options = BuildOptions {
            optimization,
            source_path: Some(canonical.to_string_lossy().into_owned()),
            ..BuildOptions::default()
        };
        let expected = build(source, &options, &knowledge).unwrap();
        let actual = Command::new(env!("CARGO_BIN_EXE_ic10"))
            .current_dir(temporary.path())
            .args(["build", "--stdout", "--optimization", name])
            .arg(&source_path)
            .output()
            .unwrap();
        assert!(
            actual.status.success(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, expected.code.as_bytes(), "{name}");
        assert!(actual.stderr.is_empty());
        assert!(!temporary.path().join(".ic10").exists());
    }
}

#[test]
fn build_writes_code_and_sidecars_identical_to_the_library() {
    let temporary = tempfile::tempdir().unwrap();
    let source_directory = temporary.path().join("programs/multi-ic");
    std::fs::create_dir_all(&source_directory).unwrap();
    let source_path = source_directory.join("program.ic10");
    let source = "define Amount 3\nmove r0 Amount # comment\n";
    std::fs::write(&source_path, source).unwrap();
    let canonical = source_path.canonicalize().unwrap();
    let knowledge = KnowledgeBase::load_embedded().unwrap();
    let options = BuildOptions {
        optimization: OptimizationLevel::Compact,
        game_version: Some(knowledge.language.game_version.clone()),
        source_path: Some(canonical.to_string_lossy().into_owned()),
        environment: Some("ci".to_owned()),
    };
    let expected = build(source, &options, &knowledge).unwrap();

    let actual = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .current_dir(temporary.path())
        .args([
            "build",
            "--optimization",
            "compact",
            "--game-version",
            &knowledge.language.game_version,
            "--environment",
            "ci",
            "--quiet",
        ])
        .arg(&source_path)
        .output()
        .unwrap();
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(actual.stdout.is_empty());
    let artefact = source_directory.join("build/program.ic10");
    assert_eq!(std::fs::read_to_string(&artefact).unwrap(), expected.code);
    assert_json_eq(
        &artefact.with_file_name("program.ic10.map.json"),
        serde_json::to_value(&expected.source_map).unwrap(),
    );
    assert_json_eq(
        &artefact.with_file_name("program.ic10.metadata.json"),
        serde_json::to_value(&expected.metadata).unwrap(),
    );
    assert_json_eq(
        &artefact.with_file_name("program.ic10.report.json"),
        serde_json::to_value(&expected.report).unwrap(),
    );
    assert_eq!(std::fs::read_to_string(&source_path).unwrap(), source);

    let code_only = temporary.path().join("code-only.ic10");
    let no_sidecars = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .args(["build", "--no-sidecars", "--output"])
        .arg(&code_only)
        .arg(&source_path)
        .output()
        .unwrap();
    assert!(no_sidecars.status.success());
    assert!(code_only.exists());
    assert!(!temporary.path().join("code-only.ic10.map.json").exists());
}

#[test]
fn build_refuses_source_overwrite_and_prints_matchable_diagnostics() {
    let temporary = tempfile::tempdir().unwrap();
    let source_path = temporary.path().join("unsafe.ic10");
    let source = "alias offset r0\n# removed\njr offset\n";
    std::fs::write(&source_path, source).unwrap();

    let unsafe_build = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .args(["build", "--stdout"])
        .arg(&source_path)
        .output()
        .unwrap();
    assert_eq!(unsafe_build.status.code(), Some(1));
    let diagnostic = String::from_utf8(unsafe_build.stderr).unwrap();
    assert!(diagnostic.contains(":3:1: error[unsafe-relative-branch]:"));

    let overwrite = Command::new(env!("CARGO_BIN_EXE_ic10"))
        .args(["build", "--optimization", "none", "--output"])
        .arg(&source_path)
        .arg(&source_path)
        .output()
        .unwrap();
    assert_eq!(overwrite.status.code(), Some(2));
    assert!(
        String::from_utf8(overwrite.stderr)
            .unwrap()
            .contains("refusing to overwrite source")
    );
    assert_eq!(std::fs::read_to_string(source_path).unwrap(), source);
}

fn assert_json_eq(path: &std::path::Path, expected: serde_json::Value) {
    let actual: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(actual, expected, "{}", path.display());
}
