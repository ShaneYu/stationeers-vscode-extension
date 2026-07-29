# P3.08 — Direct StationeersLua service integration

## Status and dependencies

- **Status:** Lua source-sync slice implemented and live HTTP contract
  validated against StationeersLua `0.9.5.0` on 2026-07-29. Isolated
  extension-host activation without the StationeersLua VS Code extension is
  covered, and the corrected Scripted Screen accessibility was manually
  confirmed in a Development Extension Host. Packaged installation validation
  remains. Debugger integration is explicitly deferred.
- **Depends on:** [P3.01](p3-01-neutral-workspace-formats.md),
  [P3.05](p3-05-vscode-live-network-explorer.md)
- **Blocks:** full live Lua feature set and final release
- **AI execution size:** large, fixture-first TypeScript/debug-adapter work

## Goal

Detect an independently installed StationeersLua in-game service and support
eligible Lua source pull, read-only compare, and explicitly best-effort push
through its public API. The custom bridge continues to own RemoteNetwork discovery.
Debugger integration is a later phase of this item and is not part of the next
implementation slice.

StationeersLua absence, disablement, version mismatch, or limited current scope
must not disable IC10 or offline functionality.

The StationeersLua VS Code extension is not a dependency and is not required.
It may nevertheless be installed and active alongside this extension. Our
extension implements its own client, view, source-sync, and eventual
debug-adapter experience. `sumneko.lua` supplies only ordinary Lua
editing/intelligence.

Coexistence is supported but not simultaneous ownership: if the StationeersLua
VS Code extension or in-game Lua editing service is detected, show a clear
warning that using both Lua editing workflows at the same time may cause
duplicate commands, competing updates, or source overwrites. Do not disable or
prevent activation of either extension. Recommend that the user chooses one
Lua editor/synchronization owner for a given chip while retaining the ability
to use both mods and extensions in the same installation.

## Context an agent must load

- [P3 epic](p3-00-live-integration-epic.md)
- P3.01 `sumneko.lua` dependency and generated annotation integration
- P3.05 independent connection state and tree context keys
- exact StationeersLua documentation and OpenAPI/extension behaviour for the
  selected supported version
- StationeersLua guides for the extension REST API, Wireless Dev Board, and
  debugger
- current VS Code debug adapter implementation and debug-type contributions

The upstream service is closed source and its documentation can change. Capture
sanitized request/response fixtures from the exact supported version before
implementing a client. Do not infer endpoints from this backlog alone.
Do not inspect or call the StationeersLua VS Code extension as a substitute for
documented service contracts.

## Discovery and capability handshake

Use an independently configurable base URL, currently documented as
`http://127.0.0.1:3030`. StationeersLua's current LSP is documented on `3031`;
the custom bridge must not use either default.

Probe the documented public status/editor/chip resources (currently described
as `/api/status`, `/api/editor`, and `/api/chips`) and use their reported
capabilities. Do not port-scan and do not infer installation merely from the
bridge mod's optional-mod detection.

Maintain a separate state:

```text
disabled -> connecting -> connected -> no editor/scope
                        \-> debugger disabled
                        \-> incompatible
                        \-> unavailable
```

All commands and context keys update dynamically when either service changes.

## Correlation rule

The bridge can expose Lua chips game-wide because the player placed
`RemoteNetwork` anchors. StationeersLua may expose only the chip reachable from
its active editor or Wireless Dev Board network.

Correlate only by authoritative housing/chip ReferenceIds represented as
strings after validating the upstream fields' meaning with fixtures:

- direct bridge Lua chip + same chip and housing ReferenceIds in
  StationeersLua -> enable the exact upstream-reported source/debug
  capabilities;
- explicitly marked composite housing + exactly one StationeersLua Lua record
  with the same `housing_ref_id` -> use that record's `ref_id` for the
  upstream operation;
- bridge Lua chip absent from StationeersLua -> keep it visible, disable
  upstream actions, and explain how to open/connect the appropriate editor or
  Wireless Dev Board;
