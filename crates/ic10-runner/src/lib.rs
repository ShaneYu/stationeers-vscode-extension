//! Protocol-neutral scenario testing built directly on `ic10-sim`.

mod evaluator;
mod runner;
mod schema;

pub use evaluator::{Value, evaluate, evaluate_with_changed, format_number, set_value};
pub use runner::{
    CaseResult, Failure, FileResult, RunLimits, RunRequest, RunSummary, Status, discover,
    load_expanded_case, run_files,
};
pub use schema::{
    Assertion, ErrorExpectation, ParameterSet, ScenarioTest, Snapshot, TestCase, TestFileError,
    TimelineEntry,
};
