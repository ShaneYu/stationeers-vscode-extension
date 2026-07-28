# P3.04 — Bridge protocol and local read-only service

## Status and dependencies

- **Status:** blocked until P3.02 records `GO` for world/source reads and P3.03
  supplies the discovery index
- **Depends on:** [P3.02](p3-02-game-api-feasibility-probes.md),
  [P3.03](p3-03-remote-network-device.md)
- **Blocks:** P3.05, P3.06, and P3.07
- **AI execution size:** large; split protocol/fixtures from C# transport if
  useful

## Goal

Expose authenticated, versioned, read-only RemoteNetwork discovery and IC10
source through a lightweight loopback HTTP service, with a bounded WebSocket
event stream for incremental updates.

The bridge is independent of StationeersLua. It may report a discovered chip as
Lua, but it must return an explicit unsupported-capability response if asked
for Lua source.

## Context an agent must load

- [P3 epic](p3-00-live-integration-epic.md)
- P3.02 verified game-thread and source contracts
- P3.03 immutable discovery DTOs
- current TypeScript/Rust serialization and fixture conventions
- OWASP guidance relevant to loopback services, WebSocket origins, bearer
  tokens, payload bounds, and request smuggling for the selected C# server

## Transport contract

- Bind to `127.0.0.1` and `::1` only by default. Never bind to all interfaces
  because localhost binding failed.
- Use a configurable port, initially `3032`, and report an actionable port
  collision. Do not probe arbitrary remote hosts.
- Require a bearer token generated/approved in game. Store it only in VS Code
  `SecretStorage`; never in workspace settings, logs, URLs, or source mappings.
- Reject unsupported browser origins. Do not use permissive CORS.
- Apply request, header, source, queue, connection, and rate limits before
  enqueueing game-thread work.
- Perform HTTP parsing, authentication, JSON serialization, and socket I/O
  off-thread. Only verified world access enters the bounded main-thread queue.
- Make the WebSocket optional. A client can always recover by fetching a fresh
  snapshot.

Use one versioned prefix. The initial contract is `/bridge/v1`.

## Recommended read schemas

All IDs that originate from Stationeers are JSON strings even if the game uses
an integer type.

### Handshake

`GET /bridge/v1/hello`

```json
{
  "apiVersion": "1.0",
  "bridgeVersion": "0.1.0",
  "gameVersion": "verified-at-runtime",
  "instanceId": "opaque-process-id",
  "role": "singlePlayer",
  "world": {
    "loaded": true,
    "epoch": "opaque-world-load-token",
    "revision": "42"
  },
  "capabilities": {
    "scopeDiscovery": true,
    "ic10SourceRead": true,
    "ic10SourceWrite": false,
    "multiplayerRelay": false,
    "eventStream": true
  },
  "mods": {
    "stationeersLua": {
      "detected": false,
      "version": null
    }
  },
  "limits": {
    "maxSourceBytes": 65536,
    "maxRequestsPerSecond": 10
  }
}
```

`instanceId`, `world.epoch`, and `world.revision` are opaque strings.
`world.epoch` changes on every world load/unload, including reloading the same
save. `revision` changes when the discovery snapshot changes.

### Discovery snapshot

`GET /bridge/v1/scopes`

```json
{
  "worldEpoch": "opaque-world-load-token",
  "revision": "42",
  "scopes": [
    {
      "scopeId": "opaque-session-routing-handle",
      "name": "Greenhouse",
      "disambiguator": "Area 3",
      "anchorCount": 2,
      "chipIds": ["9007199254740993"]
    }
  ],
  "chips": [
    {
      "chipId": "9007199254740993",
      "housingReferenceId": "12345678901234567",
      "housingName": "Climate Controller",
      "housingPrefab": "StructureCircuitHousing",
      "chipPrefab": "ItemIntegratedCircuit10",
      "language": "ic10",
      "powered": true,
      "source": {
        "readable": true,
        "writable": false,
        "version": "17",
        "sha256": "lowercase-hex"
      }
    }
  ],
  "warnings": [
    {
      "code": "remote_network_unlabeled",
      "message": "Label this Remote Network to expose its attached network.",
      "anchorReferenceId": "23456789012345678"
    }
  ]
}
```

