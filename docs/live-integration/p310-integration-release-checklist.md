# P3.10 integration hardening and release checklist

This is the execution checklist for the bridge, the VS Code live workflow, and
the separately packaged bridge mod. It is a gate record, not a claim that the
runtime is complete. A fixture or unit result proves contract behaviour only;
it does not prove game authority, save safety, or multiplayer behaviour.

## Evidence rules

- Record one result per check: `pass`, `fail`, `blocked`, or `not-run`.
- Link the result to a committed sanitized evidence file or CI/build URL.
- Keep raw logs in the ignored local evidence directory until redacted.
- Never commit tokens, source text, player/server identity, absolute paths,
  save names, network addresses, or proprietary game assemblies.
- `observed-with-blocker` is not a pass. A blocker must name the missing runtime
  observation and the next manual sequence.
- Real-game acceptance requires a real supported game/mod installation and a
  captured result from the required sequence. Compilation, mocks, OpenAPI
  validation, and screenshots of a fixture UI are insufficient.

## Gate 0 — scope and package inventory

- [ ] Record exact extension, bridge, game, BepInEx, LaunchPad,
      StationeersLua, `sumneko.lua`, OS, architecture, commit, and build inputs.
- [ ] Verify the extension and bridge are separate artifacts with explicit
      compatibility metadata.
- [ ] Verify the VSIX includes `sumneko.lua`, does not depend on
      the compatible StationeersLua package, and exposes one live workflow.
- [ ] Inspect package contents for proprietary assemblies, saves, source,
      tokens, local paths, and raw diagnostic evidence.
- [ ] Exercise clean install, previous supported schema/protocol upgrade,
      disable/uninstall, and loading a save with the bridge disabled.
- [ ] Check canonical and legacy workspace filenames through activation,
      schemas, templates, and documentation. Legacy files must not be silently
      renamed or duplicated.

## Gate 1 — bridge authentication and boundary checks

Automated/fixture checks against `docs/live-integration/bridge/v1/openapi.json`:

- [ ] Missing, malformed, expired, revoked, and wrong tokens fail with a safe
      denial; no protected response is returned.
- [ ] Tokens are generated with sufficient entropy, stored only in secret
      storage/configuration intended for secrets, revocable, and absent from
      workspace files, URLs, errors, telemetry, and routine logs.
- [ ] Local VS Code auto-pairing discovers `/pair` only on loopback, stores the
      returned token in SecretStorage, and falls back to explicit pairing when
      the game is unavailable; the bridge routes remain bearer-authenticated.
- [ ] The listener binds to loopback only. No port scan, public bind, or
      dedicated-server IDE listener is permitted.
- [ ] Origin policy, request method, content type, request ID, body size,
      source length, query length, and WebSocket frame/queue limits are tested.
- [ ] Unknown fields, duplicate/conflicting correlation IDs, replayed requests,
      late responses, and malformed JSON fail safely and do not mutate state.
- [ ] Logs retain request/result metadata and hashes where useful but redact
      bearer tokens, source text, absolute paths, and identities.

Minimum fixture assertions:

```text
GET /hello without Authorization                    -> denied
GET /scopes with invalid/revoked Authorization      -> denied
PUT source with unknown field                       -> 400, no mutation
PUT source over declared limit                      -> 413, no mutation
old-world response after reconnect                  -> ignored by client
WebSocket/event overflow                            -> bounded, resync required
```

## Gate 2 — conflict-safe IC10 writes

- [ ] Pull records `worldEpoch`, authoritative chip identity, version, and
      SHA-256 before an export.
- [ ] The mutation boundary re-resolves the chip, checks language and
      permission, compares both expected version and hash, validates source
      limits/game rules, then applies once.
- [ ] Stale version, stale hash, stale world, replaced chip, denied permission,
      Lua target, invalid source, and oversized source each return the documented
      error and leave the target unchanged.
- [ ] A `409` includes safe current metadata for a three-way diff where policy
      permits. There is no undocumented force or last-write-wins path.
- [ ] Retry after timeout is idempotent: a successful write is not duplicated;
      a late response cannot authorize a new world/session.
