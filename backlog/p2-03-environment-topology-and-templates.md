# P2.03 — Environment topology and templates

## Goal

Make large multi-IC environments understandable at a glance and quick to build.

## Topology view

Add a topology mode alongside the existing inspector:

- device and IC housing nodes;
- ports and numbered connections;
- coloured cable, gas, liquid, and chute network edges;
- data/power role and direction where relevant;
- IC pin mappings;
- duplicate, incompatible, disconnected, or unreachable elements highlighted
  in place;
- filtering by network kind, IC, prefab, and validation state;
- keyboard-accessible navigation and selection;
- automatic layout with saved optional user positions.

The JSON scenario remains the source of truth. Layout metadata should be
separate or explicitly non-semantic so formatting a scenario does not change
simulation behaviour.

## Debug overlays

While paused or stepping:

- show current network channel values;
- highlight the most recent reader/writer;
- animate or pulse recent writes without continuous expensive animation;
- show device state badges and IC run states;
- open Variables/Watch/source from a node;
- filter the trace timeline to a selected node or edge.

## Scenario generation from source

Scan an IC10 program and propose:

- IC housing and program reference;
- aliases for `d0` through `d5`;
- prefab candidates from defines and hashes;
- required fields, slots, and batch device groups;
- unresolved devices that need user selection;
- likely network connections.

Generation must produce a preview and never overwrite an existing environment.

## Templates

Ship small, tested templates for:

- solar tracking;
- one- and two-door airlocks;
- temperature/pressure control;
- filtration;
- batch production;
- sorter/vending handshakes;
- multi-IC shared-network coordination.

Templates should include source, environment, and tests, and state which game
version they target.

## Environment editing quality

- Preserve native undo/redo as coherent user actions.
- Add copy/duplicate for devices and subnetworks with safe ID generation.
- Support import/export of selected topology fragments.
- Provide search across devices, networks, fields, and ICs.
- Maintain focus across re-renders.
- Meet keyboard, screen-reader, high-contrast, zoom, and reduced-motion
  requirements.

## Acceptance criteria

- [x] A multi-network environment can be created without editing raw JSON.
- [x] Topology and inspector selections stay synchronized.
- [x] Semantic scenario data is independent from visual layout metadata.
- [x] All validation errors can reveal the affected node, port, or edge.
- [x] Debug overlays are event-driven and do not poll the full state.
- [x] Source scanning produces a preview with unresolved assumptions called out.
- [x] Every template has an automated scenario test.
- [x] Keyboard-only and high-contrast workflows are covered by extension-host
      tests.

## Dependencies

- [P1.01](p1-01-environment-aware-intelligence.md) shared context/navigation.
- [P1.02](p1-02-scenario-tests-and-cli.md) tested templates.
- [P2.01](p2-01-reversible-debugging.md) trace overlays.

## Non-goals

- A general-purpose station/base designer.
- Persisting transient debug values into the scenario automatically.

## Decisions

- The existing inspector remains available; topology is an additional view.

## Implementation status

Static authoring and templates are complete:

- [x] Inspector and Topology tabs share a stable selection.
- [x] Device, IC, port, connection, network, and pin identities are modelled
      independently from the webview.
- [x] Cable, chute, gas, liquid, and IC-pin links are labelled and use
      theme-aware visual distinctions.
- [x] Search, network-kind, IC-only, prefab-ready, and validation-state filter
      primitives are available without indexing unbounded scenario values.
- [x] Keyboard spatial navigation, roving focus, screen-reader labels,
      high-contrast styling, reduced motion, and bounded zoom are covered.
- [x] Automatic layout is deterministic; dragged positions and viewport zoom
      are stored in a non-semantic `.ic10sim.layout.json` sidecar.
- [x] Device/subnetwork duplication and guarded fragment import/export perform
      deterministic collision-safe remapping.
- [x] Eight canonical, versioned templates are packaged, manifest-tested, and
      available through `IC10: Create Environment from Template` without
      overwriting existing files.
- [x] A single topology snapshot is requested on attach/reveal; bounded,
      coalesced DAP events update channels, fields, IC states, recent access,
      and runtime-verified behaviour badges without polling.
- [x] Node and edge actions open source, Variables, Watch, or a filtered trace.
- [x] Source scanning presents ranked candidates, evidence, reasons, and
      unresolved assumptions before an explicit selection/apply step.
- [x] Proposal application is one undoable edit and refuses to replace a
      populated environment or accept a prefab outside the ranked candidates.
- [x] A real VS Code extension-host smoke test covers activation, custom-editor
      reveal/focus, one-edit undo/redo, and a high-contrast theme.
- [x] The CDP extension-host harness dispatches Tab/Arrow/Enter/Escape into the
      real topology webview and asserts focus and selection synchronization.

