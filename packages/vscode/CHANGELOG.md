# Changelog

All notable changes to Stationeers IC10 Toolkit are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Added **Ctrl + mouse wheel** zooming towards cursor position in the Topology view with persistent layout saving.

### Fixed

- Added `rust-analyzer` to the `rust-toolchain.toml` file to resolve toolchain version compatibility warnings in Antigravity IDE and VS Code.

## [0.3.0] - 2026-07-26

### Added

- Added collapsible section headers for Registers, Stack, and History & analysis in the IC10 State view with persistent user toggle state.
- Added event-driven topology debug overlays with one attach/reveal snapshot,
  bounded coalesced trace events, live channels, access and IC states,
  runtime-verified behaviour badges, and source/debug/trace actions.
- Added guarded environment proposals from IC10 source with visible ranked
  prefab candidates, evidence, unresolved assumptions, explicit selection,
  atomic apply, and populated-environment overwrite protection.
- Added real VS Code extension-host smoke harnesses for activation,
  custom-editor focus, coherent undo/redo, high-contrast operation, and
  CDP-driven keyboard-only topology navigation.
- Added a synchronized, accessible topology view with deterministic sidecar
  layout, guarded duplication, and topology fragment import/export.
- Packaged eight tested simulation templates and added a guarded
  create-from-template command.
- Added bounded checkpoint-and-replay debugging with standard Step Back and
  Reverse Continue, previous/next history navigation, value histories, state
  diffs, redacted trace export/import, deterministic coverage, and operation
  profiles in the IC10 State view.

### Changed

- Defaulted `enableHistory` to `true` across all debug launch paths so reversible debugging and history timeline views are enabled by default.
- Defaulted IC10 State view collapsible sections (Registers, Stack, History) to collapsed on initial view load.
- Reordered Scenario Test editor sidebar items to place status indicators on the left and play run controls on the right.
- Simulation inventory slots now use compact collapsible cards: configured
  slots open automatically while empty slots remain collapsed.
- Scenario-test execution now lives in the wider case sidebar, with Run All,
  per-case play controls, queued/running feedback, and pass/fail indicators.
- The simulation editor toolbar now includes a compact `{}` button for opening
  the underlying JSON source.
- Deployment builds now remove leading spaces and tabs from every generated
  line at all optimisation levels for more compact in-game editing.
- Widened simulation and scenario-test forms to use the available inspector
  space, with aligned section controls and wrapped long test names.
- Build artefacts now default to a `build/` directory beside each source
  program instead of a workspace-root `.ic10/build/` directory.

### Fixed

- Fixed CLI executable resolution in extension testing to prefer development builds over stale staged binaries when running in development mode or when freshly compiled.
- Fixed scripted driver schedule serialization errors in `ic10-runner` when executing scenario tests with deterministic drivers.
- Fixed register-before-write hints treating `alias name r0` declarations as
  runtime reads of the aliased register.
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

[Unreleased]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ShaneYu/stationeers-vscode-extension/releases/tag/v0.1.0
