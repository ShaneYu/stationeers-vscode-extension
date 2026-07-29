use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use ic10_build::{BuildOptions, BuildOutput, OptimizationLevel, build};
use ic10_data::KnowledgeBase;
use ic10_runner::{
    ExecutionSpec, RunLimits, RunRequest, RunSummary, ScenarioTest, Status,
    resolve_lua_workspace_path, run_files,
};
use ic10_sim::{LuaModuleRunner, LuaRunLimits, Program, ProgramLanguage, Scenario, Simulator};

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
        "build" => build_command(&arguments[1..]),
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

#[derive(Debug)]
struct BuildCommandOptions {
    source: PathBuf,
    output: BuildDestination,
    sidecars: bool,
    quiet: bool,
    build: BuildOptions,
}

#[derive(Debug)]
enum BuildDestination {
    Stdout,
    File(PathBuf),
}

fn build_command(arguments: &[String]) -> Result<bool, String> {
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!("{}", build_usage());
        return Ok(true);
    }
    let parsed = parse_build_options(arguments)?;
    let source_path = parsed.source.canonicalize().map_err(|error| {
        format!(
            "could not resolve source {}: {error}",
            parsed.source.display()
        )
    })?;
    let source = fs::read_to_string(&source_path)
        .map_err(|error| format!("could not read {}: {error}", source_path.display()))?;
    let knowledge = KnowledgeBase::load_embedded()
        .map_err(|error| format!("invalid embedded game data: {error}"))?;
    let mut options = parsed.build;
    options.source_path = Some(source_path.to_string_lossy().into_owned());
    let output = match build(&source, &options, &knowledge) {
        Ok(output) => output,
        Err(error) => {
            for diagnostic in error.diagnostics {
                eprintln!(
                    "{}:{}:1: error[{}]: {}",
                    source_path.display(),
                    diagnostic.source_line.unwrap_or(1),
                    diagnostic.code,
                    diagnostic.message
                );
            }
            return Ok(false);
        }
    };

    match parsed.output {
        BuildDestination::Stdout => {
            print!("{}", output.code);
        }
        BuildDestination::File(path) => {
            let output_path = absolute_output_path(&path)?;
            if output_path == source_path {
                return Err(format!(
                    "refusing to overwrite source {} with a build artefact",
                    source_path.display()
                ));
            }
            write_build_output(&output_path, &output, parsed.sidecars)?;
            if !parsed.quiet {
                println!(
                    "built {} ({} lines, {} bytes, {} lines saved, {} bytes saved)",
                    output_path.display(),
                    output.report.generated_lines,
                    output.report.generated_bytes,
                    output.report.saved_lines,
                    output.report.saved_bytes
                );
            }
        }
    }
    Ok(true)
}

fn parse_build_options(arguments: &[String]) -> Result<BuildCommandOptions, String> {
    let mut source = None;
    let mut output = None;
    let mut output_directory = None;
    let mut stdout = false;
    let mut sidecars = true;
    let mut quiet = false;
    let mut build = BuildOptions::default();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "-O" | "--optimization" => {
                index += 1;
                build.optimization = match required(arguments, index, "--optimization")? {
                    "none" => OptimizationLevel::None,
                    "readable" => OptimizationLevel::Readable,
                    "compact" => OptimizationLevel::Compact,
                    value => {
                        return Err(format!(
                            "unknown optimization level `{value}`; expected none, readable, or compact"
                        ));
                    }
                };
            }
            "--game-version" => {
                index += 1;
                build.game_version = Some(required(arguments, index, "--game-version")?.to_owned());
            }
            "--environment" => {
                index += 1;
                build.environment = Some(required(arguments, index, "--environment")?.to_owned());
            }
            "-o" | "--output" => {
                index += 1;
                let value = required(arguments, index, "--output")?;
                if value == "-" {
                    stdout = true;
                } else {
                    output = Some(PathBuf::from(value));
                }
            }
            "--output-dir" => {
                index += 1;
                output_directory = Some(PathBuf::from(required(arguments, index, "--output-dir")?));
            }
            "--stdout" => stdout = true,
            "--no-sidecars" => sidecars = false,
            "-q" | "--quiet" => quiet = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown build option `{option}`"));
            }
            path if source.is_none() => source = Some(PathBuf::from(path)),
            path => {
                return Err(format!(
                    "build accepts one source path; unexpected `{path}`"
                ));
            }
        }
        index += 1;
    }
    let source = source.ok_or_else(|| "build requires one .ic10 source path".to_owned())?;
    if !source.to_string_lossy().ends_with(".ic10") {
        return Err(format!(
            "build source must end with .ic10: {}",
            source.display()
        ));
    }
    let destination_count = usize::from(stdout)
        + usize::from(output.is_some())
        + usize::from(output_directory.is_some());
    if destination_count > 1 {
        return Err(
            "--stdout, --output, and --output-dir are mutually exclusive destinations".to_owned(),
        );
    }
    let output = if stdout {
        BuildDestination::Stdout
    } else if let Some(path) = output {
        BuildDestination::File(path)
    } else {
        let directory = output_directory.unwrap_or_else(|| {
            source
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("build")
        });
        let name = source
            .file_name()
            .ok_or_else(|| format!("source has no file name: {}", source.display()))?;
        BuildDestination::File(directory.join(name))
    };
    if matches!(output, BuildDestination::Stdout) {
        sidecars = false;
    }
    Ok(BuildCommandOptions {
        source,
        output,
        sidecars,
        quiet,
        build,
    })
}

