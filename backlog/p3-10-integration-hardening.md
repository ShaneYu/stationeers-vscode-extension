# P3.10 — Integration hardening and release

## Status and dependencies

- **Status:** blocked until the features selected for the first P3 release are
  complete
- **Current execution note (2026-07-28):** documentation, fixture coverage,
  and release evidence scaffolding may proceed. Supported-release acceptance is
  still blocked: the current P3.02 evidence does not show atomic compare-and-set
  for source writes, and P3.07 authority/relay scenarios are not runtime-
  observed. Do not check the real-game acceptance boxes from mocks, builds, or
  protocol fixtures.
- **Depends on:** P3.01 through P3.09, except any item explicitly deferred by a
  recorded release-scope decision
- **Blocks:** supported public release of the bridge/Lua integration
- **AI execution size:** release epic; execute and report each gate separately

## Goal

Turn the completed vertical slices into a supportable, secure, measurable, and
recoverable release of the extension plus separately packaged bridge mod.

This item cannot compensate for an unverified feasibility gate. A deferred
feature must be absent or clearly experimental, not silently accepted as a
known defect.

## Context an agent must load

- [P3 epic](p3-00-live-integration-epic.md) and completion packets for every
  implemented P3 item
- all `docs/live-integration/` evidence and unresolved constraints
- [P4 platform quality](p4-01-platform-quality-and-ecosystem.md)
- extension/mod release, license, privacy, and support documentation
- current supported OS/architecture/game/mod-loader/StationeersLua matrix

## Release gates

### Compatibility and packaging

- Publish the bridge mod separately from the VS Code extension with explicit
  dependency/version metadata.
- Package no proprietary game assemblies, user config, tokens, saves, or
  decompiled material.
- Define supported game, BepInEx, StationeersLaunchPad, bridge, extension,
  StationeersLua in-game service, `sumneko.lua`, OS, and architecture
  combinations.
- Verify the Marketplace package declares `sumneko.lua` as an extension
  dependency and declares no dependency on
  `OrbitalFoundryModdingCrew.stationeers-lua`.
- Detect mismatches in both the in-game mod and extension and fail safely with
  upgrade/downgrade guidance.
- Verify clean install, upgrade from the immediately previous supported
  schema/protocol, disable/uninstall, and save load without the bridge enabled.
- Confirm canonical and legacy workspace files are included in packaging,
  activation, schemas, templates, and docs.

### Security

- Threat-model local malicious processes, browsers, untrusted workspaces,
  remote VS Code extension hosts, hostile/oversized protocol input, replay,
  stale worlds, unauthorized players, and slow clients.
- Fuzz/abuse HTTP and WebSocket parsing, schemas, source sizes, queue limits,
  RPC correlation, and reconnect paths.
- Verify loopback-only binding, origin policy, token generation/storage/
  revocation, log redaction, workspace trust restrictions, server permissions,
  and audit integrity.
- Have an independent review of the authoritative write path and every place
  source code crosses a process boundary.

### Performance and resilience

Define measured budgets before release for:

- idle mod allocations and main-thread time;
- initial discovery total and maximum per-tick work;
- topology/source invalidation;
- source pull and conflict-safe push;
- WebSocket/RPC queue depth and coalescing;
- reconnect/resync;
- one and multiple multiplayer IDE users;
- extension activation, tree refresh, and memory; and
- Lua/IC10 simulation and test throughput.

Benchmark small, representative large, and adversarial worlds. Include many
RemoteNetwork anchors, duplicate labels, aliased scopes, chips, topology churn,
slow clients, and reconnects. The release must have no periodic full-world
scan, unbounded queue, or recurring observable hitch.

Test recovery from:

- port collision and invalid/revoked token;
- world unload/reload and server travel;
- device removal/cable changes/chip replacement;
- client, server mod, or VS Code restart;
- StationeersLua restart/version mismatch/scope loss;
- `sumneko.lua` missing/disabled/version mismatch and annotation-profile
  mismatch;
- multiplayer permission change and disconnect;
- malformed/unknown protocol fields; and
- extension deactivation during requests/debug sessions.

### VS Code platform behaviour

- Decide with evidence whether local-only `extensionKind: ["workspace"]` plus
  documented forwarding is sufficient, or split UI and connector components
  for Remote Development. Do not claim remote support without extension-host
  tests.
- Test multi-root, virtual/read-only/untrusted workspaces, URI schemes,
  Secondary Sidebar movement, keyboard-only access, high contrast, zoom, and
  screen-reader-critical tree/confirmation flows.
- Ensure every tree action has a command/Quick Pick equivalent.
- Keep bridge and StationeersLua diagnostics separately identifiable.

