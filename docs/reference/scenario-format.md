# Scenario test format

Scenario tests use `*.ictest`. A test file contains one or more
cases, each with the environment or fixture state it needs, scheduled stimuli,
assertions, parameters, and optional expected errors or snapshots.

The visual editor preserves valid JSON and provides validation before a run.
Older scenario-test filenames are rejected; rename them to `*.ictest`.

For the complete schema and runnable examples, see the repository's
[scenario testing guide](../scenario-testing.md).
