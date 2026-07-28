use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ic10_sim::{ProgramLanguage, Scalar, Scenario, Simulator, SimulatorError};
use serde::{Deserialize, Serialize};

use crate::evaluator::{Value, evaluate, format_number, set_value};
use crate::schema::{Assertion, ErrorKind, ScenarioTest, TestCase};
use crate::script_driver::ScriptDrivers;

#[derive(Clone, Debug)]
pub struct RunRequest {
    pub paths: Vec<PathBuf>,
    pub name_filter: Option<String>,
    pub limits: RunLimits,
}

#[derive(Clone, Debug)]
pub struct RunLimits {
    pub max_ticks: u64,
    pub max_operations: u64,
    pub wall_time: Duration,
}

impl Default for RunLimits {
    fn default() -> Self {
        Self {
            max_ticks: 10_000,
            max_operations: 10_000_000,
            wall_time: Duration::from_secs(30),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub schema_version: u32,
    pub files: Vec<FileResult>,
    pub passed: usize,
    pub failed: usize,
    pub invalid: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileResult {
    pub path: PathBuf,
    pub scenario: Option<PathBuf>,
    pub cases: Vec<CaseResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaseResult {
    pub name: String,
    pub status: Status,
    pub seed: u64,
    pub ticks: u64,
    pub operations: u64,
    pub duration_ms: u64,
    pub failures: Vec<Failure>,
    pub compatibility_warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Passed,
    Failed,
    Invalid,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Failure {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    pub possibly_unsupported: bool,
}

pub fn discover(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut fixtures = BTreeSet::new();
    for path in paths {
        discover_path(path, &mut fixtures)?;
    }
    // A migration workspace may intentionally contain both spellings of the
    // same test. Prefer the canonical spelling when both are present so a
    // directory run does not execute the migrated fixture twice. A legacy
    // file remains discoverable and runnable when it is the only spelling.
    let mut canonical = BTreeSet::new();
    for path in &fixtures {
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".stationeerstest.json"))
        {
            canonical.insert(logical_test_key(path));
        }
    }
    Ok(fixtures
        .into_iter()
        .filter(|path| {
            !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".ic10test.json"))
                || !canonical.contains(&logical_test_key(path))
        })
        .collect())
}

fn logical_test_key(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let stem = file_name
        .strip_suffix(".stationeerstest.json")
        .or_else(|| file_name.strip_suffix(".ic10test.json"))
        .unwrap_or(file_name);
    path.with_file_name(stem)
}

fn discover_path(path: &Path, output: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    if path.is_file() {
        if is_test_path(path) {
            output.insert(path.to_path_buf());
        }
        return Ok(());
    }
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    let mut entries = fs::read_dir(path)
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
                discover_path(&child, output)?;
            }
        } else if is_test_path(&child) {
            output.insert(child);
        }
    }
    Ok(())
}

fn is_test_path(path: &Path) -> bool {
    let name = path.to_string_lossy();
    name.ends_with(".ic10test.json") || name.ends_with(".stationeerstest.json")
}

pub fn run_files(request: &RunRequest) -> RunSummary {
    let discovered = discover(&request.paths);
    let mut files = Vec::new();
    match discovered {
        Err(error) => files.push(FileResult {
            path: PathBuf::new(),
            scenario: None,
            cases: vec![],
            error: Some(error),
        }),
        Ok(paths) if paths.is_empty() => files.push(FileResult {
            path: PathBuf::new(),
            scenario: None,
            cases: vec![],
            error: Some("no *.ic10test.json files found".to_owned()),
        }),
        Ok(paths) => {
            for path in paths {
                files.push(run_file(&path, request));
            }
        }
    }
    let passed = files
        .iter()
        .flat_map(|file| &file.cases)
        .filter(|case| case.status == Status::Passed)
        .count();
    let failed = files
        .iter()
        .flat_map(|file| &file.cases)
        .filter(|case| case.status == Status::Failed)
        .count();
    let invalid = files.iter().filter(|file| file.error.is_some()).count()
        + files
            .iter()
            .flat_map(|file| &file.cases)
            .filter(|case| case.status == Status::Invalid)
            .count();
    RunSummary {
        schema_version: 1,
        files,
        passed,
        failed,
        invalid,
    }
}

