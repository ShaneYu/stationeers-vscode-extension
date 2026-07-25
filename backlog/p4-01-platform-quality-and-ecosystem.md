# P4.01 — Platform quality and ecosystem

## Goal

Harden the complete toolkit so users can trust releases, contributors can make
changes safely, and persisted projects survive game and extension updates.

## Extension-host testing

Add real VS Code extension-host tests for:

- activation and native binary selection;
- diagnostics, completion, hover images, rename, references, semantic tokens,
  and code actions;
- debug launch, breakpoint capabilities, state views, and test debugging;
- custom-editor save, undo/redo, rename tracking, and reveal/navigation;
- multi-root and Remote Development workspaces;
- high-contrast, zoom, keyboard-only, and screen-reader-critical flows.

Retain fast Rust/TypeScript unit tests and protocol smoke tests. Extension-host
tests cover integration, not every semantic edge case.

## Schema and protocol migrations

Provide explicit migrations for:

- `*.ic10sim.json`;
- `*.ic10test.json`;
- build/source-map metadata;
- trace files;
- live bridge protocol and saved target mappings.

Requirements:

- preserve unknown future data where safe;
- create a backup or preview before destructive migration;
- support reading at least the immediately previous stable schema;
- test upgrades with golden fixtures;
- never silently reinterpret values.

## Game-data lifecycle

- Display the bundled Stationeers version in an About/diagnostics command.
- Detect a local game version when available and warn on mismatch without
  transmitting it.
- Automate data-refresh pull requests from an explicitly provided game export.
- Fail CI on unknown instruction/device shapes and require conformance review.
- Publish compatibility and known-deviation notes with each release.
- Support stable and beta data channels only if both can be maintained
  accurately.

## Performance and reliability budgets

Track:

- extension activation time;
- LSP latency and memory;
- environment-editor render/save latency;
- simulation operations per second;
- DAP stop/variables response time;
- trace overhead;
- bridge game-thread and network overhead.

Add representative benchmarks and retain results as CI artefacts. Define
budgets before optimising and fail or flag material regressions.

## Accessibility and UX

- Audit webviews against VS Code webview guidance.
- Support high contrast, reduced motion, keyboard navigation, focus retention,
  accessible labels, and editor zoom.
- Prefer standard Problems, Test Explorer, Debug, Output, Quick Pick, status
  bar, and tree views over custom UI.
- Keep advanced hints and overlays configurable.
- Ensure every webview action has a command or keyboard-accessible equivalent.

## Documentation and contributor experience

- Keep `docs/architecture.md` synchronized with implemented features; remove
  roadmap entries such as rename once they are complete.
- Add architecture decision records for persistent formats, evaluator grammar,
  behaviour models, and the live bridge.
- Provide small contribution fixtures for instructions, devices, behaviours,
  tests, and bridge protocol changes.
- Generate reference/compatibility documentation where possible.
- Add issue templates that request game version, extension version, scenario,
  minimal source, and compatibility report.

## Privacy and diagnostics

Keep local/offline operation as the default. Provide an explicit
`IC10: Collect Diagnostic Bundle` command that:

- previews included files and fields;
- redacts absolute paths and live-server credentials;
- includes versions, logs, compatibility status, and selected minimal fixtures;
- never uploads automatically.

Any future telemetry remains separately opt-in and is not required for core
functionality.

## Acceptance criteria

- [ ] Critical language, simulator, debugger, test, and custom-editor workflows
      run in VS Code extension-host CI.
- [ ] Persisted schemas have versioned migration fixtures.
- [ ] Game-data updates require explicit semantic/conformance review.
- [ ] Compatibility and performance reports ship with releases.
- [ ] Webviews pass keyboard and high-contrast review.
- [ ] Diagnostic bundles are previewed, redacted, and local-only.
- [ ] Architecture and backlog documents are checked during release review.
- [ ] Contributor documentation covers every generated or versioned artefact.

## Dependencies

- Applies after the interfaces introduced by P0–P3 stabilise, although tests
  and accessibility checks should be added continuously during those items.

## Non-goals

- Mandatory telemetry.
- A browser/web build that compromises the native offline simulator or
  debugger.

## Decisions

- Local-first privacy remains a product feature.
- Standard VS Code surfaces are preferred over custom webviews.
