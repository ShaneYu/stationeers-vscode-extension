# Stationeers IC10

A from-scratch IC10 language toolchain for Stationeers:

- a fast Rust parser and analysis library;
- a native Rust Language Server Protocol server;
- a VS Code extension with generated TextMate syntax highlighting;
- a standard-library-only Python pipeline for Stationpedia exports.

The generated Stationpedia reference data and relevant Stationpedia thumbnails are
bundled into release artifacts. People who install the extension do **not**
need Stationeers, Python, or a `.env` file.

## Current feature baseline

- Syntax highlighting for instructions, labels, registers, devices, macros,
  constants, enum values, numbers, and comments.
- Context-aware completion for instructions, operands, registers, device pins,
  constants, enums, labels, prefab hashes, and `HASH`/`STR` literal macros.
- Hover help for instructions, registers (`r0-r15`, `sp`, and `ra`), device
  references (`d0-d5` and `db`), constants, enums, symbols, reagent hashes,
  computed CRC-32 `HASH("...")` values, packed `STR("...")` display strings,
  prefab names, and numeric prefab hashes. Device, ingot, and ice hovers include
  bundled images.
- Signature help generated from the game's command syntax.
- Go to definition for labels, defines, and aliases in the current document.
- Diagnostics for unknown/deprecated instructions, operand counts, malformed
  literal macros, invalid `STR` text, duplicate symbols, missing labels, invalid
  labels, and the 128-line program limit.
- Document symbols for labels, defines, and aliases.

This is an intentionally conservative first parser. It understands IC10's
line-oriented structure and remains useful while a line is incomplete or
invalid. See [the architecture and roadmap](docs/architecture.md) for the next
semantic-analysis milestones.

## Repository layout

```text
crates/
  ic10-data/       Typed, embedded generated data
  ic10-core/       Parser, symbols, and diagnostics
  ic10-lsp/        LSP protocol adapter and server binary
packages/
  vscode/          VS Code extension, grammar, and hover assets
tools/
  stationpedia/    Python export transformer and overrides
data/generated/    Versioned JSON consumed by Rust builds
docs/              Architecture and data-pipeline notes
```

The previous third-party IC10 packages are not dependencies and were not used
as scaffold sources.

## Prerequisites

- Rust 1.90 (pinned by `rust-toolchain.toml`)
- Node.js 22 or newer
- Python 3.11 or newer, only when refreshing Stationpedia data

## Build and test

```powershell
npm install
npm test
npm run build
```

For extension development, open the repository in VS Code and run the
`Run IC10 Extension` launch configuration. Its build task compiles the debug
server and bundles the TypeScript client.

To create a platform-specific VSIX:

```powershell
npm run package:extension
```

The package task compiles a release server, stages it under the current
`platform-architecture`, bundles the client, and invokes `vsce`.

## Refresh Stationpedia data

Copy `.env.example` to `.env` and point it at either the game installation or
the export directory:

```dotenv
STATIONEERS_DIR="C:\Program Files (x86)\Steam\steamapps\common\Stationeers"
```

Then run:

```powershell
python tools/stationpedia/generate.py
```

The generator validates the source, applies the reviewable corrections in
`tools/stationpedia/overrides.json`, writes deterministic JSON and TextMate
grammar files, and copies logic-capable, ingot, and ice thumbnails. Generated files
are committed so normal builds never depend on a local game install.

Run `python tools/stationpedia/generate.py --help` for path overrides and the
`--no-assets` option.

## Before publishing

Replace the placeholder repository URL and VS Code publisher, then choose and
add the project license.