pub fn load_expanded_case(
    path: &Path,
    requested_name: &str,
) -> Result<(PathBuf, u64, TestCase), String> {
    let fixture = ScenarioTest::load(path).map_err(|error| error.to_string())?;
    let scenario = path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(&fixture.scenario);
    for (case_index, case) in fixture.cases.iter().enumerate() {
        for (parameter_index, (name, expanded)) in expand_case(case).into_iter().enumerate() {
            if name == requested_name {
                let seed = fixture
                    .seed
                    .wrapping_add(case_index as u64)
                    .wrapping_mul(0x9E37_79B9)
                    .wrapping_add(parameter_index as u64);
                return Ok((scenario, seed, expanded));
            }
        }
    }
    Err(format!(
        "{} does not contain test case `{requested_name}`",
        path.display()
    ))
}

fn run_file(path: &Path, request: &RunRequest) -> FileResult {
    let fixture = match ScenarioTest::load(path) {
        Ok(fixture) => fixture,
        Err(error) => {
            return FileResult {
                path: path.to_path_buf(),
                scenario: None,
                cases: vec![],
                error: Some(error.to_string()),
            };
        }
    };
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let scenario = base.join(&fixture.scenario);
    let mut cases = Vec::new();
    for (case_index, case) in fixture.cases.iter().enumerate() {
        let expanded = expand_case(case);
        for (parameter_index, (name, case)) in expanded.into_iter().enumerate() {
            if request
                .name_filter
                .as_ref()
                .is_some_and(|filter| !name.contains(filter))
            {
                continue;
            }
            let seed = fixture
                .seed
                .wrapping_add(case_index as u64)
                .wrapping_mul(0x9E37_79B9)
                .wrapping_add(parameter_index as u64);
            cases.push(run_case(&name, &case, &scenario, seed, &request.limits));
        }
    }
    FileResult {
        path: path.to_path_buf(),
        scenario: Some(scenario),
        cases,
        error: None,
    }
}

