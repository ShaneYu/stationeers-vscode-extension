# Neutral workspace formats

P3.01 establishes a language-neutral workspace contract for simulation and
scenario files. The canonical suffixes are:

| Purpose | Canonical filename | Legacy filename (readable) |
| --- | --- | --- |
| Simulation environment | `name.stationeerssim.json` | `name.ic10sim.json` |
| Scenario test | `name.stationeerstest.json` | `name.ic10test.json` |
| Simulation layout sidecar | `name.stationeerssim.layout.json` | `name.ic10sim.layout.json` |

Canonical names are the defaults for generated files, templates, examples, and
new documentation. Legacy names remain supported for existing workspaces and
fixtures. Opening or saving a legacy file does not rename it, rewrite it, or
create a second file. Migration is an explicit user action so source control
diffs never hide a format change.

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

## Before and after

Legacy IC10 workspace:

```json
{
  "scenario": "./solar.ic10sim.json",
  "cases": [{ "name": "tracks", "focusIc": "controller" }]
}
```

Canonical neutral workspace:

```json
{
  "scenario": "./solar.stationeerssim.json",
  "cases": [{ "name": "tracks", "focusProgram": "controller" }]
}
```

The migration is a filename and metadata change, not an automatic conversion.
Keep the old file until the new file has been reviewed and tested, then make
the rename and content change in one deliberate source-control operation.

## Relative URIs and paths

All `uri` and scenario references are relative to the file that contains them.
Forward slashes are required in JSON on every platform. A relative reference is
resolved against the containing document's directory and normalized before it
is compared with another reference. `./controller.ic10` and
`sub/../controller.ic10` therefore identify the same file. Absolute filesystem
paths, drive-letter paths, and paths escaping the workspace are rejected by
workspace validation. URI fragments and query strings are not part of a
program identity.

When a legacy file references another legacy file, the reference remains valid;
readability is based on the target's actual suffix and not on the suffix of the
parent document. Tools must preserve the original spelling when writing a file.

## Representative mixed-language fixture

This intentionally documents a future-compatible mixed scenario. It is a
workspace-format fixture, not an executable Lua test until P3.09 is complete:

```json
{
  "schemaVersion": 1,
  "scenario": "./mixed.stationeerssim.json",
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
