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
- Native line commenting with **Toggle Line Comment** (`Ctrl+/` on Windows and
  Linux, `Cmd+/` on macOS), including multi-line selections.
- Removal of every full-line and inline comment in the current file.

The parser remains useful while a line is incomplete or invalid, making the
extension suitable for normal incremental editing.

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
| `ic10.trace.server` | `off` | Logs LSP communication at `messages` or `verbose` level. |

Settings can be changed through **Preferences: Open Settings (UI)** by
searching for `Stationeers IC10 Toolkit`.

## Supported platforms

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