fn expand_case(case: &TestCase) -> Vec<(String, TestCase)> {
    if case.parameters.is_empty() {
        return vec![(case.name.clone(), case.clone())];
    }
    case.parameters
        .iter()
        .enumerate()
        .filter_map(|(index, parameters)| {
            let mut value = serde_json::to_value(case).ok()?;
            if let Some(object) = value.as_object_mut() {
                object.insert("parameters".to_owned(), serde_json::json!([]));
            }
            substitute(&mut value, &parameters.values);
            let expanded: TestCase = serde_json::from_value(value).ok()?;
            let suffix = parameters.name.clone().unwrap_or_else(|| {
                parameters
                    .values
                    .iter()
                    .map(|(key, value)| format!("{key}={}", scalar_text(value)))
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            Some((
                format!("{} [{suffix}]", expanded.name).replace("[#]", &index.to_string()),
                expanded,
            ))
        })
        .collect()
}

fn substitute(value: &mut serde_json::Value, parameters: &BTreeMap<String, serde_json::Value>) {
    match value {
        serde_json::Value::String(text) => {
            for (name, replacement) in parameters {
                let marker = format!("${{{name}}}");
                if text == &marker {
                    *value = replacement.clone();
                    return;
                }
                *text = text.replace(&marker, &scalar_text(replacement));
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                substitute(value, parameters);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values_mut() {
                substitute(value, parameters);
            }
        }
        _ => {}
    }
}

fn scalar_text(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn run_case(
    name: &str,
    case: &TestCase,
    scenario: &Path,
    seed: u64,
    limits: &RunLimits,
) -> CaseResult {
    if let Err(message) = validate_case_scalars(case) {
        return invalid_case(name, seed, message);
    }
    let started = Instant::now();
    let simulator = Simulator::from_scenario_path(scenario);
    let mut simulator = match simulator {
        Ok(simulator) => {
            if case
                .expect_error
                .as_ref()
                .is_some_and(|error| error.kind == ErrorKind::Compile)
            {
                return failed_case(
                    name,
                    seed,
                    "expected a compile error, but compilation succeeded",
                    None,
                );
            }
            simulator
        }
        Err(error) => {
            let compile_error = matches!(error, SimulatorError::Compile(_));
            let text = error.to_string();
            if compile_error && matches_expected_error(case, ErrorKind::Compile, &text) {
                return passed_case(name, seed);
            }
            if !compile_error && matches_expected_error(case, ErrorKind::Runtime, &text) {
                return passed_case(name, seed);
            }
            return failed_case(
                name,
                seed,
                &format!("could not load scenario: {text}"),
                None,
            );
        }
    };
    simulator.set_seed(seed);
    let compatibility_warnings = simulator.compatibility_warnings.clone();
    let thread = match case.program.as_deref() {
        Some(id) => match simulator.cpus.iter().position(|cpu| cpu.program_id == id) {
            Some(index) => index,
            None => match Scenario::load(scenario).ok().and_then(|scenario| {
                scenario
                    .programs
                    .into_iter()
                    .find(|program| program.id == id)
            }) {
                Some(program) if program.language == ProgramLanguage::Lua => {
                    let message = format!(
                        "unsupported runtime: Lua program `{id}` cannot be executed before P3.09"
                    );
                    if matches_expected_error(case, ErrorKind::Runtime, &message) {
                        return passed_case(name, seed);
                    }
                    return failed_case(name, seed, &message, None);
                }
                _ => return invalid_case(name, seed, format!("unknown program `{id}`")),
            },
        },
        None => 0,
    };
    for assertion in &case.assertions {
        if let Err(message) = assertion.expression() {
            return invalid_case(name, seed, message);
        }
    }
    for (target, value) in &case.initial {
        if let Err(message) = set_value(&mut simulator, thread, target, scalar(value)) {
            return invalid_case(
                name,
                seed,
                format!("invalid initial override `{target}`: {message}"),
            );
        }
    }
    let mut drivers = match ScriptDrivers::new(&mut simulator, thread, &case.drivers) {
        Ok(drivers) => drivers,
        Err(message) => return invalid_case(name, seed, message),
    };
    let max_ticks = case.max_ticks.min(limits.max_ticks);
    let max_operations = case.max_operations.min(limits.max_operations);
    let mut operations = 0_u64;
    let mut failures = Vec::new();
    let mut satisfied = vec![false; case.assertions.len()];
    let mut runtime_error = None;

    loop {
        let current_tick = simulator.tick;
        for entry in case
            .timeline
            .iter()
            .filter(|entry| entry.tick == current_tick)
        {
            for (target, value) in &entry.set {
                if let Err(message) = set_value(&mut simulator, thread, target, scalar(value)) {
                    failures.push(failure(
                        &simulator,
                        thread,
                        Some(target),
                        format!("timeline stimulus failed: {message}"),
                        None,
                        None,
                    ));
                }
            }
            for event in &entry.events {
                if let Err(message) =
                    set_value(&mut simulator, thread, &event.target, scalar(&event.value))
                {
                    failures.push(failure(
                        &simulator,
                        thread,
                        Some(&event.target),
                        format!("timeline event failed: {message}"),
                        None,
                        None,
                    ));
                }
            }
        }
        if let Err(message) = drivers.pump(&mut simulator, thread) {
            runtime_error = Some(message);
        }
        evaluate_assertions(&simulator, thread, case, &mut satisfied, &mut failures);
        if !failures.is_empty()
            || runtime_error.is_some()
            || simulator.tick >= max_ticks
            || operations >= max_operations
            || (simulator.is_finished() && !drivers.has_pending())
        {
            break;
        }
        if started.elapsed() > limits.wall_time {
            failures.push(failure(
                &simulator,
                thread,
                None,
                "wall-time limit exceeded".to_owned(),
                Some(format!("<= {} ms", limits.wall_time.as_millis())),
                Some(format!("> {} ms", limits.wall_time.as_millis())),
            ));
            break;
        }
        let start_tick = simulator.tick;
        while simulator.tick == start_tick && operations < max_operations {
            match simulator.scheduler_step() {
                Ok(Some(_)) => {
                    operations += 1;
                    if let Err(message) = drivers.pump(&mut simulator, thread) {
                        runtime_error = Some(message);
                        break;
                    }
                }
                Ok(None) => {
                    if let Err(message) = drivers.pump(&mut simulator, thread) {
                        runtime_error = Some(message);
                        break;
                    }
                }
                Err(error) => {
                    runtime_error = Some(error);
                    break;
                }
            }
        }
        if runtime_error.is_some() {
            break;
        }
    }

    if let Some(error) = runtime_error {
        if !matches_expected_error(case, ErrorKind::Runtime, &error) {
            failures.push(failure(
                &simulator,
                thread,
                None,
                format!("runtime error: {error}"),
                None,
                None,
            ));
        }
    } else if case
        .expect_error
        .as_ref()
        .is_some_and(|error| error.kind == ErrorKind::Runtime)
    {
        failures.push(failure(
            &simulator,
            thread,
            None,
            "expected a runtime error, but none occurred".to_owned(),
            None,
            None,
        ));
    }
    for (index, assertion) in case.assertions.iter().enumerate() {
        if assertion.eventually.is_some() && !satisfied[index] {
            let expression = assertion.expression().unwrap_or("<invalid>");
            let actual = evaluate(&simulator, thread, expression)
                .map(|value| value.display())
                .unwrap_or_else(|error| error);
            failures.push(failure(
                &simulator,
                thread,
                Some(expression),
                "eventual assertion was not satisfied".to_owned(),
                Some("true".to_owned()),
                Some(actual),
            ));
        }
    }
    if simulator.tick < max_ticks {
        for assertion in case.assertions.iter().filter(|assertion| {
            assertion.expression.is_some()
                && assertion.at_tick.is_none()
                && assertion.eventually.is_none()
                && assertion.always.is_none()
        }) {
            let expression = assertion.expression().unwrap_or("<invalid>");
            match assertion_matches(&simulator, thread, assertion, expression) {
                Ok(true) => {}
                Ok(false) => failures.push(assertion_failure(
                    &simulator,
                    thread,
                    assertion,
                    expression,
                    "final assertion failed",
                )),
                Err(error) => failures.push(assertion_error(&simulator, thread, expression, error)),
            }
        }
    }
    if operations >= max_operations && !simulator.is_finished() {
        failures.push(failure(
            &simulator,
            thread,
            None,
            "operation limit exceeded".to_owned(),
            Some(format!("< {max_operations}")),
            Some(operations.to_string()),
        ));
    }
    if let Some(snapshot) = &case.snapshot {
        for (expression, expected) in &snapshot.values {
            match evaluate(&simulator, thread, expression) {
                Ok(actual) if equal_expected(&actual, expected, None) => {}
                Ok(actual) => failures.push(failure(
                    &simulator,
                    thread,
                    Some(expression),
                    format!(
                        "final-state snapshot differs\n- {}\n+ {}",
                        format_scalar(expected),
                        actual.display()
                    ),
                    Some(format_scalar(expected)),
                    Some(actual.display()),
                )),
                Err(error) => failures.push(failure(
                    &simulator,
                    thread,
                    Some(expression),
                    format!("snapshot evaluation failed: {error}"),
                    Some(format_scalar(expected)),
                    None,
                )),
            }
        }
    }
    CaseResult {
        name: name.to_owned(),
        status: if failures.is_empty() {
            Status::Passed
        } else {
            Status::Failed
        },
        seed,
        ticks: simulator.tick,
        operations,
        duration_ms: 0,
        failures,
        compatibility_warnings,
    }
}

fn validate_case_scalars(case: &TestCase) -> Result<(), String> {
    let mut values: Vec<(&str, &Scalar)> = case
        .initial
        .iter()
        .map(|(target, value)| (target.as_str(), value))
        .collect();
    for entry in &case.timeline {
        values.extend(
            entry
                .set
                .iter()
                .map(|(target, value)| (target.as_str(), value)),
        );
        values.extend(
            entry
                .events
                .iter()
                .map(|event| (event.target.as_str(), &event.value)),
        );
    }
    for driver in &case.drivers {
        for rule in &driver.rules {
            if let Some(value) = &rule.when.equals {
                values.push((rule.when.target.as_str(), value));
            }
            collect_script_scalars(&rule.actions, &mut values);
        }
    }
    values.extend(case.assertions.iter().filter_map(|assertion| {
        assertion
            .expected
            .as_ref()
            .map(|value| ("assertion", value))
    }));
    if let Some(snapshot) = &case.snapshot {
        values.extend(
            snapshot
                .values
                .iter()
                .map(|(expression, value)| (expression.as_str(), value)),
        );
    }
    for (location, value) in values {
        value
            .as_f64()
            .map_err(|error| format!("invalid numeric value for `{location}`: {error}"))?;
    }
    Ok(())
}

fn collect_script_scalars<'a>(
    actions: &'a [crate::schema::ScriptAction],
    values: &mut Vec<(&'a str, &'a Scalar)>,
) {
    for action in actions {
        match action {
            crate::schema::ScriptAction::Set { target, value } => {
                values.push((target.as_str(), value));
            }
            crate::schema::ScriptAction::Publish { network, value, .. } => {
                values.push((network.as_str(), value));
            }
            crate::schema::ScriptAction::Schedule { actions, .. } => {
                collect_script_scalars(actions, values);
            }
            crate::schema::ScriptAction::MoveSlot { .. } => {}
        }
    }
}

fn evaluate_assertions(
    simulator: &Simulator,
    thread: usize,
    case: &TestCase,
    satisfied: &mut [bool],
    failures: &mut Vec<Failure>,
) {
    for (index, assertion) in case.assertions.iter().enumerate() {
        let expression = match assertion.expression() {
            Ok(value) => value,
            Err(_) => continue,
        };
        if assertion.at_tick.is_some_and(|tick| tick != simulator.tick) {
            continue;
        }
        if assertion.eventually.is_some() {
            let deadline = assertion.within_ticks.unwrap_or(case.max_ticks);
            match assertion_matches(simulator, thread, assertion, expression) {
                Ok(true) => satisfied[index] = true,
                Ok(false) if simulator.tick >= deadline && !satisfied[index] => {
                    failures.push(assertion_failure(
                        simulator,
                        thread,
                        assertion,
                        expression,
                        "eventual assertion deadline expired",
                    ))
                }
                Err(error) => failures.push(assertion_error(simulator, thread, expression, error)),
                _ => {}
            }
        } else if assertion.always.is_some()
            || assertion.at_tick == Some(simulator.tick)
            || (assertion.at_tick.is_none() && simulator.tick == case.max_ticks)
        {
            match assertion_matches(simulator, thread, assertion, expression) {
                Ok(true) => satisfied[index] = true,
                Ok(false) => failures.push(assertion_failure(
                    simulator,
                    thread,
                    assertion,
                    expression,
                    if assertion.always.is_some() {
                        "invariant assertion failed"
                    } else {
                        "assertion failed"
                    },
                )),
                Err(error) => failures.push(assertion_error(simulator, thread, expression, error)),
            }
        }
    }
}

fn assertion_matches(
    simulator: &Simulator,
    thread: usize,
    assertion: &Assertion,
    expression: &str,
) -> Result<bool, String> {
    let actual = evaluate(simulator, thread, expression)?;
    Ok(match &assertion.expected {
        Some(expected) => equal_expected(&actual, expected, assertion.tolerance.as_ref()),
        None => actual.truthy()?,
    })
}

fn equal_expected(
    actual: &Value,
    expected: &Scalar,
    tolerance: Option<&crate::schema::Tolerance>,
) -> bool {
    let expected = scalar(expected);
    let Ok(actual) = actual.number() else {
        return false;
    };
    if actual.is_nan() || expected.is_nan() {
        return actual.is_nan() && expected.is_nan();
    }
    if actual.is_infinite() || expected.is_infinite() || (actual == 0.0 && expected == 0.0) {
        return actual.to_bits() == expected.to_bits();
    }
    let tolerance = tolerance.cloned().unwrap_or_default();
    let difference = (actual - expected).abs();
    difference
        <= tolerance
            .absolute
            .max(tolerance.relative * actual.abs().max(expected.abs()))
}

fn assertion_failure(
    simulator: &Simulator,
    thread: usize,
    assertion: &Assertion,
    expression: &str,
    message: &str,
) -> Failure {
    let actual = evaluate(simulator, thread, expression)
        .map(|value| value.display())
        .unwrap_or_else(|error| error);
    failure(
        simulator,
        thread,
        Some(expression),
        message.to_owned(),
        Some(
            assertion
                .expected
                .as_ref()
                .map(format_scalar)
                .unwrap_or_else(|| "true".to_owned()),
        ),
        Some(actual),
    )
}

fn assertion_error(
    simulator: &Simulator,
    thread: usize,
    expression: &str,
    error: String,
) -> Failure {
    failure(
        simulator,
        thread,
        Some(expression),
        format!("could not evaluate assertion: {error}"),
        None,
        None,
    )
}

fn failure(
    simulator: &Simulator,
    thread: usize,
    expression: Option<&str>,
    message: String,
    expected: Option<String>,
    actual: Option<String>,
) -> Failure {
    let cpu = simulator.cpus.get(thread);
    let possibly_unsupported =
        !simulator.compatibility_warnings.is_empty() || message.contains("unsupported");
    Failure {
        message,
        expression: expression.map(str::to_owned),
        expected,
        actual,
        tick: Some(simulator.tick),
        source: cpu.map(|cpu| cpu.program.source_path.clone()),
        line: cpu.and_then(|cpu| cpu.current_line()).map(|line| line + 1),
        object: expression.and_then(expression_object),
        possibly_unsupported,
    }
}

fn expression_object(expression: &str) -> Option<String> {
    expression
        .split_once("(\"")
        .and_then(|(_, rest)| rest.split_once("\")"))
        .map(|(id, _)| id.to_owned())
}

fn matches_expected_error(case: &TestCase, kind: ErrorKind, message: &str) -> bool {
    case.expect_error.as_ref().is_some_and(|expected| {
        expected.kind == kind
            && expected
                .message_contains
                .as_ref()
                .is_none_or(|part| message.contains(part))
    })
}

fn scalar(value: &Scalar) -> f64 {
    value.as_f64().unwrap_or(f64::NAN)
}

fn format_scalar(value: &Scalar) -> String {
    match value {
        Scalar::Number(value) => format_number(*value),
        Scalar::Text(value) => value.clone(),
    }
}

fn passed_case(name: &str, seed: u64) -> CaseResult {
    CaseResult {
        name: name.to_owned(),
        status: Status::Passed,
        seed,
        ticks: 0,
        operations: 0,
        duration_ms: 0,
        failures: vec![],
        compatibility_warnings: vec![],
    }
}
fn failed_case(name: &str, seed: u64, message: &str, expression: Option<&str>) -> CaseResult {
    CaseResult {
        name: name.to_owned(),
        status: Status::Failed,
        seed,
        ticks: 0,
        operations: 0,
        duration_ms: 0,
        failures: vec![Failure {
            message: message.to_owned(),
            expression: expression.map(str::to_owned),
            expected: None,
            actual: None,
            tick: None,
            source: None,
            line: None,
            object: None,
            possibly_unsupported: message.contains("unsupported"),
        }],
        compatibility_warnings: vec![],
    }
}
fn invalid_case(name: &str, seed: u64, message: String) -> CaseResult {
    let mut result = failed_case(name, seed, &message, None);
    result.status = Status::Invalid;
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;

    use super::discover;

    #[test]
    fn discovery_is_sorted_and_ignores_target() {
        let root =
            std::env::temp_dir().join(format!("ic10-runner-discovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("b.ic10test.json"), "{}").unwrap();
        fs::write(root.join("a.ic10test.json"), "{}").unwrap();
        fs::write(root.join("target/ignored.ic10test.json"), "{}").unwrap();
        let found = discover(std::slice::from_ref(&root)).unwrap();
        assert_eq!(found.iter().collect::<BTreeSet<_>>().len(), 2);
        assert!(found[0].ends_with("a.ic10test.json"));
        let _ = fs::remove_dir_all(root);
    }
}
