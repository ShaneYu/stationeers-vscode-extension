# IC10 scenario tests and headless CLI

Scenario tests turn a reusable `*.stationeerssim.json` environment into deterministic
regression cases. They execute `ic10-sim` directly—the same shared-world
scheduler used by the debugger—and do not require VS Code or Stationeers.
Lua programs attached to simulated devices currently fail closed with the
`lua-runtime-unavailable` diagnostic; see the
[local Lua simulator profile](live-integration/lua-simulator-profile.md).

P3-09A changeset A embeds a sandboxed Lua 5.2 runtime and implements an
explicit pure-module test mode. The selected profile is
`stationeerslua-0.9.5.0-lua5.2-pure-module-v1`, using pinned `mlua` 0.12.0
with `lua52` and `vendored`.

That mode resolves only `.lua` files beneath explicit test-relative module
roots through a deterministic custom `require()`. It exposes only
the safe pure standard-library subset (`base`, `string`, `table`, `math`, and
`bit32`) and enforces instruction, recursion, memory, output,
module-count/source-size, and wall-time limits. Stationeers
host APIs—including `device.*`, `ic.*`, `tick`, `yield`, `sleep`, persistence,
events, messaging, networks, game/library-chip modules, HTTP, and random
services—remain unsupported. Full Lua-chip and mixed IC10/Lua scenario
execution are later changesets.

## Test file format

Test files use the canonical `*.stationeerstest.json` suffix. The legacy
`*.ic10sim.json` and `*.ic10test.json` suffixes remain readable and are never
silently renamed; see [workspace formats](live-integration/workspace-formats.md).

```json
{
  "schemaVersion": 1,
  "scenario": "./airlock.stationeerssim.json",
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
ic10 check tests examples/airlock.stationeerssim.json
ic10 build examples/demo.ic10 --optimization compact
ic10 test --filter airlock tests
ic10 test --format json --output results.json tests
ic10 test --format junit --output results.xml tests
ic10 test --max-ticks 100 --max-operations 1000000 --wall-time-ms 30000 tests
ic10 test --lua-library libraries tests
ic10 sim examples/airlock.stationeerssim.json --max-ticks 100 --json
ic10 compatibility --json
```

The optional `stationeersToolkit.lua.libraryPaths` workspace setting is an array of
workspace-relative directories used as additional Lua `require()` roots by
Test Explorer and simulation/debug launches. The CLI equivalents are repeated
`--lua-library DIR` options on `test`, `check`, and `sim`. The entry program’s
directory remains the first module root, so local sibling modules continue to
take precedence over configured libraries.

`build` uses the same deterministic deployment engine as VS Code; see the
[deployment build guide](deployment-builds.md) for its output and safety
options. `check` validates test files and compiles their scenarios. `test`
recursively discovers test files and returns 1 for failed/invalid cases (2 for
command errors). `sim` returns non-zero if its tick bound is reached before
completion.
JSON result objects carry fixture/scenario paths, status, seed, ticks,
operations, compatibility warnings, and structured failures. JUnit uses one
suite per file and one testcase per expanded case.

## VS Code

Test Explorer shows file, case, and parameter levels. Items are labelled with
their detected runtime (`IC10`, `Lua module`, `Lua chip`, or `IC10 + Lua`), and
failure locations open the reported source file relative to the scenario. Run
uses the bundled CLI.
Debug launches the existing DAP over the complete scenario, preserves normal
multi-IC scheduling, applies the selected case's initial/timeline state, and
pauses all threads on assertion failure. Failure messages link to the active
IC source line and include the object ID where available.

Saving a referenced program, scenario, or fixture invalidates affected test
results. Set `ic10.testing.rerunOnSave` to automatically run them again.
`ic10.cli.path` selects a development CLI executable.

Opening `*.stationeerstest.json` uses the guided visual editor by default. Legacy
`*.ic10test.json` files open through the same compatibility path. Run
**IC10: Create Scenario Test** to create a fixture, optionally from an active
simulation environment. The editor provides:

- scenario selection and deterministic seed controls;
- add, duplicate, rename, and delete operations for cases;
- guarded initial state, timeline state/events, and final snapshot maps;
- exact, eventual, and invariant assertions with deadlines and tolerances;
- parameter tables and expected compile/runtime errors;
- constrained scripted device drivers for unsupported active devices;
- inline cross-field checks for invalid bounds, duplicate names, malformed
  values, and incomplete assertions.

