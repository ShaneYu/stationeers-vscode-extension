# P3.01 — Neutral workspace formats

## Status and dependencies

- **Status:** ready
- **Depends on:** [P1.02](p1-02-scenario-tests-and-cli.md) current scenario
  model
- **Blocks:** P3.08, P3.09, and the final P3 release
- **AI execution size:** large; use schema/core, VS Code, and migration
  changesets if one reviewable change is not practical

## Goal

Make `*.stationeerssim.json`, `*.stationeerstest.json`, and
`*.stationeerssim.layout.json` the canonical language-neutral workspace files.
They can reference `.ic10` and `.lua` programs in one simulated world. Existing
`.ic10sim.json`, `.ic10test.json`, and `.ic10sim.layout.json` projects continue
to load without destructive migration.

This item changes formats and naming, not Lua execution. Until P3.09 lands, a
Lua program must produce a precise `unsupported runtime` diagnostic wherever
execution is requested.

## Context an agent must load

- [P3 epic](p3-00-live-integration-epic.md)
- [scenario-test backlog](p1-02-scenario-tests-and-cli.md)
- `crates/ic10-sim/src/scenario.rs` and scenario fixtures
- `crates/ic10-runner/src/main.rs` and `runner.rs`
- `packages/vscode/package.json`
- the installed/public `sumneko.lua` extension contract and VS Code extension
  dependency rules
- environment, testing, launch, and topology controllers in
  `packages/vscode/src/`
- JSON schemas and templates in `packages/vscode/`
- simulator/testing/architecture user documentation

Search all filename suffixes before editing. The known references in this
backlog are not an exhaustive migration list.

## Required model

The schema design must:

- identify every executable program with a stable scenario-local ID, URI/path,
  and explicit `language: "ic10" | "lua"`;
- keep devices, connections, networks, slots, initial values, schedules,
  stimuli, assertions, and layout language-neutral;
- allow a mixed world with multiple IC10 and Lua programs;
- allow a test case to select a program/VM without assuming IC10 registers;
- preserve unknown fields where the existing loaders already permit it;
- resolve all referenced paths relative to the owning config URI using the
  existing workspace filesystem abstraction; and
- retain deterministic ordering and portable, relative persisted paths.

Do not merely rename the current schema while leaving IC10-only field names in
new canonical files. Write a short format decision in
`docs/live-integration/workspace-formats.md` with before/after examples and the
compatibility policy.

## Lua editor integration

This toolkit replaces the StationeersLua VS Code extension, but it does not
need to replace a mature general-purpose Lua language server.

- Add `"sumneko.lua"` to the extension manifest's
  `extensionDependencies`. VS Code has no optional-dependency manifest field,
  and Lua language intelligence is part of the promised dual-language product.
- Activate the Stationeers workflow for referenced/open `.lua` programs without
  registering a second general Lua server.
- Configure Lua 5.2 only through documented `sumneko.lua` integration/settings.
  Do not silently overwrite unrelated user or workspace Lua settings.
- Generate versioned annotation/library files for supported StationeersLua
  globals, enums, modules, and callbacks. Create the minimal evidence-backed
  editor API profile in this item; P3.09 extends and consumes that same source
  for runtime mocks. Do not maintain handwritten annotations separately.
- If no public extension API can add the annotation library dynamically,
  provide a previewed, reversible configuration command and document the exact
  setting it changes.
- Detect `OrbitalFoundryModdingCrew.stationeers-lua` only to explain that it is
  unnecessary and may duplicate Stationeers commands/views. Never call its
  commands, read its state, or make it a dependency.

## Deliverables

1. Add versioned neutral schemas and representative mixed-language fixtures.
2. Refactor the shared Rust scenario/test model only as far as needed to carry
   explicit program language and VM-neutral selectors.
3. Recognize canonical and legacy suffixes in the CLI, Test Explorer, file
   watchers, custom editors, debug configuration, templates, and JSON
   validation. Recognize referenced `.lua` programs without claiming ownership
   of an unrelated generic Lua language server.
4. Generate new projects with canonical names. Never silently rename an
   existing file.
5. Keep legacy inputs readable and semantically equivalent. If conversion is
   offered, make it an explicit previewable command and preserve a backup.
6. Update product wording from IC10-only to Stationeers where the surface now
   applies to both languages; retain IC10 wording for IC10-specific features.
7. Add migration fixtures covering legacy scenario, test, and layout files.
8. Add the `sumneko.lua` dependency, Lua 5.2 configuration/annotation
   integration, and present/absent/conflicting-extension tests.

## Validation and evidence

Run at minimum:

```text
cargo test --locked -p ic10-sim -p ic10-runner
npm run test --workspace packages/vscode
npm run check
npm test
npm run package:extension
```

Evidence must include:

- golden loads of canonical IC10-only, Lua-only, mixed, and all three legacy
  file types;
- a round-trip proving no path or language changes;
- expected diagnostics for Lua execution before P3.09; and
- multi-root/URI tests proving no direct local-filesystem assumption was added;
- packaged-extension dependency metadata for `sumneko.lua`; and
- Lua editor tests with `sumneko.lua` active and the StationeersLua VS Code
  extension absent.

## Acceptance criteria

- [ ] New files default to the three canonical Stationeers suffixes.
- [ ] Old suffixes remain indexed, editable, runnable, and covered by fixtures.
- [ ] A mixed scenario can identify both VM languages without schema ambiguity.
- [ ] Referenced `.lua` files activate the applicable Stationeers
      simulation/test workflow even when StationeersLua is absent.
- [ ] Installing the packaged toolkit installs/enables the declared
      `sumneko.lua` dependency.
- [ ] Lua language service configuration targets Lua 5.2 and loads generated
      Stationeers annotations without replacing unrelated settings.
- [ ] No feature or test requires the StationeersLua VS Code extension.
- [ ] IC10 simulation/test results are unchanged for equivalent old and new
      fixtures.
- [ ] Attempting unsupported Lua execution fails explicitly and does not treat
      Lua as IC10.
- [ ] Schemas, templates, launch configuration, file watchers, CLI help, docs,
      and changelog agree on canonical and legacy names.
- [ ] No migration rewrites user files without an explicit command and preview.

## Stop conditions

- Stop and document a schema decision before implementation if the existing
  scenario model cannot express explicit programs without a breaking semantic
  migration.
- Stop rather than adding Node `fs` path handling if a VS Code API/URI path is
  required for virtual or remote workspaces.
- Stop rather than depending on undocumented `sumneko.lua` internals; use its
  supported settings/API or a previewed user configuration flow.
- Do not delete legacy support to simplify the implementation.

## Non-goals

- Embedding Lua 5.2 or implementing `ic.*`.
- Implementing another general Lua grammar/language server.
- Calling or wrapping the StationeersLua VS Code extension.
- Live game discovery or deployment.
- Renaming the existing Rust crates solely for cosmetic neutrality.

## Decisions

- Canonical persisted names are neutral; legacy IC10 names are compatibility
  aliases.
- Language is explicit persisted data, never guessed only from program
  contents.
- `sumneko.lua` is a required extension dependency; the StationeersLua VS Code
  extension is not.
