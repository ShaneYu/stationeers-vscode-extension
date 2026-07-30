---
layout: home
hero:
  name: Stationeers Toolkit
  text: IC10 development for VS Code
  tagline: Edit, simulate, debug, test, and build Stationeers programs with fast offline tooling.
  image:
    src: /icon.png
    alt: Stationeers Toolkit extension icon
  actions:
    - theme: brand
      text: Get started
      link: /guide/getting-started
    - theme: alt
      text: Explore simulation
      link: /guide/simulation
    - theme: alt
      text: Steam Workshop
      link: https://steamcommunity.com/sharedfiles/filedetails/?id=3774046989

features:
  - icon: ✨
    title: Offline editor intelligence
    details: Completion, hover help, signatures, navigation, rename, diagnostics, formatting, and semantic highlighting for IC10.
  - icon: 🧪
    title: Deterministic simulation
    details: Model devices, pins, networks, registers, stacks, slots, and shared worlds in source-controlled scenario files.
  - icon: 🐞
    title: Native debugging
    details: Use breakpoints, watches, editable state, multiple IC threads, world-tick stepping, and repeatable debug sessions.
  - icon: ✅
    title: Scenario testing
    details: Run parameterized Stationeers environments through Test Explorer or the headless ic10 command-line runner.
  - icon: 📦
    title: Safe deployment builds
    details: Produce readable or compact game code with previews, source maps, metadata, and reproducibility reports.
  - icon: 🔌
    title: Optional live mod integration
    details: Connect to the local Stationeers Toolkit mod and StationeersLua when those integrations are installed and enabled.
---

## A practical workflow

```text
write .ic10 → inspect in VS Code → simulate → debug → test → build for game
```

The extension bundles its native language server, generated Stationpedia data,
debug adapter, and command-line runner. A Stationeers installation is not
needed for normal editing, simulation, or tests.

::: tip Start with a template
Run **IC10: Create Environment from Template** to open a working example, or
follow the [getting started guide](/guide/getting-started) for a clean project.
:::

::: warning Project status
The simulator intentionally models the behaviours needed for deterministic
editing and testing. It is not a complete replacement for Stationeers physics;
check the [compatibility report](https://github.com/ShaneYu/stationeers-vscode-extension/blob/main/docs/simulator-compatibility.md)
for current fidelity.
:::

## Independent project

Stationeers Toolkit is an independent community project. It is not affiliated
with, endorsed by, or sponsored by RocketWerkz. Stationeers names, reference
material, and images remain the property of RocketWerkz and its licensors.
