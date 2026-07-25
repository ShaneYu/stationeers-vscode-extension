use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use ic10_data::KnowledgeBase;
use ic10_runner::{RunLimits, RunRequest, RunSummary, ScenarioTest, Status, run_files};
use ic10_sim::{Program, Simulator};

fn main() -> ExitCode {
    match execute(env::args().skip(1).collect()) {
        Ok(success) => {
            if success {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            eprintln!("ic10: {error}");
            ExitCode::from(2)
        }
    }
}

fn execute(arguments: Vec<String>) -> Result<bool, String> {
    let Some(command) = arguments.first().map(String::as_str) else {
        return Err(usage());
    };
    match command {
        "test" => test_command(&arguments[1..]),
        "check" => check_command(&arguments[1..]),
        "sim" => sim_command(&arguments[1..]),
        "compatibility" => compatibility_command(&arguments[1..]),
        "-h" | "--help" | "help" => {
            println!("{}", usage());
            Ok(true)
        }
        "-V" | "--version" => {
            println!("ic10 {}", env!("CARGO_PKG_VERSION"));
            Ok(true)
        }
        command => Err(format!("unknown command `{command}`\n\n{}", usage())),
    }
}

fn test_command(arguments: &[String]) -> Result<bool, String> {
    let mut paths = Vec::new();
    let mut format = "human";
    let mut output = None;
    let mut filter = None;
    let mut limits = RunLimits::default();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--format" => {
                index += 1;
                format = required(arguments, index, "--format")?;
            }
            "--output" => {
                index += 1;
                output = Some(PathBuf::from(required(arguments, index, "--output")?));
            }
            "--filter" => {
                index += 1;
                filter = Some(required(arguments, index, "--filter")?.to_owned());
            }
            "--max-ticks" => {
                index += 1;
                limits.max_ticks = number(required(arguments, index, "--max-ticks")?)?;
            }
            "--max-operations" => {
                index += 1;
                limits.max_operations = number(required(arguments, index, "--max-operations")?)?;
            }
            "--wall-time-ms" => {
                index += 1;
                limits.wall_time =
                    Duration::from_millis(number(required(arguments, index, "--wall-time-ms")?)?);
            }
            option if option.starts_with('-') => {
                return Err(format!("unknown test option `{option}`"));
            }
            path => paths.push(PathBuf::from(path)),
        }
        index += 1;
    }
    if paths.is_empty() {
        paths.push(PathBuf::from("."));
    }
    let summary = run_files(&RunRequest {
        paths,
        name_filter: filter,
        limits,
    });
    let rendered = match format {
        "human" => human(&summary),
        "json" => serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())? + "\n",
        "junit" => junit(&summary),
        other => {
            return Err(format!(
                "unknown output format `{other}`; expected human, json, or junit"
            ));
        }
    };
    if let Some(path) = output {
        std::fs::write(&path, rendered)
            .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    } else {
        print!("{rendered}");
    }
    Ok(summary.failed == 0 && summary.invalid == 0)
}

fn check_command(arguments: &[String]) -> Result<bool, String> {
    if arguments.is_empty() {
        return Err("check requires at least one path".to_owned());
    }
    let mut files = Vec::new();
    for argument in arguments {
        collect_check_paths(Path::new(argument), &mut files)?;
    }
    files.sort();
    files.dedup();
    let knowledge = KnowledgeBase::load_embedded()
        .map_err(|error| format!("invalid embedded game data: {error}"))?;
    let mut valid = true;
    let mut checked = 0;
    for path in &files {
        if path.to_string_lossy().ends_with(".ic10test.json") {
            checked += 1;
            match ScenarioTest::load(path) {
                Ok(fixture) => {
                    let scenario = path
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(fixture.scenario);
                    if let Err(error) = Simulator::from_scenario_path(&scenario) {
                        valid = false;
                        eprintln!("{}: {error}", scenario.display());
                    } else {
                        println!("ok {}", path.display());
                    }
                }
                Err(error) => {
                    valid = false;
                    eprintln!("{error}");
                }
            }
        } else if path.to_string_lossy().ends_with(".ic10sim.json") {
            checked += 1;
            match Simulator::from_scenario_path(path) {
                Ok(_) => println!("ok {}", path.display()),
                Err(error) => {
                    valid = false;
                    eprintln!("{}: {error}", path.display());
                }
            }
        } else if path.to_string_lossy().ends_with(".ic10") {
            checked += 1;
            match std::fs::read_to_string(path)
                .map_err(|error| error.to_string())
                .and_then(|source| {
                    Program::compile(path.to_path_buf(), source, &knowledge)
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                }) {
                Ok(()) => println!("ok {}", path.display()),
                Err(error) => {
                    valid = false;
                    eprintln!("{}: {error}", path.display());
                }
            }
        } else {
            valid = false;
            eprintln!("unsupported or missing path: {}", path.display());
        }
    }
    if checked == 0 {
        return Err("no scenario or test fixtures found".to_owned());
    }
    Ok(valid)
}