- [ ] Pull, compare, build, confirmation, drag/drop, and menu export use the
      same tested pipeline. Saved mappings contain selectors only, never tokens,
      epochs, or session IDs.

Required deterministic fixture sequence:

```text
pull A -> edit in game -> push A with old version/hash -> 409/no mutation
refresh -> show exact target and diff -> explicit confirmation -> one success
reload world or replace chip -> push old epoch/identity -> 410/no mutation
attempt Lua target or denied player -> 403/no mutation
```

## Gate 3 — authoritative relay and fail-closed multiplayer

- [ ] Run the matrix: single-player, listen-server host, remote listen-server
      player, dedicated server, unmodded server, read-only player, write-
      enabled player, permission revoke, disconnect, world change, and two
      simultaneous stale writers.
- [ ] In multiplayer, discovery and writes are resolved and authorized by the
      host/server. Client-observed data is never presented as authoritative.
- [ ] Every operation is bound to the authenticated game player and policy is
      rechecked after queue delay.
- [ ] Queues are bounded per player and globally; slow clients cannot stall the
      server. Cancellation, expiry, correlation, and reconnect/resync are
      recorded.
- [ ] Revocation and the global kill switch take effect without restarting VS
      Code. Audit records are sanitized and cover every attempted write.
- [ ] An unmodded server reports `server companion required` (or the exact
      advertised equivalent), disables mutation, and does not fall back to
      client-side writes.

This gate is blocked by the current evidence until the real-game matrix is
captured. Do not promote the fixture or unit result to runtime acceptance.

## Gate 4 — workspace and extension-host limitations

- [ ] Local workspace host: connect only to the configured loopback endpoint;
      verify pairing, refresh, pull/compare, and cancellation on deactivation.
- [ ] SSH, WSL, container, and Codespaces hosts: show the documented
      loopback/port-forwarding limitation and do not contact the wrong machine.
      Mark support `unsupported` or `experimental` unless extension-host tests
      prove the forwarding contract.
- [ ] Multi-root, virtual/read-only, untrusted, and non-file URI workspaces:
      verify safe pull destination, no direct filesystem bypass, and actionable
      write restrictions.
- [ ] Move the native view to the Secondary Sidebar; verify command/Quick Pick
      equivalents, keyboard operation, high contrast, zoom, and critical
      screen-reader labels.
- [ ] Keep bridge and StationeersLua connection/diagnostic states independent;
      missing or mismatched optional Lua integration must not change bridge
      authority or silently enable a capability.

## Gate 5 — performance, recovery, and release decision

- [ ] Publish measured budgets before recording results for discovery,
      invalidation, source pull/push, queues, reconnect, extension activation,
      tree refresh, memory, and simulator/Lua tests.
- [ ] Exercise representative small/large/adversarial worlds, topology churn,
      duplicate labels, aliases, many anchors, slow clients, and reconnects.
- [ ] Recover from port collision, token revocation, world unload/reload,
      server travel, device/cable/chip changes, process restarts, optional Lua
      restart/version mismatch, missing `sumneko.lua`, permission changes, and
      extension deactivation.
- [ ] Stop on public bind, credential leak, unbounded queue, recurring hitch,
      save corruption, authority/conflict failure, or proprietary package
      content.
- [ ] Complete the release report and package hash/content audit. Release only
      when every required gate is `pass` and the real-game acceptance status is
      `observed` with no unresolved blocker.

## Required real-game evidence

The evidence packet must include sanitized records for:

1. single-player pull → in-game edit → rejected stale push → refreshed diff →
   confirmed successful push;
2. world reload and chip replacement rejection;
3. listen-server host and remote player authority/permission behaviour;
4. dedicated-server relay and confirmation that no IDE listener is exposed;
5. unmodded-server fail-closed behaviour;
6. two simultaneous stale writers and bounded queue/denial behaviour; and
7. local, remote-workspace, and untrusted/read-only extension-host outcomes.

Until all applicable records exist, the release report must say `blocked` or
`not-run`; it must not say accepted.
