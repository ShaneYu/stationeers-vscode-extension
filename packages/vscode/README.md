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
- Hover help for instructions, registers (`r0-r15`, `sp`, and `ra`), device
  references (`d0-d5` and `db`), constants, enums, symbols, reagent hashes,
  computed `HASH("...")` values, packed `STR("...")` display strings, prefab
  names, and numeric prefab hashes.
- Signature help generated from IC10 command syntax.
- Go to definition for labels, defines, and aliases in the current document.
- Rename symbol support for labels, defines, and aliases using **Rename
  Symbol** (`F2`), with document-wide reference updates and collision
  validation.
- Diagnostics for unknown or deprecated instructions, operand counts, malformed
  literal macros, invalid `STR` text, duplicate symbols, missing labels, invalid
  labels, and the 128-line program limit.
- Document symbols for labels, defines, and aliases.
- Visual `*.ic10sim.json` environments for devices, labeller names, numbered
  connections, networks, pins, fields, slots, registers, and stack values.
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
> network debugging. Device logic state is modeled, but active game physics
> such as chute travel, vending exports, recipes, and atmospherics are not yet
> advanced automatically.

1. Open an `.ic10` program.
2. Run **IC10: Create Simulation Environment**.
3. Add networks and devices in the visual editor.
4. Configure each IC housing's program, data-cable pins, connections,
   registers, and initial stack.
5. Select an IC housing and use the environment editor's **Debug** button or
   press F5. F5 from an `.ic10` file locates the simulation that references
   that program.

### Configure a shared environment

![Visual IC10 simulation environment with multiple networks, devices, and IC programs.](https://raw.githubusercontent.com/ShaneYu/stationeers-vscode-extension/main/docs/marketplace/editor.png)

The environment editor filters `d0`–`d5` targets to devices on the housing's
data cable, offers named choices for known device modes, and provides inline
metadata help. IC10 program paths can be selected from workspace files or
browsed externally, and VS Code file/folder renames update references.

The searchable device catalogue matches display names, prefab names, and
PrefabHash values while showing each result's thumbnail and identity.

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

## Commands

Open the Command Palette and run:

- **IC10: Remove All Comments** — removes every comment from the current IC10
  file while preserving line breaks and hash characters inside quoted
  `HASH`/`STR` literals. Comment-only lines are deleted, and literal numeric
  offsets in relative `br...` and `jr` instructions are updated to preserve
  their destinations. Relative branches that become a redundant zero-offset
  jump are removed. Dynamic offsets stored in registers, aliases, or defines
  cannot be safely updated and produce a warning. The command is also
  available from the editor's context menu.
- **IC10: Restart Language Server** — stops and restarts the language server.
- **IC10: Create Simulation Environment** — creates a source-controlled
  `*.ic10sim.json` environment and opens its visual editor.
- **IC10: Step World Tick** — advances every eligible IC in the active
  simulation by one coordinated tick.

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
