# P3.06 — Conflict-safe IC10 synchronization

## Status and dependencies

- **Status:** IC10 conditional source-sync vertical slice implemented and live
  validated; full backlog acceptance remains open for mappings, build/export,
  drag/drop deployment, and durable evidence.
- **Depends on:** [P1.04](p1-04-deployment-build-pipeline.md),
  [P3.02](p3-02-game-api-feasibility-probes.md),
  [P3.04](p3-04-bridge-protocol-readonly.md),
  [P3.05](p3-05-vscode-live-network-explorer.md)
- **Blocks:** P3.07 and final release
- **AI execution size:** large C#/TypeScript vertical slice

## Goal

Add explicit, previewed IC10 pull/export with optimistic concurrency. An IDE
edit must never silently overwrite newer in-game source or a chip that was
replaced during a world/session change.

### Delivered slice (2026-07-29)

- The bridge supports conditional IC10 source writes using world epoch, target
  identity, expected version, expected SHA-256, source limits, and post-write
  verification.
- Opening a chip creates a named in-memory editor tab. Manual Save, the Push
  command, and the explorer Push action use the same conditional write path.
- Pull refreshes the open tab. Compare is read-only and does not modify the
  tab. A failed conditional write leaves the editor untouched and reports a
  conflict.
- Merge and force-push are intentionally not implemented. Conflict recovery is
  currently Pull/Compare, then edit and retry.

## Context an agent must load

- P3.02 evidence for IC10 source ownership, compile behaviour, and change
  observation
- P3.04 schemas/error rules
- P3.05 compare and drop-preflight pipeline
- P1.04 build output, source-map, and diagnostics
- current workspace trust, confirmation, and filesystem conventions

## Write contract

Add:

`PUT /bridge/v1/chips/{chipId}/source`

```json
{
  "requestId": "client-generated-id",
  "worldEpoch": "opaque-world-load-token",
  "expectedVersion": "17",
  "expectedSha256": "lowercase-hex",
  "source": "deployable IC10 source",
  "sourceSha256": "lowercase-hex"
}
```

Successful response:

```json
{
  "worldEpoch": "opaque-world-load-token",
  "chipId": "9007199254740993",
  "version": "18",
  "sha256": "new-lowercase-hex",
  "applied": true
}
```

The authoritative game/server command must, at one safe mutation boundary:

1. re-resolve the chip in the supplied world epoch;
2. confirm it is still an IC10 target and write permission remains;
3. read current source and calculate/obtain current version/hash;
4. compare both expected version and expected hash;
5. validate source limits and the game's accepted source rules;
6. apply once; and
7. return the new coherent version/hash.

A mismatch returns `409` with current metadata and, only when policy permits,
current source for a three-way diff. A stale world or replaced target returns
`410`. Never implement `force=true` as an undocumented fallback.

If Stationeers exposes no durable source version, the bridge may maintain a
monotonic in-world-session revision backed by content SHA-256. `worldEpoch`
prevents reuse after load. The final algorithm must be documented and tested
against edits made through the in-game editor.

## User workflow

### Pull and live editor (implemented slice)

- Open a chip from the live explorer to create a named in-memory `.ic10` tab.
- Pull refreshes that tab from the game.
- Manual Save and Push attempt a conditional write.
- Compare creates read-only snapshots and never changes the live tab.
- Remember no token or opaque session ID in the editor content.

The original workspace-file pull/export workflow, portable mappings, and
build-before-deploy flow remain future work rather than delivered behaviour.

### Export

1. Resolve the file's explicit saved selector or selected chip.
2. Refresh discovery and reject ambiguity/stale world.
3. Build through P1.04; block on errors or size/line constraints.
4. Fetch current source/version/hash.
5. Show exact scope aliases, housing name, reference ID, old/new hash, and diff.
6. Ask for confirmation on first/new/ambiguous target.
7. Send the conditional write.
8. Report the authoritative result and retain a comparable build artefact.

Drag/drop and the **Export to Chip** context menu use this same pipeline.

### Saved mappings

Persist only a portable human selector, for example:

```json
{
  "remoteNetwork": "Greenhouse",
  "housing": "Climate Controller",
  "language": "ic10"
}
```

Re-resolve every session. If it matches zero or multiple authoritative chips,
prompt. Do not persist bearer tokens, `scopeId`, `worldEpoch`, or treat labels
as authorization.

Auto-push, if ever enabled after this item, must be opt-in per mapping and use
the same conflict checks. It is not required here.

## Multi-target safety

Multi-target deployment is optional. If implemented:

- preflight every target and present the complete plan;
- choose and document atomic-all-or-none or explicit partial-result semantics;
- never describe a partial write as success; and
- return per-target versions/errors.

Do not add multi-target support merely because one file can appear under
multiple scope aliases; aliases may reference the same chip.

## Deliverables

1. Versioned write schema and golden conflict/stale/denied fixtures.
2. Atomic authoritative C# mutation handler and audit-safe logs.
3. TypeScript pull, mapping, compare, build, confirmation, export, and conflict
   resolution flow.
4. DnD/context-menu integration.
5. Tests for in-game edits, replacement, world switch, duplicates, concurrent
   clients, cancellation, build failure, and oversized source.
6. User documentation and recovery guidance.

## Validation and evidence

Run C# tests/game probes, P1.04 tests, extension tests, and:

```text
npm run check
npm test
npm run build
```

Capture a real-game sequence showing pull -> in-game edit -> rejected stale
push -> refreshed diff -> confirmed successful push. Also capture world reload
and chip replacement rejection.

## Acceptance criteria

- [x] Every write validates world, target, permission, version, and hash at the
      authoritative mutation boundary.
- [ ] Stale IDE and in-game edits produce a usable conflict workflow.
- [ ] The P1.04 build is the deployed source; a source file is not silently
      rewritten.
- [ ] Saved mappings are portable selectors and ambiguity is visible.
- [ ] Drag/drop and menu export share one tested pipeline.
- [x] Lua targets cannot enter the IC10 write handler.
- [ ] Routine logs contain hashes/metadata, not complete source or tokens.

## Stop conditions

- Stop if source validation and mutation cannot occur coherently at one safe
  authoritative boundary.
- Stop rather than making label-only or last-write-wins deployment.
- Stop for a product decision before adding any privileged force-write path.

## Non-goals

- IC10 live breakpoint debugging.
- Lua source synchronization; this is now tracked by P3.08 instead of the
  IC10 write handler.
- Background bidirectional synchronization.
- Mandatory multi-target export.

## Decisions

- Both expected version and expected content hash are required.
- A mapping improves selection convenience; it never grants authority.