- StationeersLua chip absent from bridge scopes -> do not invent an
  `All Networks` node; optionally expose it in a clearly separate
  `Current StationeersLua Scope` group if product review accepts that UX.

Names and scope labels are never correlation keys.

## Source operations — implemented slice

- Use StationeersLua's public pull/export endpoints exactly as documented.
- Preserve its `source_version` and conflict response semantics.
- Label Lua writes as best-effort because the supported API does not currently
  expose an atomic expected-version/hash precondition. Surface any API conflict
  response, but do not implement client-side merge, force, or silent retry.
- Never send Lua source through the custom bridge's IC10 handler.
- Reuse URI-safe compare and confirmation UX where semantics align, while
  keeping transport/error handling separate.

The first Lua slice should mirror the now-validated IC10 user workflow:

1. independently detect and authenticate to StationeersLua;
2. correlate only authoritative housing/chip ReferenceIds, including the
   unique-housing rule for explicitly marked composite hosts;
3. pull Lua source into a named in-memory tab;
4. compare local and game snapshots read-only;
5. push best-effort through the documented raw-code endpoint, warning that a
   newer in-game edit may be overwritten; and
6. fail visibly when the target leaves StationeersLua's active scope or the
   upstream API rejects the operation.

Do not route Lua through the custom bridge IC10 PUT handler. Do not add merge,
force-write, or background save behaviour in this slice.

## Debugging — deferred

- Contribute a distinct debug type for StationeersLua remote attach only after
  the source-sync slice is complete and separately approved.
- Implement only the thin VS Code/DAP translation needed to call the documented
  StationeersLua debug session API.
- Let StationeersLua own breakpoints, stack/locals/upvalues, evaluation,
  stepping, pause/resume, multiplayer proxying, and VM lifecycle.
- Advertise commands only when the upstream handshake says the experimental
  debugger is enabled for the correlated chip.
- Pausing/stepping must affect the selected Lua VM, not the main game thread.
- Handle chip power loss, source change, editor/network scope change, world
  change, upstream restart, and disconnect as explicit terminated/stale debug
  states.

## Deliverables

1. An upstream contract note and sanitized versioned HTTP/debug fixtures under
   `docs/live-integration/stationeers-lua/`.
2. Independent cancellable StationeersLua API client and state machine.
3. ReferenceId correlation service with duplicate/missing cases.
4. Tree/context-menu/status integration for eligible Lua pull/compare/push.
5. Thin debug adapter integration using upstream capabilities (deferred).
6. Fixture tests and real-mod tests for current editor, Wireless Dev Board,
   no-scope, absent mod, and source concurrency semantics.
7. User documentation that clearly assigns responsibility between the bridge
   and StationeersLua.
8. Extension-host coverage proving pull/compare/push work when
   `OrbitalFoundryModdingCrew.stationeers-lua` is not installed.

## Validation and evidence

Run:

```text
npm run test --workspace packages/vscode
npm run check --workspace packages/vscode
npm run test:extension-host --workspace packages/vscode
npm run check
npm test
```

Test a matrix with:

- bridge only;
- StationeersLua in-game service reachable with the custom bridge absent;
- both services;
- `sumneko.lua` installed with the StationeersLua VS Code extension absent;
- the StationeersLua VS Code extension also installed, verifying that both
  extensions activate and that a clear coexistence/side-effects warning is
  shown without private integration;
- different configured ports;
- same/different ReferenceIds;
- IC10 and Lua chips on one scope;
- StationeersLua current editor scope and Wireless Dev Board scope;
- debugger enabled/disabled; and
- either service restarting mid-operation.

## Acceptance criteria

- [x] IC10/offline features behave identically when StationeersLua is absent.
- [x] Live Lua source pull/compare/push works with or without the StationeersLua
      VS Code extension for the wireless/network `mode=chip` path. Exact
      in-game editor synchronization is deferred to an upstream StationeersLua
      fix: the visible selected editor is reported by `/api/editor`, but the
      upstream write guard reports no active IC editor in the dedicated-server
      client session. This is an accepted external limitation to revisit.
