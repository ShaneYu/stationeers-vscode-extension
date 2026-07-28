# P3.05 — VS Code live network explorer

## Status and dependencies

- **Status:** blocked until the P3.04 contract is stable
- **Depends on:** [P3.03](p3-03-remote-network-device.md),
  [P3.04](p3-04-bridge-protocol-readonly.md)
- **Blocks:** P3.06 and the user-visible parts of P3.08
- **AI execution size:** large TypeScript vertical slice

## Goal

Add a native VS Code tree for live RemoteNetwork scopes and their IC10/Lua
chips, with independent connection status, refresh, search, pull, compare, and
safe file-to-chip drag/drop preparation.

VS Code extensions cannot force a view to open in the Secondary Sidebar. Add a
normal view container or Panel contribution and document **Move View** so users
can place it on the right.

## Context an agent must load

- [P3 epic capability matrix](p3-00-live-integration-epic.md)
- P3.04 machine-readable contract and fixtures
- `packages/vscode/package.json`
- `packages/vscode/src/extension.ts` and existing lifecycle/services
- existing URI, multi-root, output, status-bar, and testing patterns
- current VS Code `TreeDataProvider`, `createTreeView`, context value,
  SecretStorage, and `TreeDragAndDropController` APIs

## Connection model

Create a bridge client service with an explicit state machine:

```text
disabled -> discovering -> pairing -> connected -> stale/reconnecting
                          \-> incompatible
                          \-> denied
```

- Default URL: `http://127.0.0.1:3032`, configurable as a machine setting.
- Store the token in `ExtensionContext.secrets`.
- Probe only the configured loopback endpoint. Never port-scan.
- Keep bridge and future StationeersLua states independent. Do not model one
  generic boolean named `connected`.
- Cancel in-flight requests on deactivation, URL/token changes, world epoch
  changes, and explicit disconnect.
- Apply timeouts, exponential reconnect with jitter, and a user-visible manual
  retry.
- Redact token, absolute user paths, and source text from routine logs.

The extension currently declares `extensionKind: ["workspace"]`. For this
item, explicitly support local workspace extension hosts. In SSH, WSL,
containers, or Codespaces, show a clear loopback/port-forwarding limitation
rather than contacting the wrong machine. A split UI/workspace extension is a
P3.10 decision.

## Tree model

Use a `TreeDataProvider` with stable per-snapshot keys:

```text
Live Stationeers
  Bridge status / world
  Greenhouse
    Climate Controller             IC10
    Irrigation Supervisor          Lua
  Greenhouse · Area 3
    ...
  Configuration warnings
    Unlabeled Remote Network ...
```

- Sort scopes by name then disambiguator; sort chips by housing name then
  reference ID.
- Preserve expansion/selection where snapshot identity still exists.
- Display the same chip under every intentional label alias.
- Use description/icon/context values for language, power, read/write/debug
  eligibility, staleness, and ambiguity. Do not encode all state into long
  labels.
- Refresh from event invalidation with debounce and a manual command.
- Show empty, loading, permission, incompatible-version, and disconnected
  states as actionable tree items.
- Add fuzzy Quick Pick search over scope label, housing label, prefab, language,
  and reference ID without persisting session-only scope IDs.

## Commands

Initial read-only commands:

- Connect/disconnect/pair bridge.
- Refresh live networks.
- Pull chip source into a newly chosen file.
- Compare chip source with an open or selected `.ic10`/`.lua` file.
- Copy housing/reference ID.
- Open bridge logs/diagnostics.

Context menus must be capability-driven. P3.06 later enables IC10 export; P3.08
enables eligible StationeersLua actions.

## Drag and drop

Register a `TreeDragAndDropController` that accepts VS Code `files` and
`text/uri-list` onto compatible chip nodes.

In this read-only item, a drop performs:

1. URI/language validation;
2. target and current world revalidation;
3. local build/diagnostic preflight where implemented;
4. a compare/preview; and
5. a message that deployment requires P3.06 or an unavailable capability.

It must not write source yet. Build the drop pipeline so P3.06 adds one explicit
confirmed mutation step rather than duplicating validation.

## Feature context keys

Set documented context keys from handshake and selection state, for example:

```text
stationeers.bridge.connected
stationeers.bridge.canReadIc10
stationeers.bridge.canWriteIc10
stationeers.stationeersLua.connected
stationeers.liveChip.language
stationeers.liveChip.luaDebugEligible
stationeers.liveChip.stale
```

Names may be refined to match repository conventions, but tests must prove
that menus disable and re-enable correctly as capabilities change.

## Deliverables

1. Typed, cancellable bridge client isolated from VS Code rendering.
2. Native view, status, commands, context menus, search, and DnD preflight.
3. SecretStorage pairing and safe settings.
4. Fixture-driven client/tree tests plus extension-host coverage for critical
   commands.
5. User documentation including Secondary Sidebar placement and remote
   workspace limitations.

## Validation and evidence

Run:

```text
npm run test --workspace packages/vscode
npm run check --workspace packages/vscode
npm run test:extension-host --workspace packages/vscode
npm run check
```

Use a fixture HTTP/WS server to test normal, stale, malformed, slow, denied,
incompatible, resync, world-change, and duplicate-scope responses. At least one
manual test must use the real P3.04 service.

## Acceptance criteria

- [ ] The tree accurately renders all P3.03 duplicate/dedupe cases.
- [ ] Tokens never enter workspace configuration or logs.
- [ ] Source pull/compare uses VS Code URI APIs and works in multi-root
      workspaces.
- [ ] Drag/drop validates and previews but cannot mutate before P3.06.
- [ ] Menu and status state follows capabilities without extension reload.
- [ ] Reconnect/world changes cancel stale work and preserve no stale authority.
- [ ] Remote-workspace limitations are detected and explained.
- [ ] The user can move the native view to the Secondary Sidebar.

## Stop conditions

- Stop rather than introducing direct Node filesystem access for workspace
  resources.
- Stop if a command would use `scopeId`, label, or stale cached identity as
  mutation authority.
- Do not implement a custom webview just to control right-side placement.

## Non-goals

- IC10 writes.
- Owning StationeersLua's connection or debugger.
- Automatic deployment on save.
- MCP configuration.

## Decisions

- Native tree/commands are preferred over a custom explorer webview.
- Bridge status and StationeersLua status are separate state machines.
