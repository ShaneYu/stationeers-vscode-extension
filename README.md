# Stationeers IC10 Toolkit

<p align="center">
  <img src="packages/vscode/assets/icon.png" width="160" alt="Stationeers IC10 Toolkit extension icon">
</p>

Fast, offline IC10 language support for Stationeers, powered by a native Rust
language server and deterministic simulator.

The extension provides context-aware completion, hover documentation, signature
help, navigation, document symbols, and diagnostics while you edit `.ic10`
programs. The language server and generated reference data are bundled, so
users do not need Python, a Stationeers installation, or a separate server.

> This is an independent community project. It is not affiliated with,
> endorsed by, or sponsored by RocketWerkz.

## Installation

Visual Studio Code 1.107 or newer is required.

- **Visual Studio Code:** open Extensions, search for
  **Stationeers IC10 Toolkit**, and install the extension published by
  `shaneyu`.
- **Antigravity and other Open VSX editors:** search the Extensions view for
  **Stationeers IC10 Toolkit**.
- **Manual installation:** download the VSIX matching your operating system and
  architecture from
  [GitHub Releases](https://github.com/ShaneYu/stationeers-vscode-extension/releases),
  then choose **Extensions: Install from VSIX...**.

Marketplace links will become active with the first public release:

- [Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=shaneyu.stationeers)
- [Open VSX Registry](https://open-vsx.org/extension/shaneyu/stationeers)

## Quick start

1. Install the extension.
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
- Hover help for instructions, registers, devices, constants, enums, symbols,
  reagent hashes, packed display strings, prefab names, and numeric hashes.
- Signature help generated from IC10 command syntax.
- Go to definition, references, highlights, rename, and document/workspace
  symbols for labels, defines, and aliases.
- Typed operand validation and conservative control-flow/value diagnostics,
  including subtle unused/dead-code hints.
- Semantic tokens, label folding, inlay hints, safe quick fixes, formatting,
  and a live official line/operations budget.
- A visual, source-controlled simulation environment for devices, labeller
  names, numbered connections, pins, logic fields, slots, registers, and stack
  values.
- Native VS Code debugging for single- and multi-IC systems, with breakpoints,
  editable variables, watches, instruction stepping, and coordinated world
  ticks.
- Shared cable-network channels and separate data/power network topology.

The parser is intentionally tolerant of incomplete lines, so editor assistance
continues to work while a program is being written.

## Commands and settings

| Name | Purpose |
| --- | --- |
| `IC10: Restart Language Server` | Restarts the bundled language server. |
| `IC10: Create Simulation Environment` | Creates and opens a visual `*.ic10sim.json` scenario. |
| `IC10: Step World Tick` | Runs every eligible simulated IC for one game tick. |
| `ic10.server.path` | Uses a custom `ic10-lsp` executable instead of the bundled server. |
| `ic10.debugAdapter.path` | Uses a custom `ic10-dap` executable instead of the bundled adapter. |
| `ic10.diagnostics.unused` | Controls unused/dead-code diagnostics as `off`, `hint`, or `warning`. |
| `ic10.trace.server` | Logs LSP messages for troubleshooting. |

See the [extension usage guide](packages/vscode/README.md) for troubleshooting
and platform details. See the [simulator guide](docs/simulator.md) for the
environment format, multi-IC debugger model, and current fidelity, and the
[generated compatibility report](docs/simulator-compatibility.md) for the
evidence-backed instruction status.

## Privacy

The extension runs locally. It does not include telemetry and does not send
source code or Stationeers data to an external service.

## Contributing

Bug reports and feature requests are welcome in
[GitHub Issues](https://github.com/ShaneYu/stationeers-vscode-extension/issues).
See [CONTRIBUTING.md](CONTRIBUTING.md) to build the monorepo, run the test suite,
or refresh generated Stationpedia data.

Contributors record user-facing changes under `Unreleased`; maintainers follow
the [release guide](docs/releasing.md) for versioning, signed tags, and
publication.

The design is documented in [docs/architecture.md](docs/architecture.md). The
ordered, implementation-ready product roadmap lives in
[backlog/README.md](backlog/README.md).

## License and attribution

Project source code is available under the [MIT License](LICENSE).
Stationeers names, reference material, and images remain the property of
RocketWerkz and its licensors; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
