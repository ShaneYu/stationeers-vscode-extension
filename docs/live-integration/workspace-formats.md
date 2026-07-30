# Neutral workspace formats

P3.01 establishes a language-neutral workspace contract for simulation and
scenario files. The canonical suffixes are:

| Purpose | Filename |
| --- | --- |
| Simulation environment | `name.icsim` |
| Scenario test | `name.ictest` |
| Simulation layout sidecar | `name.icsimlayout` |

These are the only supported workspace filenames. Older simulation, test, and
layout filenames are rejected and must be renamed before they can be opened or
run. This release intentionally makes a clean break because the project had no
meaningful installed user base when the new names were introduced.

From this point forward, breaking changes will be avoided where practical. If
a future breaking change is necessary, it will include a documented migration
path rather than silently invalidating existing workspaces.

## Neutral metadata

Program references carry an explicit language. The only accepted values are
`ic10` and `lua`; the language is not inferred from the filename.

```json
{
  "schemaVersion": 1,
  "programs": [
    { "id": "controller", "language": "ic10", "path": "./controller.ic10" },
    { "id": "telemetry", "language": "lua", "path": "./telemetry.lua" }
  ],
  "devices": [
    { "id": "controller-chip", "prefab": "StructureCircuitHousing", "programId": "controller" }
  ]
}
```

`focusProgram` selects the program used by a test or debug launch when a
scenario contains more than one program. It is a stable program ID, not a
filename. A selector is optional; consumers choose the first runnable program
when it is absent.

The IC10-only runtime currently accepts the neutral metadata for documentation
and workspace migration, but it cannot execute a Lua program before P3.09.
Selecting a Lua `focusProgram`, or reaching a Lua program during simulation,
must produce an explicit unsupported-runtime diagnostic identifying the program
ID and URI. It must not compile Lua as IC10, silently skip the program, or
claim that the scenario passed. Lua execution, Lua diagnostics, and Lua test
discovery are intentionally deferred to P3.09.

The VS Code Test Explorer makes this boundary visible: pure `luaModule` cases
are labelled **Lua module**, full world-attached cases **Lua chip**, and worlds
containing both languages **IC10 + Lua**. Local Debug reports an explicit
unsupported Lua-chip/mixed-world result rather than starting the IC10 adapter;
remote StationeersLua debugging remains a separate live-game workflow.

## Test selector

```json
{
  "scenario": "./solar.icsim",
  "cases": [{ "name": "tracks", "focusProgram": "controller" }]
}
```

## Relative URIs and paths

All `uri` and scenario references are relative to the file that contains them.
Forward slashes are required in JSON on every platform. A relative reference is
resolved against the containing document's directory and normalized before it
is compared with another reference. `./controller.ic10` and
`sub/../controller.ic10` therefore identify the same file. Absolute filesystem
paths, drive-letter paths, and paths escaping the workspace are rejected by
workspace validation. URI fragments and query strings are not part of a
program identity.

References must point to files using the supported suffixes above. Tools reject
references to obsolete workspace filenames instead of rewriting them.

## Representative mixed-language fixture

This intentionally documents a future-compatible mixed scenario. It is a
workspace-format fixture, not an executable Lua test until P3.09 is complete:

```json
{
  "schemaVersion": 1,
  "scenario": "./mixed.icsim",
  "cases": [{
    "name": "reports unsupported Lua runtime explicitly",
    "focusProgram": "telemetry",
    "expectError": {
      "kind": "runtime",
      "messageContains": "unsupported runtime"
    }
  }]
}
```