`scopeId` is valid only for the current `worldEpoch` and is never a mutation
target. `disambiguator` is optional display metadata; it must not expose
sensitive coordinates by default.

### IC10 source

`GET /bridge/v1/chips/{chipId}/source?worldEpoch=...`

```json
{
  "worldEpoch": "opaque-world-load-token",
  "chipId": "9007199254740993",
  "housingReferenceId": "12345678901234567",
  "language": "ic10",
  "version": "17",
  "sha256": "lowercase-hex",
  "source": "alias Sensor d0\n..."
}
```

The response is a coherent main-thread snapshot. P3.06 adds `PUT`; read-only
work must not ship a hidden mutation route.

### Events

`GET /bridge/v1/events` upgrades to WebSocket. Every message uses:

```json
{
  "apiVersion": "1.0",
  "eventId": "105",
  "worldEpoch": "opaque-world-load-token",
  "revision": "43",
  "type": "snapshot.invalidated",
  "data": {
    "reason": "topologyChanged"
  }
}
```

Initial event types should be minimal:

- `world.changed`;
- `snapshot.invalidated`;
- `chip.sourceChanged`;
- `capabilities.changed`; and
- `resync.required`.

Prefer invalidation plus one fresh HTTP snapshot over a complicated delta
contract until profiling demonstrates that snapshot size is a problem.

### Error envelope

```json
{
  "error": {
    "code": "stale_world",
    "message": "The world changed; refresh discovery before retrying.",
    "requestId": "client-generated-id",
    "retryable": true,
    "details": {}
  }
}
```

Use at least:

- `400` malformed/invalid request;
- `401` missing or invalid pairing token;
- `403` denied by policy;
- `404` unknown target;
- `409` revision/source conflict;
- `410` stale world or replaced chip;
- `413` payload too large;
- `423` world loading/not safe to access;
- `429` queue or rate limit reached; and
- `503` bridge/game unavailable.

## Protocol source of truth

Check in a machine-readable schema/OpenAPI contract and golden JSON fixtures.
Generate models only if generation stays deterministic and does not introduce a
runtime dependency. The C# producer and TypeScript consumer must run contract
tests against the same fixtures.

Document additive/minor and breaking/major compatibility rules. Unknown fields
must be ignored where safe; unknown enum values and unsupported major versions
must fail visibly rather than being misinterpreted.

## Deliverables

1. Versioned schema/OpenAPI and golden success/error/event fixtures.
2. C# loopback listener, pairing lifecycle, bounded command dispatcher, and
   sanitized diagnostics.
3. Read-only handlers for handshake, scopes, and IC10 source.
4. Optional bounded event stream with resync semantics.
5. Transport/core tests that do not require Unity plus game integration tests.
6. User/admin documentation for port selection, pairing, token revocation, and
   disabling the service.

## Validation and evidence

Run the C# commands established by P3.02, schema/fixture contract tests, and:

```text
npm run check
npm test
```

Exercise invalid auth, oversized input, slow clients, queue saturation, world
reload during a request, disconnect/reconnect, and a port collision. Capture
queue depth, dropped/coalesced events, bytes, and main-thread duration.

## Acceptance criteria

- [ ] The default listener is reachable only through loopback and requires a
      non-logged token.
- [ ] C# and TypeScript contract tests consume the same fixtures.
- [ ] All game IDs are strings and all session handles become stale on world
      change.
- [ ] Read responses are coherent snapshots created on the game thread.
- [ ] Lua source requests return a capability error, not guessed data.
- [ ] Queue, payload, rate, and connection bounds are tested.
- [ ] A dropped/slow WebSocket client cannot stall the game and can resync.
- [ ] No source mutation route exists yet.

## Stop conditions

- Stop if the selected embedded server cannot enforce loopback binding,
  authentication, limits, and clean shutdown.
- Stop if any serializer touches live Unity objects off-thread.
- Stop if a convenient numeric JSON model would lose 64-bit reference
  precision in JavaScript.

## Non-goals

- Public network binding or TLS termination.
- StationeersLua proxying.
- Source writes or live values.
- Full-world device/network topology.

## Decisions

- REST supplies coherent state; WebSocket supplies invalidation and small
  events.
- Version one favors resync simplicity over fine-grained topology deltas.
