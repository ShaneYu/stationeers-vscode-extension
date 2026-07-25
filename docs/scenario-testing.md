# IC10 scenario tests and headless CLI

Scenario tests turn a reusable `*.ic10sim.json` environment into deterministic
regression cases. They execute `ic10-sim` directly—the same shared-world
scheduler used by the debugger—and do not require VS Code or Stationeers.

## Test file format

Test files end in `*.ic10test.json`. VS Code validates them with the bundled
schema and discovers them in Test Explorer.

```json
{
  "schemaVersion": 1,
  "scenario": "./airlock.ic10sim.json",
  "seed": 73,
  "cases": [
    {
      "name": "opens after depressurising",
      "maxTicks": 20,
      "maxOperations": 100000,
      "focusIc": "airlock-controller",
      "initial": {
        "r0": 0,
        "device(\"sensor\").Pressure": 100
      },
      "timeline": [
        {
          "tick": 2,
          "set": {
            "device(\"sensor\").Pressure": 0
          }
        }
      ],
      "expect": [
        {
          "expression": "r0",
          "expected": 0,
          "atTick": 1
        },
        {
          "eventually": "device(\"exterior\").Open == 1",
          "withinTicks": 10
        },
        {
          "always": "device(\"interior\").Open == 0"
        }
      ]
    }
  ]
}
```

Assignable locations are registers, `stack[n]`, device fields, device slots,
device memory, and cable channels. Timeline `events` are an array of
`{"target": "...", "value": ...}` entries and are equivalent to `set`; the
separate spelling is useful when a fixture models an external device or
network event.

Assertions use the debugger expression language. It supports registers,
stack, tick, device/slot/memory fields, cable channels, numeric constants,
parentheses, arithmetic, comparisons, and `!`, `&&`, and `||`.

- `expression` with `atTick` checks one exact tick. Without `atTick`, it checks
  final state.
- `eventually` succeeds on the first matching tick and must match by
  `withinTicks` (or `maxTicks`).
- `always` is checked at every tick, after that tick's stimuli.
- `expected` compares the expression's numeric result. Omit it for a truth
  assertion.
- `tolerance` accepts non-negative `absolute` and `relative` values. The larger
  bound is used.

`"NaN"`, `"Infinity"`, `"-Infinity"`, and `"-0"` preserve IEEE-754 values that
JSON cannot encode. NaNs compare equal to NaNs for test assertions; infinities
and signed zero compare by their exact representation. Approximate comparison
is used only for finite, non-zero values.

`expectError` accepts `kind: "compile"` or `"runtime"` plus optional
`messageContains`. `parameters` expands a case into named children; `${name}`
markers are replaced in names, expressions, targets, and scalar values.
`snapshot.values` is a sorted expression-to-value map checked at final state.

## Bounds and repeatability

Every case has explicit tick and operation bounds. Command-line ceilings can
only reduce fixture bounds. The operation ceiling is enforced between
instructions, including inside a world tick. The runner also has a wall-time
guard for host failures; simulation never reads wall-clock time.

Files, cases, parameters, maps, failures, and seeds have stable ordering.
Machine output uses a fixed `durationMs: 0`, so JSON and JUnit are
byte-for-byte repeatable for a fixed path, seed, and build.

## Command line

Build with `cargo build -p ic10-runner`; the executable is named `ic10`.

```text
ic10 check tests examples/airlock.ic10sim.json
ic10 test --filter airlock tests
ic10 test --format json --output results.json tests
ic10 test --format junit --output results.xml tests
ic10 test --max-ticks 100 --max-operations 1000000 --wall-time-ms 30000 tests
ic10 sim examples/airlock.ic10sim.json --max-ticks 100 --json
ic10 compatibility --json
```

`check` validates test files and compiles their scenarios. `test` recursively
discovers test files and returns 1 for failed/invalid cases (2 for command
errors). `sim` returns non-zero if its tick bound is reached before completion.
JSON result objects carry fixture/scenario paths, status, seed, ticks,
operations, compatibility warnings, and structured failures. JUnit uses one
suite per file and one testcase per expanded case.

## VS Code

Test Explorer shows file, case, and parameter levels. Run uses the bundled CLI.
Debug launches the existing DAP over the complete scenario, preserves normal
multi-IC scheduling, applies the selected case's initial/timeline state, and
pauses all threads on assertion failure. Failure messages link to the active
IC source line and include the object ID where available.

Saving a referenced program, scenario, or fixture invalidates affected test
results. Set `ic10.testing.rerunOnSave` to automatically run them again.
`ic10.cli.path` selects a development CLI executable.

## Schema versions and migration

Version 1 is the only accepted version. `schemaVersion` is required; unknown
versions are rejected rather than guessed or silently rewritten. Fixtures
remain ordinary source-controlled JSON and the runner never modifies them.

There was no released pre-versioned test schema. To migrate an experimental
file, split tests out of the scenario, set `schemaVersion` to 1, add a relative
`scenario`, move test entries under `cases`, rename final checks to
`expect`, and encode special numbers as the strings listed above. Run
`ic10 check` after migration. Future schema changes will document explicit
field-by-field steps here.

Examples include
[solar](../examples/scenario-tests/solar/solar.ic10test.json),
[airlock](../examples/scenario-tests/airlock/airlock.ic10test.json),
[multi-IC handshake](../examples/multi-ic/ingot-supplier.ic10test.json), and a
deliberate [failure](../examples/scenario-tests/failures/assertion-failure.ic10test.json).