- [x] The extension never refuses activation because StationeersLua is present.
- [x] A clear, non-blocking warning explains the risk of simultaneous Lua
      editing and recommends one editing owner per chip.
- [x] Services have independent URLs, cancellation, status, logs, and errors.
- [x] Lua actions appear only for authoritative exact-pair or explicitly
      marked unique-housing ReferenceId matches and advertised capabilities.
- [x] Global bridge visibility is never presented as global StationeersLua
      operability.
- [x] Lua source never enters the custom IC10 mutation route.
- [ ] Deferred debugging, when resumed, delegates to the upstream VM session
      and never pauses the game thread.
- [x] Unsupported/incompatible upstream versions fail visibly.
- [x] Upstream fixtures identify the exact StationeersLua version tested.

## Stop conditions

- Stop if the upstream public contract cannot supply a stable ReferenceId or
  debugger capability needed for safe correlation.
- Stop rather than using reflection into StationeersLua's closed-source
  assemblies.
- Stop rather than delegating any required workflow to the StationeersLua VS
  Code extension.
- Document and visibly label best-effort semantics when the upstream write API
  lacks expected-version concurrency.
- Do not add any MCP configuration or proxy while completing this item.

## Non-goals

- Reimplementing StationeersLua's VM, LSP, debugger, REST server, or MCP server.
- Depending on, wrapping, or importing the StationeersLua VS Code extension.
- Replacing `sumneko.lua` as the general Lua language service.
- Expanding StationeersLua's active editor/wireless visibility from the custom
  mod.
- Promising atomic Lua writes the upstream API does not provide.

## Decisions

- The bridge supplies global player-curated discovery; StationeersLua supplies
  operations only for its own reported scope.
- ReferenceId equality is the sole cross-service correlation mechanism.
- Direct Lua housings require exact chip `ref_id` and `housing_ref_id`
  equality. Composite Scripted Screens are represented by the bridge as an
  explicitly marked motherboard housing identity; they may resolve only when
  exactly one current StationeersLua Lua record has that `housing_ref_id`.
  The upstream record's `ref_id` is then used for REST operations. Exact pairs
  take priority, and names/network labels are never correlation inputs.
- The 2026-07-29 post-restart live probe observed the composite rule working
  without a selected editor: Screen 1 mapped housing `1626` to Lua chip `1702`
  and Screen 2 mapped housing `1589` to Lua chip `1590`, with one candidate
  each in the active wireless scope.
- The user then manually confirmed the corrected Screen accessibility and
  explorer workflow in a Development Extension Host. Packaged-extension
  installation remains a separate release check.
- Our extension is the sole required Stationeers workflow client.
- The live 2026-07-29 probe confirms that `/api/chips` can report multiple Lua
  chips in a wireless scope, while `/api/editor` separately reports the
  selected chip. The bridge must therefore retain global discovery and use the
  StationeersLua `ref_id`/`housing_ref_id` pair only for capability correlation;
  it must not restrict the tree to the selected editor or wireless chip.
- Lua Push is an explicitly best-effort operation through
  `PUT /api/chips/{refId}/code`. It does not claim P3.06-equivalent conflict
  safety. An exact selected editor target uses `mode=editor_then_chip` only
  while `/api/editor` reports `editor_open: true` and both its chip and housing
  ReferenceIds match the target. A chip accessible through the Wireless
  Development Board or another network scope uses `mode=chip`. Closed-editor
  or partial/stale selected IDs never qualify for editor mode. `editor_only`
  is not used by the normal Push command because it updates only the editor
  draft rather than exporting to the chip.
- A globally bridge-discovered Lua chip remains visible when StationeersLua
  cannot currently access it. The explorer shows a distinct signal state and
  explains that the user must select it in an IC editor or connect the Wireless
  Development Board to that network before Pull, Compare, or Push is available.
