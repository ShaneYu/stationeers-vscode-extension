![GitHub Release](https://img.shields.io/github/v/release/shaneyu/stationeers-vscode-extension)
![VS Code Version](https://vsmarketplacebadges.dev/downloads/shaneyu.stationeers.webp?label=VS%20Code%20Downloads)
![Open VSX Downloads](https://img.shields.io/open-vsx/dt/shaneyu/stationeers?label=Open-VSX%20Downloads)

# Stationeers IC10 Toolkit

Fast, offline IC10 language support for Stationeers, powered by a native Rust
language server.

The language server and generated reference data are bundled with the
extension. You do not need Python, a Stationeers installation, or a separately
installed language server.

> This is an independent community project. It is not affiliated with,
> endorsed by, or sponsored by RocketWerkz.

## Quick start

1. Install **Stationeers IC10 Toolkit** from your editor's Extensions view.
2. Open or create a file ending in `.ic10`.
3. Start typing an instruction or hover an existing symbol.

```ic10
define Solar HASH("StructureSolarPanel")
alias sensor d0

start:
  l r0 sensor Horizontal
  yield
  j start
```

Language features activate automatically for `.ic10` files.

## Features

- Syntax highlighting for instructions, labels, registers, devices, macros,
  constants, enum values, numbers, and comments.
- Context-aware completion for instructions, operands, registers, device pins,
  constants, enums, labels, prefab hashes, and `HASH`/`STR` literal macros.
  On `define NAME …`, search devices and items by display name, prefab name, or
  signed PrefabHash and insert the selected numeric hash.
- Hover help for instructions, registers (`r0-r15`, `sp`, and `ra`), device
  references (`d0-d5`, `db`, and numbered connections such as `db:1`),
  constants including `nan`, `pinf`, and `ninf`, enums, symbols, reagent hashes,
  computed `HASH("...")` values, packed `STR("...")` display strings, prefab
  names, and numeric prefab hashes.
- Signature help generated from IC10 command syntax.
- Go to definition, find references, document highlights, workspace symbols,
  and identity-safe rename for labels, defines, and aliases.
- Rename symbol support for labels, defines, and aliases using **Rename
  Symbol** (`F2`), with document-wide reference updates and collision
  validation.
- Typed operand diagnostics plus conservative checks for unused declarations,
  unreachable code, constant branches, tight loops, register use, stack
  bounds, addresses, division by zero, return-address clobbering, and the
  official program limits.
- Device-aware LogicType completion and diagnostics across direct, slotted,
  validity-check, direct-ID, and every batch load/store operator. Selected
  simulation pins restrict `l`, `s`, `ls`, `ss`, `bdnvl`, `bdnvs`, `ld`, and
  `sd`; a known prefab selector such as `define LED 1944485013` also restricts
  every `lb*`/`sb*` form without requiring a simulation file.
- Semantic highlighting, label-delimited folding, resolved-value inlay hints,
  safe quick fixes, document formatting, and document/workspace symbols.
- A status-bar program budget showing physical lines and, where it can be
  estimated safely, operations executed per game tick.
- Deterministic deployment builds with `none`, `readable`, and `compact`
  levels, safe relative-branch rewriting, preview diffs, clipboard output,
  source maps, reproducibility metadata, and optimisation reports.
- Visual `*.ic10sim.json` environments for devices, labeller names, numbered
  connections, networks, pins, fields, slots, registers, and stack values.
- A synchronized Topology tab with labelled network and pin links,
  deterministic non-semantic layout sidecars, search and validation filters,
  keyboard navigation, safe duplication, and fragment import/export.
- Eight packaged, scenario-tested starting templates available through
  **IC10: Create Environment from Template**, with destination preview and
  overwrite protection.
- Native single- and multi-IC debugging with source breakpoints, one thread per
  housing, editable variables, watches, and deterministic world ticks.
- Native line commenting with **Toggle Line Comment** (`Ctrl+/` on Windows and
  Linux, `Cmd+/` on macOS), including multi-line selections.
- Removal of every full-line and inline comment in the current file.

The parser remains useful while a line is incomplete or invalid, making the
extension suitable for normal incremental editing.

## Language tooling in action

### Understand IC10 in place

Hover instructions, registers, aliases, hashes, and devices without leaving the
editor. Completion and signature help use the same bundled Stationpedia data.

![Offline IC10 hover, hash resolution, completion, and signature help.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/language-intelligence.gif)

### Rename symbols safely

![Renaming IC10 labels, defines, and aliases updates every reference.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/rename-symbols.gif)

### Remove comments without breaking relative jumps

![Removing all IC10 comments while preserving relative jump destinations.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/remove-comments.gif)

## Simulate and debug IC10

> **Preview:** the simulator is designed for deterministic IC10 program and
> network debugging. It includes a narrow test-oriented vending-machine,
> digital-chute-valve, and chute-outlet behaviour pack. Other active game
> physics such as recipes and atmospherics are not yet advanced automatically.

1. Open an `.ic10` program.
2. Run **IC10: Create Simulation Environment**.
3. Add networks and devices in the visual editor.
4. Configure each IC housing's program, data-cable pins, connections,
   registers, and initial stack.
5. Select an IC housing and use the environment editor's **Debug** button or
   press F5. F5 from an `.ic10` file locates the simulation that references
   that program.

The debugger supports conditional and hit-count source breakpoints, logpoints
with `{expression}` interpolation, symbolic label breakpoints, and data
breakpoints on registers, stack cells, device fields and slots, device memory,
and cable-network channels. Conditions, logpoints, watches, hover evaluation,
scenario-test assertions, and the Debug Console all use the same expression
grammar. It includes arithmetic, comparisons, boolean operators, aliases,
defines, world objects, `tick`, `line`, `runState`, `operationsThisTick`,
`abs`, `isnan`, `isfinite`, and `changed`.

Runtime exceptions are grouped into instruction/operand, missing-device,
access, address, compile, and explicit-`hcf` categories. Use native **Restart**
to restore the original launch/test state. While paused, run **IC10: Hot Reload
Paused Simulation** and explicitly choose whether to preserve CPU/world state
or reset it. The adapter rejects a preserve-state reload if the new sources do
not compile or no longer contain the current instruction.

Run to Cursor, instruction stepping, and **IC10: Step World Tick** remain
available. Single-thread continue is supported for specialist investigation,
but emits a warning because it intentionally departs from coordinated world
scheduling. Changed debugger values are marked after each stop, and optional
inline values remain sparse: current-line registers/aliases plus the tick and
operation budget.

### Configure a shared environment

![Visual IC10 simulation environment with multiple networks, devices, and IC programs.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/editor.png)

The environment editor filters `d0`–`d5` targets to devices on the housing's
data cable, offers named choices for known device modes, and provides inline
metadata help. IC10 program paths can be selected from workspace files or
browsed externally, and VS Code file/folder renames update references.

The searchable device catalogue matches display names, prefab names, and
PrefabHash values while showing each result's thumbnail and identity.

### Use environment-aware editing

Every `*.ic10sim.json` file in each workspace folder is indexed against the
program path and stable IC housing ID it declares. When an IC10 editor is
active, the **IC10 environment** status item shows one of three states:

- **no environment** — all normal document-only language features remain
  available;
- the active environment and housing — device-aware completion, diagnostics,
  hover, inlay hints, and navigation are enabled;
- **choose environment** — the program is used by multiple housings and the
  toolkit will not guess which one is authoritative.

Click the status item to switch context or return to document-only analysis.
Aliases such as `alias sensor d0` inherit the selected pin's prefab and access
rules. Environment diagnostics include the selected context in their message;
their quick fix opens the simulation editor at the relevant housing, device,
field, slot, connection, or memory address.

Scenario changes, creation, deletion, and renames are reflected without
restarting the language server. Program paths are resolved relative to the
scenario URI through VS Code's workspace filesystem, including multi-root and
Remote Development workspaces. A scenario whose `gameVersion` differs from the
bundled official data is identified explicitly; the toolkit does not invent
cross-version field support.

![Filtering the visual device catalogue by name, prefab name, or PrefabHash.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/add-device-filter.png)

Network media and cable purpose filter numbered connections to compatible
choices. Cable data and power networks can be modeled separately, while cable
channels can be initialized directly for deterministic tests.

![Configuring a named data-cable network and its shared channels.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/networks.png)

Slot item presets provide a separate text-only search over bundled item
metadata without adding item thumbnails to the package. Presets initialize the
numeric slot Class and SortingClass values as well as occupant identity,
quantity, occupied state, damage, and known maximum quantity. Device memory is
only shown for prefabs that expose addressable memory.

### Debug multiple ICs together

Every IC housing runs as a thread in one shared debug session. Breakpoints can
be placed in all participating programs; hitting one pauses the complete world.
The normal Step action executes one instruction on the selected IC. Use
**IC10: Step World Tick** to run all eligible ICs for one 0.5-second game tick.

![Two IC10 programs paused and inspected in one shared debug session.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/debugging.png)

### Inspect and edit runtime state

Registers, stack cells, device fields, inventory slots, addressable device
memory, and cable-network channels are editable while paused. Watch expressions
can inspect the same shared world state.

![Inspecting device slots and shared-network values with debugger watches.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/debugging-watch.png)

The **IC10 State** debug view provides a compact register and 512-cell stack
editor alongside the standard Variables and Watch views. Its **Save stack**
action can capture the result of a one-time setup program as that housing's
sparse initial stack before switching to the operational program.

![Editing registers and stack cells in the dedicated IC10 State debugger view.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/debugger-ic-state.png)

See the
[simulation guide](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/docs/simulator.md)
for the scenario model, watch expressions, and current device-behaviour
fidelity.

## Test scenarios

Put repeatable cases in `*.ic10test.json` beside a reusable simulation
environment. Test Explorer discovers file, case, and parameter levels and can
run them through the bundled headless runner or debug them in the existing
multi-IC debugger. Debug sessions apply scheduled stimuli and pause on
assertion failure.

Open a test file to use the visual editor for cases, initial state, assertions,
timeline events, parameter sets, expected errors, and snapshots. It prevents
invalid form state from overwriting valid JSON and retains normal save,
undo/redo, and source-control behaviour. Scenario-aware suggestions cover
registers, device/slot/memory fields, networks, expressions, and parameter
placeholders. **Validate** checks the complete fixture and **Run case** records
the latest pass/fail result. **Open JSON** provides the canonical source view
for advanced editing.

The bundled `ic10` command also emits human, JSON, or JUnit output for CI. The
schema, CLI options, migration policy, and examples are in the
[scenario testing guide](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/docs/scenario-testing.md).

## Commands

Open the Command Palette and run:

- **IC10: Remove All Comments** — removes every comment from the current IC10
  file while preserving hash characters inside quoted `HASH`/`STR` literals.
  Comment-only and blank lines are deleted, and literal numeric
  offsets in relative `br...` and `jr` instructions are updated to preserve
  their destinations. Relative branches that become a redundant zero-offset
  jump are removed. Dynamic offsets stored in registers, aliases, or defines
  cannot be safely updated cause the edit to be refused. The command is also
  available from the editor's context menu.
- **IC10: Build for Game** — validates and writes deployable code plus source
  map, metadata, and report sidecars under `build/` beside the source program
  by default. Compact builds show a preview diff.
- **IC10: Copy Deployable Code** — runs the identical build and copies only
  deployable code without writing an artefact.
- **IC10: Open Built Code** — builds and opens the generated program.
- **IC10: Restart Language Server** — stops and restarts the language server.
- **IC10: Create Simulation Environment** — creates a source-controlled
  `*.ic10sim.json` environment and opens its visual editor.
- In an environment's **Topology** view, **Propose from source** previews
  ranked device/prefab candidates, evidence, inferred networks, and unresolved
  assumptions before an explicit, non-overwriting apply.
- **IC10: Create Scenario Test** — creates a source-controlled
  `*.ic10test.json` fixture and opens its guarded visual editor.
- **IC10: Select Simulation Context** — chooses the environment and stable IC
  housing used for the active program's language intelligence, or returns to
  document-only analysis.
- **IC10: Step World Tick** — advances every eligible IC in the active
  simulation by one coordinated tick.

While debugging, the environment topology receives one initial state snapshot
and bounded event updates rather than polling. It shows live channel values,
recent readers/writers, IC run states, and versioned active-device behaviour
badges; node and edge actions can open source or focus Variables, Watch, and a
filtered trace.

To comment or uncomment the current line or a multi-line selection, run
**Toggle Line Comment** or press `Ctrl+/` (`Cmd+/` on macOS).

To rename a label, define, or alias, place the cursor on its declaration or any
usage and press `F2`. For labels, the trailing `:` is excluded from the rename
and remains on the declaration. The rename is rejected if the new name is
invalid or already belongs to another define, alias, or label.

## Settings

| Setting | Default | Purpose |
| --- | --- | --- |
| `ic10.server.path` | Empty | Absolute path to a custom `ic10-lsp` executable. Leave empty to use the bundled server. |
| `ic10.debugAdapter.path` | Empty | Absolute path to a custom `ic10-dap` executable. Leave empty to use the bundled debug adapter. |
| `ic10.debug.history.enabled` | `false` | Records bounded checkpoint-and-replay history for Step Back and Reverse Continue. |
| `ic10.debug.history.events` | `20000` | Maximum reversible events retained in memory. |
| `ic10.debug.history.checkpointInterval` | `10000` | Events between mutable-state checkpoints. |
| `ic10.debug.history.memoryMiB` | `64` | Approximate memory ceiling for retained history. |
| `ic10.cli.path` | Empty | Absolute path to a custom `ic10` command-line executable. Leave empty to use the bundled CLI. |
| `ic10.testing.rerunOnSave` | `false` | Automatically re-run scenario tests affected by a saved program or scenario. |
| `ic10.diagnostics.unused` | `hint` | Shows unused declarations and unreachable code as `off`, subtle `hint`, or `warning` diagnostics. Prefix a deliberately unused symbol with `_` to suppress its hint. |
| `ic10.build.optimization` | `readable` | Selects exact-source, comment-free, or safe compact deployment output. |
| `ic10.build.outputDirectory` | `build` | Directory for code and JSON sidecars, relative to the source program's folder unless absolute. |
| `ic10.build.gameVersion` | Empty | Optional exact Stationeers version; a mismatch with bundled official data fails the build. |
| `ic10.trace.server` | `off` | Logs LSP communication at `messages` or `verbose` level. |

Settings can be changed through **Preferences: Open Settings (UI)** by
searching for `Stationeers IC10 Toolkit`.

## Supported platforms

The extension requires Visual Studio Code 1.107 or newer.

Release packages are built separately for:

- Windows x64 and ARM64
- Linux x64 and ARM64
- macOS Intel and Apple silicon

The extension runs in the workspace extension host, including compatible
Remote Development environments. A custom server can be selected with
`ic10.server.path` when a platform package is unavailable.

## Troubleshooting

### The language server did not start

1. Open **View: Output**.
2. Select **Stationeers IC10 Toolkit** from the channel list.
3. Run **IC10: Restart Language Server**.
4. Check that you installed the package matching the host that runs the
   extension. For remote workspaces, this is normally the remote host.

If you configured `ic10.server.path`, confirm that it points to an executable
for the current operating system.

### Language features are not active

Confirm that the file ends in `.ic10` and that the language mode shown in the
status bar is **IC10**.

### Collecting a protocol trace

Set `ic10.trace.server` to `messages` or `verbose`, reproduce the problem, and
include the relevant output in a bug report. Review the trace before sharing it
because it can contain source text.

## Privacy

The extension runs locally. It does not include telemetry and does not send
source code or Stationeers data to an external service.

## Support and contributing

- [Report a bug or request a feature](https://github.com/ShaneYu/stationeers-vscode-extension/issues)
- [Support policy](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/packages/vscode/SUPPORT.md)
- [Contributing guide](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/CONTRIBUTING.md)
- [Architecture and roadmap](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/docs/architecture.md)

## License and attribution

Project source code is available under the
[MIT License](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/LICENSE).
Stationeers names, reference material, and images remain the property of
RocketWerkz and its licensors; see the
[third-party notices](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/THIRD_PARTY_NOTICES.md).
