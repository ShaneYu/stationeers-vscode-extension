//! Protocol-neutral scenario testing built directly on `ic10-sim`.

mod evaluator;
mod runner;
mod schema;
mod script_driver;

pub use evaluator::{
    Value, evaluate, evaluate_with_changed, format_number, set_value, set_value_as,
};
pub use runner::{
    CaseResult, Failure, FileResult, RunLimits, RunRequest, RunSummary, Status, discover,
    load_expanded_case, run_files,
};
pub use schema::{
    Assertion, ErrorExpectation, ParameterSet, ScenarioTest, ScriptAction, ScriptTrigger,
    ScriptedDriver, ScriptedRule, Snapshot, TestCase, TestFileError, TimelineEntry,
};