fn collect_check_paths(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    let mut entries = std::fs::read_dir(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let child = entry.path();
        if child.is_dir() {
            if !matches!(
                child.file_name().and_then(|name| name.to_str()),
                Some(".git" | "node_modules" | "target")
            ) {
                collect_check_paths(&child, files)?;
            }
        } else if child.to_string_lossy().ends_with(".ic10")
            || child.to_string_lossy().ends_with(".ic10sim.json")
            || child.to_string_lossy().ends_with(".ic10test.json")
        {
            files.push(child);
        }
    }
    Ok(())
}

fn sim_command(arguments: &[String]) -> Result<bool, String> {
    let scenario = arguments
        .first()
        .ok_or_else(|| "sim requires a .ic10sim.json scenario".to_owned())?;
    let max_ticks = arguments
        .windows(2)
        .find(|pair| pair[0] == "--max-ticks")
        .map(|pair| number(&pair[1]))
        .transpose()?
        .unwrap_or(1_000);
    let json = arguments.iter().any(|argument| argument == "--json");
    let mut simulator =
        Simulator::from_scenario_path(Path::new(scenario)).map_err(|error| error.to_string())?;
    let mut operations = 0_usize;
    while !simulator.is_finished() && simulator.tick < max_ticks {
        operations += simulator
            .step_world_tick()
            .map_err(|error| format!("tick {}: {error}", simulator.tick))?
            .len();
    }
    if json {
        let cpus: Vec<_> = simulator.cpus.iter().map(|cpu| serde_json::json!({
            "id": cpu.id, "state": format!("{:?}", cpu.state), "line": cpu.current_line().map(|line| line + 1), "error": cpu.error
        })).collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "schemaVersion": 1, "scenario": scenario, "tick": simulator.tick, "operations": operations, "finished": simulator.is_finished(), "cpus": cpus,
            "compatibilityWarnings": simulator.compatibility_warnings
        })).map_err(|error| error.to_string())?);
    } else {
        println!(
            "{}: {} ticks, {} operations, {}",
            scenario,
            simulator.tick,
            operations,
            if simulator.is_finished() {
                "finished"
            } else {
                "bounded"
            }
        );
    }
    Ok(simulator.is_finished())
}

fn compatibility_command(arguments: &[String]) -> Result<bool, String> {
    let json = include_str!("../../../data/generated/conformance.json");
    if arguments.iter().any(|argument| argument == "--json") {
        print!("{json}");
        if !json.ends_with('\n') {
            println!();
        }
    } else {
        let report: serde_json::Value =
            serde_json::from_str(json).map_err(|error| error.to_string())?;
        let instructions = report["instructions"]
            .as_object()
            .ok_or_else(|| "invalid embedded compatibility report".to_owned())?;
        let supported = instructions
            .values()
            .filter(|entry| entry["status"] == "supported")
            .count();
        let partial = instructions
            .values()
            .filter(|entry| entry["status"] == "partial")
            .count();
        let unsupported = instructions.len() - supported - partial;
        println!(
            "Stationeers {}",
            report["gameVersion"].as_str().unwrap_or("unknown")
        );
        println!("{supported} supported, {partial} partial, {unsupported} unsupported");
    }
    Ok(true)
}