### Documentation and operations

- Update architecture diagrams, user guides, in-game crafting/device docs,
  pairing, permissions, mapping, conflict resolution, local Lua profiles,
  compatibility, known deviations, and troubleshooting.
- Explain duplicate label semantics, session-only IDs, why a visible Lua chip
  may not be StationeersLua-eligible, and why MCP is not configured.
- Explain that this toolkit replaces the StationeersLua VS Code extension and
  that `sumneko.lua` supplies the underlying general Lua 5.2 language service.
- Add a redacted diagnostic bundle that previews content and never uploads
  automatically.
- Document kill switch, token revocation, server audit location/retention,
  protocol/schema migration, backup/recovery, and support data.
- Include an `Unreleased` changelog entry in each shipped component.

## Deliverables

1. Supported-version and packaging manifests for the extension and bridge.
2. Threat model, resolved security review, fuzz/abuse suites, and redaction
   audit.
3. Published performance budgets, benchmark harnesses, and retained results.
4. Extension-host accessibility/remote-workspace evidence.
5. Complete player, server-admin, contributor, migration, troubleshooting, and
   recovery documentation.
6. Redacted diagnostic bundle and final durable release evidence packet.

## Validation and evidence

Verify commands against current manifests, then run at least:

```text
npm ci
npm run conformance:check
npm run check
npm test
npm run build
npm run package:extension
npm run release:check
```

Also run the C# format/build/test/package commands established by P3.02, native
platform build matrix, extension-host suites, protocol/schema compatibility
fixtures, security tests, and benchmark gates.

The focused P3.10 documentation/contract check is:

```text
node tools/verify-p310-docs.mjs
```

It validates the OpenAPI/fixture JSON and checks that the release checklist,
evidence template, and report template retain the required fail-closed and
real-game gates. It does not perform a game run and cannot advance a gate.

Use [the P3.10 integration and release checklist](../docs/live-integration/p310-integration-release-checklist.md)
for the executable fixture matrix, manual game sequences, workspace-host
limitations, and release-stop rules. Copy
`docs/live-integration/evidence/p310-release-evidence.template.json` and
`docs/live-integration/releases/p310-release-report.template.md` for a new
evidence packet; replace every `pending` value with an attributable result or
leave it pending.

`npm ci` is appropriate for clean CI/release environments; an agent must not
discard a user's local dependency work merely to run it.

## Release evidence packet

Create a durable release report under `docs/live-integration/releases/` with:

- component/version compatibility table;
- exact commits/build inputs;
- all automated command results;
- real-game topology/authority matrix;
- performance budget table and measurements;
- security review findings and resolutions;
- known deviations/deferred capabilities;
- package hashes and contents audit; and
- rollback/recovery result.

Redact paths, player/server identity, network addresses, source code, and
credentials.

## Acceptance criteria

- [ ] Supported combinations pass clean install, upgrade, recovery, and
      packaging tests.
- [ ] A clean install includes `sumneko.lua`, requires no StationeersLua VS
      Code extension, and exposes only one Stationeers live workflow.
- [ ] Security review finds no unauthenticated/public listener, stale/name-only
      write, token leak, unbounded input, or client-authoritative multiplayer
      path.
- [ ] Published performance budgets pass representative large-world and
      multi-user runs.
- [ ] Remote workspace support claims match tested behaviour.
- [ ] Diagnostic collection is previewed, redacted, local-only, and separates
      bridge from StationeersLua state.
- [ ] User/admin/contributor docs and changelogs match shipped capabilities.
- [ ] Packages contain no proprietary assemblies or sensitive evidence.
- [ ] Deferred features fail explicitly and are absent from misleading menus.
- [ ] The release report is complete and reproducible.

## Stop conditions

- Stop release on any authority, conflict-safety, credential, public-bind,
  unbounded-queue, recurring-hitch, save-corruption, or proprietary-package
  failure.
- Stop if manual game evidence is unavailable; compilation and mocks alone are
  insufficient for a supported release.
- Stop rather than weakening a gate to meet a date. This backlog has no
  calendar deadline.
- A report with `realGame.acceptance.status` other than `observed` is not a
  supported-release acceptance report. In particular, `not-run`,
  `runtime-constrained`, `mock-only`, and `observed-with-blocker` remain
  blocked states.

## Non-goals

- Adding new product features during release hardening.
- Direct public dedicated-server IDE access.
- Custom MCP integration.
- Claiming support for untested game/mod versions.

## Decisions

- Extension and bridge mod are versioned and packaged separately.
- Release readiness is an evidence packet, not elapsed implementation time.
