# Getting started

Stationeers Toolkit gives `.ic10` files a full local development loop: edit,
inspect, simulate, debug, test, and build.

## Install the extension

Install **Stationeers Toolkit** from the Visual Studio Marketplace or Open VSX.
For a manual install, download the VSIX from the
[GitHub Releases](https://github.com/ShaneYu/stationeers-vscode-extension/releases)
page and run **Extensions: Install from VSIX...**.

The extension requires Visual Studio Code 1.107 or newer. Platform packages
are provided for Windows, Linux, and macOS architectures supported by the
release.

## Create your first program

Create `hello.ic10` and open it in VS Code:

```ic10
define Light HASH("StructureLight")
alias lamp d0

start:
  s lamp On 1
  yield
  j start
```

Completion, hover help, diagnostics, and the line/operation budget activate
automatically when the language mode is **IC10**.

## Try a simulation

Run **IC10: Create Simulation Environment**, add an IC housing and a light,
assign `hello.ic10`, then press F5. The environment is saved as a
`*.stationeerssim.json` file so it can be reviewed and shared in source control.

## Next steps

- Learn the [IC10 editing workflow](/guide/ic10-editing).
- Walk through [simulation and debugging](/guide/simulation).
- Start with a [template project](/examples/templates).
- Read the [commands and settings reference](/guide/commands-settings).
