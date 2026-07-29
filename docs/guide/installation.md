# Installation and updates

## Recommended installation

Search for **Stationeers Toolkit** in the VS Code Extensions view and install
the publisher package from `shaneyu`. Open VSX-compatible editors can install
the `shaneyu.stationeers` package from their Extensions view.

## Manual installation

Download the VSIX matching the host that runs the extension from
[Releases](https://github.com/ShaneYu/stationeers-vscode-extension/releases),
then use **Extensions: Install from VSIX...**. In Remote Development, the
native server runs on the remote extension host, so choose a package supported
by that host.

## Updating

Use the normal editor update flow, or install a newer VSIX over the current
version. Workspace files are source-controlled JSON and are not migrated by
silently rewriting them; review release notes when a schema changes.

## Build from source

The repository build requires Node.js 22 or newer, Rust, and the platform
toolchains documented in [CONTRIBUTING.md](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/CONTRIBUTING.md). The bundled
language server and generated data mean users do not need Python or a local
Stationeers installation after installation.