fn human(summary: &RunSummary) -> String {
    let mut output = String::new();
    for file in &summary.files {
        if let Some(error) = &file.error {
            output.push_str(&format!("INVALID {}: {error}\n", file.path.display()));
            continue;
        }
        output.push_str(&format!("{}\n", file.path.display()));
        for case in &file.cases {
            let mark = match case.status {
                Status::Passed => "PASS",
                Status::Failed => "FAIL",
                Status::Invalid => "INVALID",
            };
            output.push_str(&format!(
                "  {mark} {} (tick {}, {} ops)\n",
                case.name, case.ticks, case.operations
            ));
            for failure in &case.failures {
                output.push_str(&format!(
                    "    tick {}: {}",
                    failure
                        .tick
                        .map(|tick| tick.to_string())
                        .unwrap_or_else(|| "-".to_owned()),
                    failure.message
                ));
                if let Some(expression) = &failure.expression {
                    output.push_str(&format!(": `{expression}`"));
                }
                if failure.expected.is_some() || failure.actual.is_some() {
                    output.push_str(&format!(
                        " (expected {}, actual {})",
                        failure.expected.as_deref().unwrap_or("-"),
                        failure.actual.as_deref().unwrap_or("-")
                    ));
                }
                if let Some(source) = &failure.source {
                    output.push_str(&format!(
                        " at {}:{}",
                        source.display(),
                        failure.line.unwrap_or(1)
                    ));
                }
                output.push('\n');
            }
        }
    }
    output.push_str(&format!(
        "\n{} passed, {} failed, {} invalid\n",
        summary.passed, summary.failed, summary.invalid
    ));
    output
}

fn junit(summary: &RunSummary) -> String {
    let tests = summary
        .files
        .iter()
        .map(|file| file.cases.len().max(usize::from(file.error.is_some())))
        .sum::<usize>();
    let mut output = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuites tests=\"{tests}\" failures=\"{}\" errors=\"{}\" time=\"0\">\n",
        summary.failed, summary.invalid
    );
    for file in &summary.files {
        output.push_str(&format!(
            "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{}\" errors=\"{}\" time=\"0\">\n",
            xml(&file.path.display().to_string()),
            file.cases.len().max(usize::from(file.error.is_some())),
            file.cases
                .iter()
                .filter(|case| case.status == Status::Failed)
                .count(),
            usize::from(file.error.is_some())
                + file
                    .cases
                    .iter()
                    .filter(|case| case.status == Status::Invalid)
                    .count()
        ));
        if let Some(error) = &file.error {
            output.push_str(&format!(
                "    <testcase name=\"fixture\" time=\"0\"><error message=\"{}\"/></testcase>\n",
                xml(error)
            ));
        }
        for case in &file.cases {
            output.push_str(&format!(
                "    <testcase name=\"{}\" classname=\"{}\" time=\"0\">",
                xml(&case.name),
                xml(&file.path.display().to_string())
            ));
            if case.status != Status::Passed {
                let body = case
                    .failures
                    .iter()
                    .map(|failure| failure.message.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                let tag = if case.status == Status::Invalid {
                    "error"
                } else {
                    "failure"
                };
                output.push_str(&format!(
                    "<{tag} message=\"{}\">{}</{tag}>",
                    xml(case
                        .failures
                        .first()
                        .map(|failure| failure.message.as_str())
                        .unwrap_or("failed")),
                    xml(&body)
                ));
            }
            output.push_str("</testcase>\n");
        }
        output.push_str("  </testsuite>\n");
    }
    output.push_str("</testsuites>\n");
    output
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn required<'a>(arguments: &'a [String], index: usize, option: &str) -> Result<&'a str, String> {
    arguments
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("{option} requires a value"))
}
fn number(value: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("`{value}` is not a non-negative integer"))
}
fn usage() -> String {
    "Usage:\n  ic10 check <paths...>\n  ic10 test [--format human|json|junit] [--output FILE] [--filter NAME] [--max-ticks N] [--max-operations N] [--wall-time-ms N] <paths...>\n  ic10 sim <scenario.ic10sim.json> [--max-ticks N] [--json]\n  ic10 compatibility [--json]".to_owned()
}
