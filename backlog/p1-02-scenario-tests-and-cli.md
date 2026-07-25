# P1.02 — Scenario tests and headless CLI

## Goal

Turn deterministic simulation environments into repeatable regression tests
that run from VS Code and continuous integration.

## File model

Introduce `*.ic10test.json` rather than embedding test cases directly in
`*.ic10sim.json`. A test file references one base scenario and contains cases:

```json
{
  "schemaVersion": 1,
  "scenario": "./airlock.ic10sim.json",
  "cases": [
    {
      "name": "opens after the chamber is depressurised",
      "maxTicks": 20,
      "timeline": [
        {
          "tick": 2,
          "set": {
            "device(\"chamber-sensor\").Pressure": 0
          }
        }
      ],
      "expect": [
        {
          "eventually": "device(\"exterior-door\").Open == 1",
          "withinTicks": 10
        },
        {
          "always": "device(\"interior-door\").Open == 0"
        }
      ]
    }
  ]
}
```

Keep the expression language shared with the debugger. The schema should
support:

- per-case initial overrides;
- tick-based state changes and device/network events;
- assertions at exact ticks;
- `eventually`, `withinTicks`, and `always`;
- approximate numeric equality with absolute/relative tolerance;
- NaN, infinities, and signed zero;
- expected compile/runtime errors;
- parameter tables expanded into named cases;
- selected IC focus without changing shared-world scheduling;
- optional final-state snapshots.

## Test runner

Add a protocol-neutral runner crate and a small `ic10` CLI:

```text
ic10 check <paths>
ic10 test <paths>
ic10 sim <scenario>
ic10 compatibility
```

Requirements:

- deterministic case ordering and random seeds;
- human-readable and JSON output;
- JUnit output for CI;
- non-zero exit status for failures or invalid fixtures;
- source/scenario locations attached to failures;
- filtering by file and test name;
- bounded ticks, operations, and wall time;
- no VS Code dependency.

## VS Code integration

Use the native Test Explorer:

- discover `*.ic10test.json`;
- show file, case, and parameter hierarchy;
- run or debug a case;
- display duration and failure diffs;
- navigate failures to source or environment objects;
- re-run tests affected by a saved source/scenario change;
- optionally show coverage after a run.

Debugging a test launches the existing DAP with the test case's scheduled
stimuli and pauses on assertion failure.

## Failure quality

A failure should answer:

- which expression failed;
- expected and actual values;
- tick and IC instruction location;
- last writer when trace data is available;
- a compact diff for snapshots;
- whether the failure could be caused by an unsupported simulator behaviour.

## Acceptance criteria

- [x] The schema supports initial overrides, timeline stimuli, exact/eventual/
      invariant assertions, tolerance, and expected errors.
- [x] A headless runner executes the same simulator code as VS Code.
- [x] Test Explorer can discover, run, debug, and navigate every case.
- [x] CI can consume JSON or JUnit results.
- [x] Test results are byte-for-byte deterministic for a fixed seed and build.
- [x] Infinite programs are bounded by explicit tick/operation limits.
- [x] Failed assertions include tick, expression, expected/actual, and source
      context.
- [x] Schema validation and migration behaviour are documented.
- [x] Example solar, airlock, multi-IC handshake, and failure fixtures ship
      with the repository.

## Dependencies

- [P0.02](p0-02-simulator-conformance.md) compatibility reporting.
- The richer evaluator in [P1.03](p1-03-debugger-power-features.md) may be
  developed as a shared library while this item is implemented.

## Non-goals

- Recreating a complete Stationeers save in every unit test.
- Making tests depend on wall-clock timing.

## Decisions

- Tests live in separate `*.ic10test.json` files that reference reusable
  simulation environments.
- The evaluator is shared by tests and the debugger.
