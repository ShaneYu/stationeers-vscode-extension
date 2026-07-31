![GitHub Release](https://img.shields.io/github/v/release/shaneyu/stationeers-vscode-extension)
![VS Code Version](https://vsmarketplacebadges.dev/downloads/shaneyu.stationeers.webp?label=VS%20Code%20Downloads)
![Open VSX Downloads](https://img.shields.io/open-vsx/dt/shaneyu/stationeers?label=Open-VSX%20Downloads)

# Stationeers Toolkit

Fast, offline development tools for Stationeers IC10 and Lua in VS Code. The
extension bundles its language server and reference data, so you do not need
Python, a Stationeers installation, or a separately installed language server.

📚 **[Read the full documentation](https://shaneyu.github.io/stationeers-vscode-extension/)**

> This is an independent community project. It is not affiliated with,
> endorsed by, or sponsored by RocketWerkz.

## What you get

- IC10 completion, hover help, signatures, diagnostics, navigation, formatting,
  and safe symbol renaming.
- Source-controlled simulation environments, deterministic scenario tests, and
  native debugging for single- and multi-IC systems.
- Visual networks and topology editing for simulated devices and connections.
- Deterministic deployment builds with readable or compact output, source maps,
  reproducibility metadata, and reports.
- **Live Networks** for discovering networks and IC housings in a running game
  and inspecting or synchronising live IC10 source.
- Optional Lua editing support, with StationeersLua integration available for
  current Lua metadata and live Lua source operations.

## See it in action

![Visual simulation environment with networks, devices, and IC housings.](https://shaneyu.github.io/stationeers-vscode-extension/screenshots/simulation/editor-overview.png)

![IC10 debugger paused at a breakpoint with runtime state available.](https://shaneyu.github.io/stationeers-vscode-extension/screenshots/debugging/breakpoint-paused.png)

![Live Networks showing connected remote networks and discovered chips.](https://shaneyu.github.io/stationeers-vscode-extension/screenshots/live-integration/network-scope-and-chips.png)

## Quick start

1. Install **Stationeers Toolkit** from the VS Code Extensions view.
2. Create or open a `.ic10` file.
3. Start typing, or run **IC10: Create Simulation Environment** to build a
   source-controlled world and press F5 to debug it.

```ic10
define Light HASH("StructureLight")
alias lamp d0

start:
  s lamp On 1
  yield
  j start
```

Language features activate automatically when the language mode is **IC10**.

## Choose a guide

- [Getting started](https://shaneyu.github.io/stationeers-vscode-extension/guide/getting-started)
- [IC10 editing](https://shaneyu.github.io/stationeers-vscode-extension/guide/ic10-editing)
- [Simulation and debugging](https://shaneyu.github.io/stationeers-vscode-extension/guide/simulation)
- [Scenario testing](https://shaneyu.github.io/stationeers-vscode-extension/guide/scenario-testing)
- [Debugging](https://shaneyu.github.io/stationeers-vscode-extension/guide/debugging)
- [Live Networks](https://shaneyu.github.io/stationeers-vscode-extension/guide/live-networks)
- [StationeersLua integration](https://shaneyu.github.io/stationeers-vscode-extension/guide/stationeers-lua)
- [Deployment builds](https://shaneyu.github.io/stationeers-vscode-extension/guide/deployment-builds)
- [Commands and settings](https://shaneyu.github.io/stationeers-vscode-extension/guide/commands-settings)

## Installation

Install **Stationeers Toolkit** from the Visual Studio Marketplace or Open VSX.
For a manual install, download the matching VSIX from
[GitHub Releases](https://github.com/ShaneYu/stationeers-vscode-extension/releases)
and run **Extensions: Install from VSIX...**.

The extension requires Visual Studio Code 1.107 or newer. Packages are
available for Windows, Linux, and macOS architectures supported by the release.

The optional Stationeers Toolkit mod is required only for live game discovery
and source operations. Offline IC10 editing, simulation, testing, debugging,
and deployment builds work without the mod.

## Lua and StationeersLua

Toolkit includes a lightweight Lua metadata fallback, so Lua editing does not
require StationeersLua. Install both extensions when you want StationeersLua's
latest Lua metadata or live Lua source support. Use only one extension's live
network and source-synchronisation tools for a given game session; Toolkit is
the recommended live owner for projects using its IC10 simulation, testing,
topology, or debugging workflows.

See the [StationeersLua integration guide](https://shaneyu.github.io/stationeers-vscode-extension/guide/stationeers-lua)
for the details.

## Support

- [Report a bug or request a feature](https://github.com/ShaneYu/stationeers-vscode-extension/issues)
- [Support policy](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/packages/vscode/SUPPORT.md)
- [Contributing guide](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/CONTRIBUTING.md)
- [Architecture and roadmap](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/docs/architecture.md)

## Privacy and license

The extension runs locally, includes no telemetry, and does not send source code
or Stationeers data to an external service.

Project source code is available under the
[MIT License](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/LICENSE).
Stationeers names, reference material, and images remain the property of
RocketWerkz and its licensors; see the
[third-party notices](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/THIRD_PARTY_NOTICES.md).
