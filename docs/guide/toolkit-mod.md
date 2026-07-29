# Stationeers Toolkit mod

The optional Stationeers Toolkit mod adds the in-game side of the local
integration. It is useful for live discovery and IC10 source operations while
the VS Code extension remains the editor and debug client.

The mod is built from this repository under `mods/StationeersToolkit`. Its
runtime is intentionally local: the bridge binds to loopback and the extension
does not send source code or Stationeers data to an external service.

## When to install it

Install the mod when you want to inspect live networks, discover IC housings,
or use the editor's source bridge against a running game. You do not need it
for offline completion, simulator scenarios, tests, or deployment builds.

See the [mod README](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/mods/StationeersToolkit/README.md) and
[live integration contract](../live-integration/stationeers-lua/contract-0.9.5.0.md)
for packaging and compatibility details.
