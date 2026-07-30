# Scenario testing

Scenario testing lets you describe a small Stationeers situation, run an IC10
program in it, and check what happened. It is much easier to trust an
automation program when the same test can be run repeatedly after every change.

You do not need VS Code or a running Stationeers game to run these tests. The
bundled simulator runs the scenario using the same deterministic world model
used by the debugger.

## The basic idea

Think of a scenario test as a short story:

1. **Initial state** — set up the world before the program starts.
2. **Timeline** — change something at a particular simulation tick.
3. **Assertions** — describe what must be true while the story runs.
4. **Snapshot** — record the important final values.

The reusable world layout lives in a `*.stationeerssim.json` simulation file.
The test file, `*.stationeerstest.json`, supplies the situation and expected
results. This keeps one environment reusable across many test cases.

## A complete example

This example starts a controller with a pressure sensor reading, changes the
sensor after two ticks, and checks that the exterior door eventually opens.

```json
{
  "schemaVersion": 1,
  "scenario": "./airlock.stationeerssim.json",
  "cases": [
    {
      "name": "opens after depressurising",
      "maxTicks": 20,
      "focusProgram": "airlock-controller",
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
      ],
      "snapshot": {
        "values": {
          "r0": 0,
          "device(\"exterior\").Open": 1
        }
      }
    }
  ]
}
```

You can create this structure with **IC10: Create Scenario Test**, then use
the visual editor. **Open JSON** is available whenever you want to inspect or
edit the source directly.

## Initial state: where a test starts

`initial` is the starting line-up for one test case. It overrides selected
values in the reusable simulation before the first world tick. Use it when you
want a test to begin with a known condition, such as:

- a register containing a setup value;
- a tank or sensor at a particular pressure or temperature;
- a device switched on or off;
- an item in a slot;
- a cable-network channel containing a message or value.

For example, this test starts with a full tank and an empty output counter:

```json
"initial": {
  "device(\"tank\").Pressure": 500,
  "device(\"pump\").On": 0,
  "device(\"output\").ExportCount": 0
}
```

Why use it? Without an explicit initial state, a test can accidentally depend
on whatever values happen to be in the shared simulation file. Initial state
makes the test explainable and repeatable: anyone can see the conditions under
which the program is being checked.

Initial state is not a recording of the whole world. The simulation file still
defines the devices and networks; `initial` only changes the values relevant to
this case.

## Assertions: what the program must prove

The `expect` array contains assertions. An assertion is simply a statement that
must be true for the test to pass. The expression language can inspect
registers, stacks, devices, slots, memory, network channels, and `tick`.

### Check one value at one moment

Use `expression` when you want to check one exact tick. Add `expected` when the
expression returns a number:

```json
{
  "expression": "r0",
  "expected": 42,
  "atTick": 3
}
```

If you leave out `expected`, the expression is treated as a truth check:

```json
{
  "expression": "device(\"lamp\").On == 1",
  "atTick": 4
}
```

If `atTick` is omitted, the expression is checked against the final state.

### Check that something eventually happens

Use `eventually` when the exact tick is not important, but the result must occur
before a deadline. This is useful for programs that need a few ticks to react:

```json
{
  "eventually": "device(\"door\").Open == 1",
  "withinTicks": 10
}
```

The assertion passes as soon as the expression becomes true. If it is still
false after ten ticks, the test fails.

### Check that something is always safe

Use `always` for an invariant: a condition that must remain true throughout
the run. This is ideal for safety rules:

```json
{
  "always": "device(\"inner-door\").Open == 0"
}
```

This catches a door opening at the wrong time even if the program eventually
reaches its intended final state.

### Allow small numeric differences

For finite numeric values, `tolerance` allows a controlled difference:

```json
{
  "expression": "device(\"sensor\").Temperature",
  "expected": 293.15,
  "tolerance": {
    "absolute": 0.05,
    "relative": 0.001
  }
}
```

Use tolerance only when the simulation or calculation genuinely permits a
small difference. Exact assertions are easier to understand when exact values
are expected.

## Timeline: changing the world while it runs

The `timeline` is a list of outside events. Each event happens at a simulation
tick and changes one or more assignable values. It represents something the
program does not control: a sensor changing, a player pressing a button, a
network message arriving, or a machine receiving a new input.

```json
"timeline": [
  {
    "tick": 5,
    "set": {
      "device(\"button\").Activate": 1,
      "network(\"control\").Channel0": 99
    }
  }
]
```

Timeline ticks are simulation ticks, not seconds. Keeping the test driven by
ticks makes it deterministic and avoids depending on the speed of your
computer. Use `initial` for conditions that exist before the program begins;
use `timeline` for changes that happen during the test.

## Parameters: one test with many inputs

Parameters let you run the same case several times with different values. This
is useful for testing sunrise, noon, and sunset without duplicating the whole
case.

```json
{
  "name": "tracks ${name}",
  "initial": {
    "r2": "${angle}"
  },
  "parameters": [
    { "name": "sunrise", "angle": -90 },
    { "name": "noon", "angle": 0 },
    { "name": "sunset", "angle": 90 }
  ],
  "expect": [
    {
      "expression": "r2",
      "expected": "${angle}"
    }
  ]
}
```

The editor and Test Explorer show these as named child runs. `${angle}` is
replaced separately for each parameter row, so a failure tells you which input
failed. Parameters can be used in names, expressions, targets, and scalar
values.

## Snapshots: important final values

A `snapshot` is a compact final-state checklist. It maps expressions to the
values they must have when the case finishes:

```json
"snapshot": {
  "values": {
    "r2": 1,
    "device(\"output\").ExportCount": 1,
    "device(\"door\").Open": 0
  }
}
```

Use assertions to describe behaviour over time—“the door never opens while the
inner room is occupied”—and snapshots to describe the final result—“one item
was exported and the door is closed”. A snapshot is not a screenshot; it is a
small, deterministic set of values that makes regressions easy to spot.

## Expected errors

Some tests should prove that invalid programs fail safely. Use `expectError`
when compilation or runtime failure is the expected outcome:

```json
"expectError": {
  "kind": "compile",
  "messageContains": "unknown instruction"
}
```

The supported kinds are `compile` and `runtime`. Keep the optional message
fragment short enough that it describes the important part of the error.

## Running tests in VS Code

1. Open a `*.stationeerstest.json` file.
2. Use **Validate** to check the scenario, programs, bounds, and assertions.
3. Use **Run case** to execute the selected case.
4. Use **Open JSON** for advanced editing or source-control review.
5. Use the Debug action or Test Explorer when you need to pause at a breakpoint.

Test Explorer shows the file, case, and parameter levels. When a test fails,
the failure includes the assertion, tick, observed value, and relevant object
where available. Saving a referenced program or simulation invalidates affected
results; set `ic10.testing.rerunOnSave` to run them again automatically.

## Headless runs and CI

The bundled `ic10` runner emits human-readable, JSON, or JUnit results:

```text
ic10 check tests examples/airlock.stationeerssim.json
ic10 test --filter airlock tests
ic10 test --format json --output results.json tests
ic10 test --format junit --output results.xml tests
```

Use JUnit output when your CI service understands test reports, or JSON when a
script needs structured failure details. Every case has explicit tick and
operation bounds, so a broken program cannot run forever.

## File names and compatibility

Use the canonical `*.stationeerstest.json` suffix. Legacy `*.ic10test.json`
files remain readable and are not silently renamed. See the repository's
[complete scenario testing reference](../scenario-testing.md) for the full
schema, scripted device drivers, Lua module tests, migration policy, and
repeatability guarantees.