The visual editor reads the selected simulation and suggests its IC housings,
registers, device fields, slots, memory, and network channels. Target,
expression, expected-value, and stimulus inputs retain free-form editing while
offering these completions and inline syntax checks. **Validate** runs the
headless CLI's full fixture/scenario/program check. **Run case** executes the
selected case (including every parameter row) and keeps its latest pass/fail
state visible until the case changes.

Parameters appear before state and assertions because they supply values used
below. If a parameter set contains `"angle": -90`, the placeholder
`"${angle}"` is replaced with `-90` for that expanded run. An exact scalar
placeholder must resolve to a number or encoded special number before the
expanded case is validated. Boolean parameters are useful inside expression
text, where placeholders can also be embedded in surrounding syntax.

Edits use VS Code's text-document edit path, so normal save, undo, redo, and
source control behaviour remains intact. Invalid form state is kept in the
editor but is not written over the last valid JSON. Use **Open JSON** at any
time for advanced editing or to repair syntax that cannot be parsed. The JSON
file remains the canonical, reviewable source, and the schema continues to
provide validation and completion in that source view.

## Scripted device drivers

A case can add `drivers` when a device has no built-in active behaviour.
Each versioned driver contains declaration-ordered rules. A rule's `when`
target uses the same target syntax as initial state and may include `equals`.
It fires when that target's numeric value changes, and can perform:

- `set` with a target and scalar value;
- `moveSlot` between `device("id").slot[n]` endpoints;
- `publish` to a named network and channel 0–7;
- `schedule` with `afterTicks` and nested actions.

Values and targets support the normal parameter substitution. Rule and event
ordering is stable; scheduled actions use simulation ticks only. The format
cannot execute JavaScript, access the filesystem, create threads, or read wall
clock time. Hard limits on drivers, rules, nested actions, pending events, and
reaction cascades turn accidental cycles into a runtime failure. Those errors
include the driver ID, model/version, and rule name, and are preserved in text,
JSON, and JUnit output.

The visual editor can create drivers and rules, completes trigger targets and
values, and validates the declarative action array. JSON schema completion is
also available for direct editing.

## Pure Lua module tests

P3-09A adds an explicit `luaModule` execution mode for pure Lua 5.2 modules.
Set `focusProgram` to a Lua entry script and add:

```json
"execution": {
  "kind": "luaModule",
  "profile": "stationeerslua-0.9.5.0-lua5.2-pure-module-v1",
  "moduleRoots": ["."]
}
```

The entry script uses Lua `assert` for checks and the sandboxed `require()` for
workspace modules. `maxOperations` is the instruction budget; the execution
object can also bound memory, output, modules, aggregate source, and recursion.
Fixture values cannot raise the host ceilings: 10,000,000 instructions,
30 seconds, 64 MiB memory, 1 MiB captured output, 256 modules, 4 MiB aggregate
source, and 512 calls. Scenario, entry, and module-root paths must stay beneath
the test file directory after canonicalization; absolute paths, parent
traversal, and symlink/junction escapes are rejected.
Test Explorer Run, CLI filtering, JSON/JUnit results, and failure navigation
work normally. IC10 Debug is intentionally unavailable for this mode.

This is not simulated Lua-chip execution. World state, timelines, scripted
drivers, world expressions, and snapshots are rejected in `luaModule` cases.
Stationeers APIs such as `ic`, `device`, `tick`, `yield`, and `sleep` remain
explicitly unsupported. See the
[pure module example](../examples/lua-modules/README.md).

World-attached Lua programs are validated separately from `luaModule` tests,
but both execution modes support the same sandboxed source-relative
`require()` resolver. The mixed-language example demonstrates a world Lua
program importing logic that is also covered by a Lua module unit test.
Otherwise structurally valid Lua-only and mixed IC10/Lua worlds fail with
`lua-runtime-unavailable` before any program or world tick executes;
unsupported Lua chips are never silently skipped.

Debug reports a distinct local Lua-chip/mixed-world unsupported path and does
not start the IC10 adapter. This is separate from remote StationeersLua
live-game source/debugging. Packaged examples cover `pure-lua-library`,
`full-lua-chip`, and `mixed-ic10-lua`.

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
[solar](../examples/scenario-tests/solar/solar.stationeerstest.json),
[airlock](../examples/scenario-tests/airlock/airlock.stationeerstest.json),
[multi-IC handshake](../examples/multi-ic/ingot-supplier.stationeerstest.json), and a
deliberate [failure](../examples/scenario-tests/failures/assertion-failure.ic10test.json).
