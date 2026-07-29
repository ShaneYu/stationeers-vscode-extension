# Scenario test format

Scenario tests use `*.stationeerstest.json`. A test file contains one or more
cases, each with the environment or fixture state it needs, scheduled stimuli,
assertions, parameters, and optional expected errors or snapshots.

The visual editor preserves valid JSON and provides validation before a run.
Legacy `*.ic10test.json` files remain readable for migration.

For the complete schema and runnable examples, see the repository's
[scenario testing guide](../scenario-testing.md).
