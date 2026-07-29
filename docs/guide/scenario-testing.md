# Scenario testing

Scenario tests put repeatable world state, stimuli, and assertions beside the
program they exercise. Use `*.stationeerstest.json`; the legacy
`*.ic10test.json` format remains readable.

## Test Explorer

Open a scenario test to use its visual editor for cases, initial state,
assertions, timeline events, parameters, expected errors, and snapshots.
**Validate** checks the fixture; **Run case** executes it; **Open JSON** exposes
the canonical source when you need advanced edits.

## Headless runs

The bundled `ic10` runner emits human-readable, JSON, or JUnit results, making
it suitable for local scripts and continuous integration. Test files can share
a `*.stationeerssim.json` environment and override only the state needed by a
case.

Read the repository's [scenario testing reference](../scenario-testing.md) for
the complete schema, migration policy, and examples.