fn absolute_output_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|error| format!("could not resolve current directory: {error}"))?
            .join(path)
    };
    if absolute.exists() {
        return absolute
            .canonicalize()
            .map_err(|error| format!("could not resolve {}: {error}", absolute.display()));
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| format!("output has no parent: {}", absolute.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("could not resolve {}: {error}", parent.display()))?;
    let name = absolute
        .file_name()
        .ok_or_else(|| format!("output has no file name: {}", absolute.display()))?;
    Ok(parent.join(name))
}

fn write_build_output(path: &Path, output: &BuildOutput, sidecars: bool) -> Result<(), String> {
    let sidecar_name = sidecars
        .then(|| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .ok_or_else(|| format!("output file name is not valid UTF-8: {}", path.display()))
        })
        .transpose()?;
    fs::write(path, &output.code)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    let Some(name) = sidecar_name else {
        return Ok(());
    };
    write_json_sidecar(
        &path.with_file_name(format!("{name}.map.json")),
        output
            .source_map_json()
            .map_err(|error| error.to_string())?,
    )?;
    write_json_sidecar(
        &path.with_file_name(format!("{name}.metadata.json")),
        output.metadata_json().map_err(|error| error.to_string())?,
    )?;
    write_json_sidecar(
        &path.with_file_name(format!("{name}.report.json")),
        output.report_json().map_err(|error| error.to_string())?,
    )
}

fn write_json_sidecar(path: &Path, mut json: String) -> Result<(), String> {
    json.push('\n');
    fs::write(path, json).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn test_command(arguments: &[String]) -> Result<bool, String> {
    let mut paths = Vec::new();
    let mut format = "human";
    let mut output = None;
    let mut filter = None;
    let mut limits = RunLimits::default();
    let mut lua_library_paths = Vec::new();
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
            "--lua-library" => {
                index += 1;
                lua_library_paths.push(resolve_library_path(required(
                    arguments,
                    index,
                    "--lua-library",
                )?)?);
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
        lua_library_paths,
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
    let mut lua_library_paths = Vec::new();
    let mut paths = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == "--lua-library" {
            index += 1;
            lua_library_paths.push(resolve_library_path(required(
                arguments,
                index,
                "--lua-library",
            )?)?);
        } else if arguments[index].starts_with('-') {
            return Err(format!("unknown check option `{}`", arguments[index]));
        } else {
            paths.push(arguments[index].clone());
        }
        index += 1;
    }
    if paths.is_empty() {
        return Err("check requires at least one path".to_owned());
    }
    for argument in &paths {
        collect_check_paths(Path::new(argument), &mut files)?;
    }
    files.sort();
    files.dedup();
    let knowledge = KnowledgeBase::load_embedded()
        .map_err(|error| format!("invalid embedded game data: {error}"))?;
    let mut valid = true;
    let mut checked = 0;
    for path in &files {
        if is_test_path(path) {
            checked += 1;
            match ScenarioTest::load(path) {
                Ok(fixture) => {
                    let fixture_base = path.parent().unwrap_or_else(|| Path::new("."));
                    let workspace_root = lua_workspace_root(fixture_base);
                    let has_lua_modules = fixture.cases.iter().any(|case| case.execution.is_some());
                    let scenario = if has_lua_modules {
                        match resolve_lua_workspace_path(
                            workspace_root,
                            fixture_base,
                            &fixture.scenario,
                            "scenario",
                        ) {
                            Ok(scenario) => scenario,
                            Err(error) => {
                                valid = false;
                                eprintln!("{}: {error}", path.display());
                                continue;
                            }
                        }
                    } else {
                        fixture_base.join(&fixture.scenario)
                    };
                    let scenario_model = match Scenario::load(&scenario) {
                        Ok(scenario) => scenario,
                        Err(error) => {
                            valid = false;
                            eprintln!("{}: {error}", scenario.display());
                            continue;
                        }
                    };
                    let mut needs_world = false;
                    let mut fixture_valid = true;
                    for case in &fixture.cases {
                        match &case.execution {
                            Some(ExecutionSpec::LuaModule {
                                module_roots,
                                memory_limit_bytes,
                                max_output_bytes,
                                max_modules,
                                max_source_bytes,
                                max_recursion_depth,
                                ..
                            }) => {
                                let program_id =
                                    case.program.as_deref().expect("validated focusProgram");
                                let Some(program) = scenario_model
                                    .programs
                                    .iter()
                                    .find(|program| program.id == program_id)
                                else {
                                    fixture_valid = false;
                                    eprintln!(
                                        "{}: unknown Lua program `{program_id}`",
                                        path.display()
                                    );
                                    continue;
                                };
                                if program.language != ProgramLanguage::Lua {
                                    fixture_valid = false;
                                    eprintln!(
                                        "{}: program `{program_id}` is not Lua",
                                        path.display()
                                    );
                                    continue;
                                }
                                for root in module_roots {
                                    if let Err(error) = resolve_lua_workspace_path(
                                        workspace_root,
                                        fixture_base,
                                        root,
                                        "Lua module root",
                                    ) {
                                        fixture_valid = false;
                                        eprintln!("{}: {error}", path.display());
                                    }
                                }
                                let scenario_base =
                                    scenario.parent().unwrap_or_else(|| Path::new("."));
                                let entry = match resolve_lua_workspace_path(
                                    fixture_base,
                                    scenario_base,
                                    &program.path,
                                    "Lua entry program",
                                ) {
                                    Ok(entry) => entry,
                                    Err(error) => {
                                        fixture_valid = false;
                                        eprintln!("{}: {error}", path.display());
                                        continue;
                                    }
                                };
                                let limits = LuaRunLimits {
                                    max_instructions: case.max_operations,
                                    wall_time: ic10_sim::LUA_MAX_WALL_TIME,
                                    memory_bytes: *memory_limit_bytes,
                                    max_output_bytes: *max_output_bytes,
                                    max_modules: *max_modules,
                                    max_source_bytes: *max_source_bytes,
                                    max_recursion_depth: *max_recursion_depth,
                                };
                                if let Err(error) =
                                    LuaModuleRunner::new().check_syntax(&entry, &limits)
                                {
                                    fixture_valid = false;
                                    eprintln!("{error}");
                                }
                            }
                            None => needs_world = true,
                        }
                    }
                    if needs_world && let Err(error) = Simulator::from_scenario_path_with_lua_library_paths(&scenario, &lua_library_paths) {
                        valid = false;
                        eprintln!("{}: {error}", scenario.display());
                    } else if fixture_valid {
                        println!("ok {}", path.display());
                    }
                    valid &= fixture_valid;
                }
                Err(error) => {
                    valid = false;
                    eprintln!("{error}");
                }
            }
        } else if is_scenario_path(path) {
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
            || is_scenario_path(&child)
            || is_test_path(&child)
        {
            files.push(child);
        }
    }
    Ok(())
}

fn is_scenario_path(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".stationeerssim.json") || name.ends_with(".ic10sim.json")
}

fn is_test_path(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".stationeerstest.json") || name.ends_with(".ic10test.json")
}

