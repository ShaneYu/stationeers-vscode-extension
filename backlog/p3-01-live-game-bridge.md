# P3.01 — Live game bridge

## Goal

Provide safe, low-latency discovery, pull, push, observation, and eventual live
debugging of IC programs in local and multiplayer Stationeers games.

This is a separately shipped companion system, not an implicit capability of
the VS Code extension.

## Design principles

- The server is authoritative in multiplayer.
- No network listener is exposed publicly by default.
- Reads and writes require explicit game/server permission.
- Human-readable names aid discovery, but stable identity prevents accidental
  writes after rename, duplication, or reconnection.
- Synchronisation uses versions and deltas, not repeated full-world snapshots.
- Game-main-thread work is bounded and measured.
- A stale IDE edit never silently overwrites newer in-game code.

## Components

### Companion mod

Add a C# mod project to this monorepo after a technical spike selects the
supported Stationeers mod loader/API (BepInEx or the currently maintained
equivalent).

The mod:

- discovers IC-capable devices through supported game APIs;
- assigns/exposes stable world identity plus display metadata;
- reads and writes IC source under game-authority checks;
- tracks code/source versions and dirty devices;
- captures subscribed state changes;
- relays multiplayer requests to the authoritative server;
- publishes bounded deltas to a local bridge endpoint.

Do not commit proprietary Stationeers assemblies. Local builds may reference a
configured game installation; CI should use legal public contracts/stubs where
possible.

### Shared bridge protocol

Define a versioned protocol and generate or share models across C#, Rust, and
TypeScript:

- protocol/game/mod/extension versions and capability negotiation;
- discovery and subscription;
- pull and push with expected source version/hash;
- state delta and trace events;
- permission and error responses;
- heartbeat, reconnect, resume, and resynchronisation;
- request IDs, cancellation, rate limits, and payload limits.

Start with a debuggable framed JSON protocol. Move high-volume telemetry to a
compact encoding only after profiling proves JSON is the bottleneck.

### VS Code bridge client

The extension provides:

- connection/pairing status;
- IC explorer grouped by world, network, and housing;
- fuzzy search by labeller name, prefab, reference ID, and stable ID;
- pull into a file or compare with an open file;
- push built code with preview and conflict checks;
- explicit mapping from a workspace program to one or more live ICs;
- subscriptions only for visible/debugged values;
- clear read-only mode.

## Identity and naming

Players should give housings useful labeller names, but names are not unique or
stable enough for writes. Represent an IC using:

- stable world/save identifier;
- stable entity/device identifier supplied by the authoritative game;
- current labeller/display name;
- prefab and reference ID;
- coarse location/network metadata where legally and technically available;
- current source hash/version.

If the game cannot guarantee stable entity IDs across save/load, maintain a
mapping with fingerprint and require reconfirmation when ambiguous.

## Local single-player connection

Preferred default:

```text
VS Code <-> authenticated loopback IPC <-> companion mod <-> local game
```

Use a loopback-only transport or operating-system IPC. Pair with a short-lived
code/token displayed in game. Do not accept arbitrary browser origins or
unauthenticated local processes.

## Multiplayer and dedicated servers

### Recommended default: authenticated client relay

```text
VS Code
  <-> loopback bridge in the player's client mod
  <-> existing authenticated game connection
  <-> server companion mod
  <-> authoritative IC state
```

Advantages:

- no extra public IDE port on the dedicated server;
- reuses the player's authenticated game session;
- the server can apply its normal identity and role checks;
- works through the same NAT/firewall path as the game;
- local IDE traffic remains local.

The server mod defines permissions such as:

- bridge disabled;
- read-only;
- write own/assigned ICs;
- write any IC;
- observe runtime state;
- live debug/control;
- administrator.

Every write should include player identity, target stable ID, old/new hash,
result, and timestamp in the server audit log.

### Optional direct server endpoint

Headless administration may eventually need IDE-to-server access without a
running game client. This is opt-in and requires:

- a separate bind address/port disabled by default;
- TLS or a secure tunnel;
- scoped, revocable tokens or mutual authentication;
- IP allowlists and rate limits;
- no reliance on labeller name for authority;
- complete audit logging.

Do not make direct remote connection part of the first release.

## Conflict-safe synchronisation

Every pull returns a source version and content hash. Every push sends the
expected version/hash:

- matching version: apply atomically and return the new version;
- mismatch: reject and provide current source for a three-way diff;
- deleted/replaced target: reject and require remapping;
- compile failure: reject by default, with an explicit privileged override only
  if the game itself permits invalid code;
