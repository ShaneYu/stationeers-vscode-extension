# What the toolkit does

Stationeers Toolkit is a VS Code extension plus native tooling for IC10
development. The extension is designed to remain useful away from the game:
the parser, language server, simulator, debugger, and test runner operate on
local files and bundled data.

## Core pieces

| Piece | Role |
| --- | --- |
| Language server | Completion, hover, signatures, symbols, navigation, diagnostics, and formatting. |
| Simulator | Deterministic IC10 execution with devices, pins, networks, memory, slots, and stacks. |
| Debug adapter | Breakpoints, stepping, watches, editable values, and multi-IC world ticks. |
| Scenario runner | Headless and Test Explorer execution for repeatable environments. |
| Build command | Validates and emits deployable code plus useful sidecars. |
| Stationeers Toolkit mod | Optional in-game bridge for local discovery and live IC10 integration. |

The separate StationeersLua integration is optional. It is useful when a world
also contains Lua chips, but it is not required to edit or simulate IC10.

## Repository documentation

This site is the friendly entry point. The repository also contains deeper
engineering references for [architecture](../architecture.md),
[simulator behaviour](../simulator.md),
[scenario testing](../scenario-testing.md),
[deployment builds](../deployment-builds.md), and
[live integration](../live-integration/workspace-formats.md).