fn sim_command(arguments: &[String]) -> Result<bool, String> {
    let scenario = arguments.first().ok_or_else(|| {
        "sim requires a .stationeerssim.json or .ic10sim.json scenario".to_owned()
    })?;
    let max_ticks = arguments
        .windows(2)
        .find(|pair| pair[0] == "--max-ticks")
        .map(|pair| number(&pair[1]))
        .transpose()?
        .unwrap_or(1_000);
    let json = arguments.iter().any(|argument| argument == "--json");
    let mut lua_library_paths = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        if arguments[index] == "--lua-library" {
            index += 1;
            lua_library_paths.push(resolve_library_path(required(
                arguments,
                index,
                "--lua-library",
            )?)?);
        }
        index += 1;
    }
    let mut simulator = Simulator::from_scenario_path_with_lua_library_paths(
        Path::new(scenario),
        &lua_library_paths,
    )
    .map_err(|error| error.to_string())?;
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

fn resolve_library_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let resolved = path.canonicalize().map_err(|error| {
        format!("could not resolve Lua library directory {}: {error}", path.display())
    })?;
    if !resolved.is_dir() {
        return Err(format!(
            "Lua library path is not a directory: {}",
            resolved.display()
        ));
    }
    Ok(resolved)
}

fn lua_workspace_root(fixture_base: &Path) -> &Path {
    fixture_base
        .file_name()
        .filter(|name| name.eq_ignore_ascii_case("testing"))
        .and_then(|_| fixture_base.parent())
        .unwrap_or(fixture_base)
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
    format!(
        "Usage:\n  ic10 check [--lua-library DIR]... <paths...>\n  {}\n  ic10 test [--format human|json|junit] [--output FILE] [--filter NAME] [--max-ticks N] [--max-operations N] [--wall-time-ms N] [--lua-library DIR]... <paths...>\n  ic10 sim <scenario.stationeerssim.json|scenario.ic10sim.json> [--max-ticks N] [--lua-library DIR]... [--json]\n  ic10 compatibility [--json]",
        build_usage()
    )
}

fn build_usage() -> String {
    "ic10 build <source.ic10> [-O none|readable|compact] [--game-version VERSION] [--environment NAME] [--stdout | --output FILE | --output-dir DIR] [--no-sidecars] [--quiet]".to_owned()
}