- multi-target deployment: preflight all targets and report partial-failure
  semantics before applying.

There is no background two-way overwrite mode. Optional auto-push may operate
only for an explicitly mapped target with conflict protection.

## Performance architecture

Existing mod lag commonly comes from scanning and serialising too much state on
the game thread. The bridge should:

- perform one bounded discovery pass, spread across ticks if needed;
- use dirty/event tracking for IC code and subscribed values;
- never rescan every IC or snapshot the complete world on a timer;
- copy minimal immutable DTOs on the game thread and serialize off-thread;
- coalesce repeated changes to the latest value under backpressure;
- bound messages, queues, subscriptions, and per-tick work;
- batch code writes and apply them at a safe tick boundary;
- make telemetry opt-in by target and field;
- stop producing telemetry when no IDE is subscribed;
- expose queue depth, dropped/coalesced updates, bytes, and game-thread time;
- degrade by reducing telemetry frequency, never by blocking the game loop.

Before feature implementation, build a benchmark harness with representative
small, medium, and very large bases. Establish and publish budgets for:

- idle allocations and game-thread time;
- discovery duration and worst single-tick cost;
- subscribed delta processing;
- code pull/push;
- reconnect/resynchronisation;
- server performance with multiple IDE users.

Release criteria should include no periodic full scan, no unbounded queue, no
observable recurring hitch in the reference large-base benchmark, and no
regression beyond the agreed budgets.

## Security and safety

- Explicit pairing and server permission are mandatory.
- Default to read-only until the user requests a write.
- Show exact target name and stable identity before first push.
- Never execute arbitrary filesystem or shell commands from bridge messages.
- Validate all lengths, indexes, enum values, and protocol versions.
- Limit source and telemetry payload sizes.
- Protect against replay and cross-world stale requests.
- Audit multiplayer writes.
- Provide a server kill switch and per-player revocation.

## Delivery phases

### Phase A — feasibility and measurement

- select supported mod loader/API;
- prove safe IC enumeration and source read/write;
- measure existing full-scan and event-driven approaches;
- validate server-authoritative RPC;
- write an architecture decision record and threat model.

### Phase B — local read-only

- pairing;
- discovery;
- pull/compare;
- version handshake;
- performance metrics.

### Phase C — local conflict-safe write

- build then push;
- optimistic concurrency;
- diff/confirmation;
- rename and replacement handling.

### Phase D — multiplayer client relay

- server mod;
- roles and audit;
- read/push through the authenticated game connection;
- multi-user conflict tests and load benchmarks.

### Phase E — subscribed state and live debugging

- selected state deltas;
- live scopes/watches;
- pause/control only where the server can do so safely;
- capture real-game conformance traces;
- simulator/live comparison.

### Phase F — optional direct administration

- secure direct endpoint for headless operators;
- separately documented deployment and threat model.

## Acceptance criteria

- [ ] An ADR documents the chosen mod loader, supported game versions, and API
      limitations.
- [ ] The protocol is versioned and capability-negotiated.
- [ ] Local endpoints bind only to loopback/IPC and require pairing.
- [ ] Multiplayer state is read/written only by the authoritative server.
- [ ] The default multiplayer path uses the authenticated client relay.
- [ ] Names are never the sole identity or concurrency key.
- [ ] Stale pushes are rejected with a usable diff workflow.
- [ ] Permissions and audit logs cover every multiplayer write.
- [ ] Discovery and observation use dirty tracking/subscriptions, not periodic
      full snapshots.
- [ ] Queues and per-tick work are bounded with visible metrics.
- [ ] Large-base and multi-user performance budgets are published and enforced.
- [ ] The bridge can be disabled globally and per player.
- [ ] Mod and extension version mismatches fail safely with upgrade guidance.

## Dependencies

- [P0.02](p0-02-simulator-conformance.md) consumes real-game captures.
- [P1.04](p1-04-deployment-build-pipeline.md) supplies safe deployable code.
- [P2.01](p2-01-reversible-debugging.md) defines trace/state concepts.

## Non-goals for the first release

- Exposing an unauthenticated public WebSocket.
- Direct IDE-to-dedicated-server access.
- Continuous snapshots of all ICs and devices.
- Transparent automatic overwrite of changed in-game code.
- Full live reverse debugging.

## Decisions

- The recommended multiplayer architecture is IDE-to-local-client bridge,
  relayed through the authenticated game connection to an authoritative server
  mod.
- Direct remote server access is a later, opt-in administrative feature.
- Event-driven deltas and bounded subscriptions are release requirements, not
  optional optimisations.
