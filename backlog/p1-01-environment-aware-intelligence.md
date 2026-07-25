# P1.01 — Environment-aware language intelligence

## Goal

Use a selected `*.ic10sim.json` environment to make completion, diagnostics,
hover, and navigation understand the actual devices and networks attached to
the program being edited.

## User experience

When a program is referenced by one or more simulation environments, VS Code
should show the active environment and IC housing in the status bar. Users can
switch context when the same program is used by multiple housings.

Within that context:

- `d0` through `d5` resolve to configured devices;
- `db` resolves to the housing/device running the program;
- aliases inherit their resolved device or register type;
- completions include only logic fields valid for that prefab and operation;
- read/write access, slots, modes, memory size, and connection suffixes are
  validated;
- batch operations understand devices on the housing's data network;
- hover shows the friendly name, prefab, image, network, connection, and
  relevant initial state;
- inlay hints can display mappings such as `d0 -> Outside Sensor`;
- quick fixes open the environment editor at a missing or invalid device.

## Architecture

Keep scenario loading and validation protocol-neutral:

1. Add an analysis context model shared by the LSP and simulator.
2. Resolve scenario paths relative to the scenario file.
3. Index workspace scenarios by canonical program URI and IC stable ID.
4. Pass context selection from the VS Code client to the LSP using
   configuration/custom notifications.
5. Cache parsed scenarios by URI/version and invalidate them through file
   watches.
6. Continue providing document-only intelligence when no environment exists.

The LSP must never silently choose between ambiguous contexts. It may remember a
per-workspace selection, but should identify it visibly.

## Diagnostics

Add diagnostics for:

- unassigned device pins used by the program;
- device references that are not reachable on the housing data cable;
- unsupported or inaccessible logic fields and slot fields;
- invalid slots, modes, connections, channels, and memory addresses;
- batch operations that match no devices;
- writes to read-only values and reads from write-only values;
- scenario prefab hashes that disagree with source defines;
- use of a field that exists in another game-data version but not the bundled
  version.

Diagnostics derived from a simulation context must identify that context in
their message and disappear cleanly when the context is deselected.

## Environment/source navigation

- From `d0` or its alias, open the configured device in the environment editor.
- From a scenario IC program path, open the source.
- From a diagnostic, reveal the relevant device, connection, field, or slot.
- Add CodeLens or references on a scenario program showing its housing usages.

## Acceptance criteria

- [ ] Every open IC10 document can report zero, one, or multiple scenario
      contexts.
- [ ] Multiple contexts require an explicit visible selection.
- [ ] Completion and validation follow aliases to their configured device.
- [ ] Read/write and slot diagnostics agree with simulator validation.
- [ ] Batch diagnostics account for network reachability and prefab/name hashes.
- [ ] No-environment editing retains all current language features.
- [ ] Scenario changes refresh intelligence without restarting the server.
- [ ] Multi-root, renamed-file, and remote-workspace tests are included.
- [ ] Quick fixes navigate to the precise environment object.

## Dependencies

- [P0.01](p0-01-language-correctness.md) resolved symbols and typed operands.
- [P0.02](p0-02-simulator-conformance.md) versioned data and fidelity status.

## Non-goals

- Treating initial scenario values as compile-time constants unless the user
  explicitly opts into such analysis.
- Guessing which environment is authoritative.

## Decisions

- Environment-aware diagnostics augment rather than replace document-only
  analysis.
- Context selection is explicit when a program has multiple housing usages.
