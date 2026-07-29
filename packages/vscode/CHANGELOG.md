# Changelog

All notable changes to Stationeers Toolkit are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Added a VitePress documentation site for Stationeers Toolkit covering IC10
  editing, simulation, debugging, scenario testing, deployment builds, the
  Stationeers Toolkit mod, and optional StationeersLua integration.
- Added GitHub Pages CI/CD that builds documentation on `main` and
  `experimental`, while deploying automatically from `main` and allowing an
  explicit manual deployment from `experimental`.
- Replaced inherited external-author documentation references with
  project-owned branding and neutral integration guidance.

## [0.4.2] - 2026-07-29

- Fixed the Stationeers Toolkit workshop mod not appearing in LaunchPad or
  loading in-game by publishing its workshop handle and synchronizing the
  runtime mod version with release metadata; release checks now catch version
  drift in the runtime registration.

## [0.4.0] - 2026-07-29

- Fixed the mixed IC/Lua example's nested `testing/` fixtures being rejected by
  the visual editors; parent-relative program and module paths now resolve from
  the owning test or simulation file, and canonical `programId` assignments
  are recognized by environment validation.
- Fixed the simulation editor's IC debug selector to resolve canonical IC10
  programs through `programId` instead of requiring legacy inline program paths.
- Fixed the simulation editor's Debug button being replaced by the generic F5
  selection flow when the environment editor was active.
- Fixed nested `testing/` fixture validation on Linux CI when paths use Windows
  separators.
- Marked VSIX packages created from `*-prerelease` GitHub tags as prerelease so
  Visual Studio Marketplace publishing accepts them.
- Renamed the product and mod to **Stationeers Toolkit**, including the VS Code
  extension display name, project/solution names, assemblies, documentation,
  and Stationeers mod metadata.
- Changed the mod identifier to `com.shaneyu.stationeerstoolkit`, which gives
  the BepInEx config the filename `com.shaneyu.stationeerstoolkit.cfg`.
- Set Bridge, RemoteNetwork, Relay, and `AllowRemoteWrites` enabled by default.
- Added friendly Remote Network item language entries in
  `StationeersToolkit_EN.xml` and packaged the new About preview and thumbnail
  assets with Debug deployments.
- Added friendly StationeersLua integration guidance and links to the VS Code
  Marketplace and Open VSX listing.

- Add deterministic, world-free stateful Lua mock services for persisted state,
  lifecycle, virtual time, and seeded random replay. Unsupported extended host
  capabilities remain explicit.
- Added authoritative multiplayer RemoteNetwork relay support with authenticated
  player identity, bounded request/response queues, stale-write conflict safety,
  revocation/disable handling, and dedicated-server suppression of the public
  IDE bridge listener.
- Added live validation coverage for multiplayer authority, incremental network
  discovery, single-player IC10 read/write, wireless Lua read/write, and direct
  editor Lua synchronization.
- Recorded exact-editor Lua synchronization in dedicated-server sessions as an
  accepted upstream StationeersLua limitation pending the author's fix; the
  wireless Lua workflow remains supported and verified.

### Added

- Added language-aware Test Explorer discovery and source locations for IC10,
  pure Lua modules, full Lua-chip, and mixed IC10/Lua scenarios. Local Lua-chip
  debugging now reports an explicit unsupported path distinct from remote
  StationeersLua debugging, with three packaged Lua scenario templates.

- Added the P3-09A pure-module Lua test runner: pinned `mlua` 0.12.0 with
  Lua 5.2 and vendored builds, explicit `luaModule` test selection,
  deterministic workspace `require()`, structured output/source failures,
  bounded sandbox execution, and explicit unsupported Stationeers host APIs.
- Added the mixed IC10/Lua vending example with shared cable-channel
  addressing, shared Lua supplier logic, per-program tests, and a joint
  simulation. World-attached Lua programs now support the same sandboxed
  source-relative `require()` resolver as Lua module tests.
- Added optional `stationeersToolkit.lua.libraryPaths` workspace settings for additional Lua
  module directories across Test Explorer, headless CLI runs, and simulation
  debugging. The CLI also accepts repeated `--lua-library DIR` options.
- Added direct StationeersLua REST integration for best-effort Lua Pull,
  Compare, and Push, with authoritative ReferenceId correlation and explorer
  signal state showing whether each globally discovered Lua chip is currently
  accessible through the active editor or Wireless Development Board scope.
- Versioned read-only local bridge contract and shared golden fixtures.

- Added language-neutral `*.stationeerssim.json`,
  `*.stationeerstest.json`, and `*.stationeerssim.layout.json` workspace
  formats with explicit IC10/Lua program metadata, mixed-language fixtures,
  and legacy IC10 suffix compatibility.
- Added generated Stationeers Lua annotations, the required `sumneko.lua`
  dependency, and an explicit preview/apply/restore Lua 5.2 configuration
  command.

### Changed

- Added the P3-09B VM-neutral scheduler boundary while preserving IC10
  stepping, checkpoints, traces, and replay. Mixed or Lua-only simulated
  worlds now fail closed instead of silently skipping attached Lua programs.
- Updated CLI, Test Explorer, editors, launch configuration, topology layouts,
  templates, packaging, and documentation to generate canonical neutral names
  while retaining legacy scenario/test/layout support.
- Packaged templates now emit canonical workspace files only; legacy fixtures
  remain available in the repository for compatibility validation.

### Fixed

- Fixed console-hosted Lua Scripted Screens being shown as inaccessible by
  correlating their bridge-reported motherboard housing identity to exactly
  one StationeersLua chip without using display names.

## [0.3.1] - 2026-07-27

### Added

- Added **Ctrl + mouse wheel** zooming towards cursor position in the Topology view with persistent layout saving.
- Improved **Auto layout** with spacious grid bounds (460px × 270px) and connected-neighbor barycenter sorting to minimize line crossings and avoid squished cards.
- Added **automatic viewport fit zoom** on initial view load and layout reset so all topology nodes fit into the viewport without clipping.
- Added **smart network label placement** that positions line labels in clear open space to prevent text from rendering under node cards.
- Added **smart SVG cable curve routing** with obstacle avoidance so network lines automatically curve around unattached node cards instead of passing under them.

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

- Renamed the Marketplace display name to **Stationeers Toolkit** to
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

[Unreleased]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.4.2...HEAD
[0.4.2]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/ShaneYu/stationeers-vscode-extension/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/ShaneYu/stationeers-vscode-extension/releases/tag/v0.1.0
