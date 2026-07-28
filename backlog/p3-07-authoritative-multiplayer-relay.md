# P3.07 — Authoritative multiplayer relay

## Status and dependencies

- **Status:** blocked until P3.02 proves the RPC/authority path and local
  read/write is complete
- **Depends on:** [P3.02](p3-02-game-api-feasibility-probes.md),
  [P3.04](p3-04-bridge-protocol-readonly.md),
  [P3.06](p3-06-conflict-safe-ic10-sync.md)
- **Blocks:** supported multiplayer release
- **AI execution size:** large security-sensitive game-mod slice

## Goal

Make the game server/host authoritative for RemoteNetwork discovery and IC10
source operations while keeping the IDE endpoint on the local player's client.

```text
VS Code
  <-> authenticated loopback bridge in player client
  <-> authenticated in-game mod RPC
  <-> authoritative host/dedicated-server bridge mod
  <-> world and chips
```

Single-player uses the same command model with an internal short circuit.
Dedicated servers run the authority component but do not publish the loopback
HTTP listener.

## Context an agent must load

- P3 epic authority/security rules
- P3.02 verified game RPC and player identity evidence
- P3.04 bounds and protocol errors
- P3.06 atomic write handler
- Stationeers server admin/config and logging conventions for the supported
  version

## Authority and permissions

Define server configuration and per-player effective capability:

- bridge disabled;
- discovery/read;
- IC10 write own/assigned targets, if the game provides a verified ownership
  concept;
- IC10 write any exposed target; and
- server administrator.

Do not invent ownership from proximity, label, scope, or the local client's
claim. If verified ownership is unavailable, ship only read and explicit
administrator/allowlist write roles.

The server:

- resolves `RemoteNetwork` scopes and chips from its authoritative world;
- binds each RPC to the authenticated game player supplied by the game
  transport;
- checks policy on every request, including after queue delay;
- runs the P3.06 concurrency check and mutation;
- bounds request/response sizes and per-player/global queues;
- supports revocation and a global kill switch; and
- writes a sanitized audit event for every attempted mutation.

Audit fields include timestamp, player identity, world epoch/save metadata safe
for logs, target reference, old/new hashes, permission decision, request ID,
and result. Do not log bearer tokens or source text by default.

## Relay behaviour

- The local client never returns locally observed world data as authoritative
  in multiplayer.
- RPC envelopes carry protocol version, request ID, operation, bounded payload,
  and cancellation/expiry metadata.
- The client verifies response correlation and ignores late responses from an
  old world/session.
- Disconnects cancel or expire pending commands; retries are safe and do not
  duplicate a successful write.
- Discovery events are coalesced and permission-filtered on the server before
  relay.
- An unmodded server produces a clear `server companion required` capability
  state; it must not fall back to unsafe client-side writes.

## Deliverables

1. Versioned in-game RPC messages and contract fixtures.
2. Server authority service plus single-player short circuit.
3. Permission configuration, runtime capability projection, revocation, kill
   switch, and audit log.
4. Client relay integrated behind the existing loopback API; the VS Code
   contract should change only through advertised capabilities/errors.
5. Hosted and dedicated multiplayer test matrix, including two simultaneous
   IDE users.
6. Server administrator and player troubleshooting documentation.

## Validation and evidence

Run C# unit/contract tests and real:

- single-player;
- listen-server host;
- remote player on listen server;
- dedicated server;
- unmodded server;
- read-only and write-enabled players;
- mid-request revoke/disconnect/world-change; and
- simultaneous stale writes from two users.

Record server tick cost, per-player/global queue behaviour, relay latency,
payload sizes, audit output, and denial paths. Sanitize identities and
addresses before committing evidence.

## Acceptance criteria

- [ ] Remote-client world reads and writes occur only on the authoritative
      server/host.
- [ ] The public/dedicated server exposes no IDE HTTP/WS listener.
- [ ] Every operation is bound to authenticated game player identity and
      current permission.
- [ ] An unmodded server fails closed with actionable UI.
- [ ] Revocation and global disable take effect without restarting VS Code.
- [ ] Concurrent writes preserve P3.06 conflict behaviour.
- [ ] Queues are bounded per player and globally; one user cannot stall the
      server or other users.
- [ ] Every attempted write has a sanitized audit record.

## Stop conditions

- Stop if the game RPC cannot provide trustworthy player identity or execute on
  the authoritative process.
- Stop rather than deriving permission from client-provided names or IDs.
- Stop for security review if replay/idempotency cannot distinguish a retried
  request from a new mutation.

## Non-goals

- Direct IDE-to-dedicated-server access.
- TLS/public endpoint administration.
- IC10 main-game-loop debugging.
- Reusing StationeersLua private internals for bridge authority.

## Decisions

- The local client relay is the only multiplayer IDE path in this epic.
- Permission capabilities are projected into the ordinary bridge handshake.
