# Changelog

All notable changes to Stationeers IC10 Toolkit are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added scenario-aware test-editor suggestions, inline target/value checks,
  parameter guidance, full CLI validation, per-case run controls, and visible
  last-run status.
- Added a default visual `*.ic10test.json` editor with guarded case, state,
  assertion, timeline, parameter, expected-error, and snapshot authoring,
  plus creation, save/undo/redo, scenario browsing, and direct JSON access.
- Extended device-aware LogicType completion and diagnostics to direct and
  slotted loads/stores, validity checks, direct-ID access, and every batch
  load/store form, respecting both field access and slot access metadata.
- Added searchable device and item PrefabHash completions after `define NAME`,
  including display names, prefab names, and signed hashes.
- Added prefab-aware batch LogicType completion and diagnostics even when no
  simulation context is selected, plus numbered device-connection hover help.
- Added complete language, evaluator, and simulator handling for `nan`, `pinf`,
  and `ninf`, and named Stationpedia regression fixtures for `sgn`, `clamp`,
  `rol`, and `ror`.
- Added conditional and hit-count breakpoints, expression logpoints, symbolic
  label breakpoints, and register/stack/device/slot/memory/network data
  breakpoints using the shared scenario-test evaluator.
- Added categorized exception filters and `exceptionInfo`, sparse inline
  values, changed-value presentation, run to cursor, deterministic restart,
  and explicit preserve-state or reset-state hot reload.
- Expanded Debug Console expressions with aliases, defines, world objects,
  arithmetic and boolean operators, runtime context, IEEE-754 helpers, and
  stop-to-stop `changed(...)` evaluation.
- Added versioned `*.ic10test.json` scenario tests, native Test Explorer
  discovery/run/debug/navigation, saved-file invalidation, and optional
  affected-test re-runs.
- Added the protocol-neutral `ic10` runner and CLI with deterministic bounded
  simulation, shared debugger expressions, parameter tables, timelines,
  tolerances, expected errors, snapshots, and human/JSON/JUnit output.
- Added explicit environment-aware IC10 intelligence with a visible
  environment/housing selector, pin- and alias-aware completion, diagnostics,
  hover, inlay mappings, navigation, source CodeLens usages, and precise
  environment quick fixes.
- Added multi-root and remote-safe scenario URI indexing with live
  create/change/delete/rename invalidation and a document-only fallback for
  missing, ambiguous, deselected, or version-mismatched contexts.
- Added deterministic `none`, `readable`, and `compact` deployment builds with
  safe relative-branch rewriting, source maps, reproducibility metadata,
  official-limit reports, compact preview diffs, file/open/clipboard commands,
  a protocol-neutral API, and the `ic10 build` CLI with file, sidecar, and
  write-free stdout modes.
- Added a generated, CI-verified simulator conformance matrix and user-facing
  compatibility report with evidence versions, golden fixture IDs, known
  deviations, and active-device dependencies.
- Added deterministic IEEE-754 storage and multi-IC shared-world ordering
  regressions plus a minimal real-game capture workflow for ambiguous
  instruction behaviour.
- Added a non-blocking Debug Console warning when a simulation scenario targets
  a Stationeers version newer than the bundled data.
- Added generated-signature operand validation and conservative control-flow
  and value diagnostics for unused symbols, dead code, branches, loops,
  registers, stack bounds, addresses, division by zero, and nested calls.
- Added references, document highlights, workspace symbols, identity-safe
  rename, semantic tokens, folding ranges, inlay hints, safe quick fixes, and
  conservative document formatting.
- Added configurable `off`, `hint`, and `warning` levels for unused/dead-code
  diagnostics, with `Unnecessary` rendering and `_` suppression.
- Added a live program-budget status item using the official generated line and
  per-tick operation limits.

### Changed

- Widened simulation and scenario-test forms to use the available inspector
  space, with aligned section controls and wrapped long test names.
- Build artefacts now default to a `build/` directory beside each source
  program instead of a workspace-root `.ic10/build/` directory.

### Fixed

- Fixed format-on-save duplicating full-line comments on every save.
- Replaced the mispositioned Chromium datalist used for simulation program
  paths with an aligned native selector and adjacent Open/Browse actions.
- Fixed `clamp` panicking for documented NaN inputs by applying the
  Stationpedia `min(max(value, min), max)` semantics explicitly.

## [0.2.0] - 2026-07-24

### Added

- Added a visual, versioned IC10 simulation environment for networks, devices,
  connections, pins, fields, slots, registers, and stack state.
- Added a native multi-IC debug adapter with one thread per housing, source
  breakpoints, editable debugger scopes, watches, instruction stepping, and
  coordinated world-tick stepping.
- Added deterministic shared cable-network channels and a dense **IC10 State**
  register/stack view.
- Added context-aware simulation editing with data-cable pin filtering,
  structured register/stack rows, Stationpedia help and mode choices, IC10
  program browsing, and rename-safe program references.
- Added a searchable thumbnail device catalogue and prefab-aware validation
  for logic fields, slots, and addressable device memory.
- Added cable-purpose and network-media connection filtering, network-kind
  illustrations, and text-only slot item presets backed by generated item
  metadata.
- Added editable runtime inventory slots and device memory to debugger scopes,
  including slot and memory watch expressions.
- Added a **Save stack** action that captures a setup IC's non-zero runtime
  stack cells as the housing's sparse initial stack.
- Added numeric item Class and SortingClass initialization from Stationpedia
  enum data.
- Added inline validation for duplicate device, network, and Reference IDs,
  and excluded disabled or unconfigured ICs from debug launch targets.
- Added direct debugging from the selected environment housing and F5 lookup
  from the active IC10 program, including focused startup for multi-IC worlds.
- Added symbolic debugger hover evaluation for device/connection references
  and LogicType names such as `db:1` and `Channel0`.
- Fixed manual instruction stepping after `yield` so the following jump or
  instruction no longer reports a waiting-state exception.
- Fixed debugger network scopes so chute, gas, and liquid networks do not
  expose cable-only channels.
- Fixed multi-IC debugger scopes using variable references larger than
  JavaScript can represent exactly, which caused invalid-reference, unknown
  thread, and unknown device errors in VS Code.
- Added an **IC10: Remove All Comments** editor command that deletes
  comment-only lines and updates literal numeric relative branch offsets,
  removing branches that become redundant zero-offset jumps.
- Added standard `F2` rename support for labels, defines, and aliases, including
  document-wide reference updates and symbol-name collision checks.
- Documented native single-line and multi-line comment toggling with the
  standard editor shortcut.

## [0.1.2] - 2026-07-23

### Changed

- Renamed the Marketplace display name to **Stationeers IC10 Toolkit** to
  distinguish it from an existing extension. The extension ID remains
  `shaneyu.stationeers`.

## [0.1.1] - 2026-07-23

### Fixed

- Corrected automated publication of the platform-specific VSIX packages.

## [0.1.0] - 2026-07-23

### Added

- Native Rust language server bundled with the extension.
- IC10 syntax highlighting.
- Context-aware completion for instructions, operands, registers, devices,
  constants, enums, labels, prefab hashes, and literal macros.
- Hover documentation for instructions, symbols, devices, hashes, and packed
  display strings.
- Signature help, go to definition, and document symbols.
- Diagnostics for syntax, operands, symbols, labels, literal macros, and the
  IC10 program line limit.
- Generated Stationpedia reference data and selected hover thumbnails bundled
  for offline use.

[Unreleased]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ShaneYu/stationeers-vscode-extension/releases/tag/v0.1.0
