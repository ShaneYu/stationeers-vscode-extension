# P3.02 — Game API feasibility probes

## Status and dependencies

- **Status:** complete — downstream gates recorded; multiplayer remains blocked
- **Depends on:** no code dependency; follow the [P3 epic evidence
  policy](p3-00-live-integration-epic.md)
- **Blocks:** P3.03, P3.04, P3.06, and P3.07
- **AI execution size:** bounded spike; it ends in a go/block decision, not
  production features

## Goal

Prove the exact supported Stationeers, BepInEx, and StationeersLaunchPad hooks
needed by the bridge before committing to a production mod architecture.
Replace hypotheses with reproducible game evidence and legal compile-time
contracts.

## Context an agent must load

- [P3 epic](p3-00-live-integration-epic.md)
- `data/generated/devices.json` entries for `StructureLogicMemory`
- `data/generated/resources.json` entries for `ItemKitLogicMemory`
- current repository build/release conventions
- the locally installed game's public metadata and configured mod SDK only when
  the user has put that installation in scope
- StationeersLua documentation for authority and chip reference behaviour

Do not commit proprietary assemblies or decompiled game source. Prefer a local
MSBuild property for the game installation and legal stubs/contracts for CI.

## Probe matrix

Create the smallest diagnostic mod/project capable of answering each row:

| Probe | Required evidence |
| --- | --- |
| Loader lifecycle | Exact plugin entrypoint, load/unload behaviour, supported versions, and a clean log |
| Main-thread dispatch | Confirm where world reads and writes may execute and how off-thread bridge work returns safely |
| Prefab registration | Clone/derive a distinct Logic Memory-like kit and structure without mutating the vanilla prefab |
| Placement and recipe | Confirm two data ports, labeller support, recipe registration, and passive power behaviour |
| Save/load | Confirm the placed custom prefab and its labeller name survive world save/reload |
| Network traversal | From each port, obtain an authoritative physical data-network handle and enumerate attached IC housings/chips |
| Duplicate topology | Observe one anchor on each port, multiple anchors on one network, bridged networks, disconnect/reconnect, and world reload |
| Chip classification | Distinguish IC10 from StationeersLua chips without requiring StationeersLua to be installed |
| IC10 source | Read and write source at a safe tick boundary; determine compile/validation and change notification behaviour |
| Source concurrency | Determine which game value can seed a source hash/version and how external in-game edits are observed |
| Authority | Demonstrate single-player, host, remote client, and dedicated-server ownership of reads/writes |
| In-game RPC | Demonstrate a client request reaching the authoritative host/server mod and a bounded response returning |
| Optional mod detection | Detect StationeersLua without a hard assembly dependency and correlate its visible chip reference to the game housing reference |

Use test worlds containing named and unnamed anchors, IC10 and Lua chips,
duplicate labels, same-network aliases, and same-label separate networks.

## Deliverables

1. Add a minimal C# mod solution with reproducible local build instructions and
   no proprietary binaries.
2. Add diagnostic commands/logging behind a development-only flag.
3. Write `docs/live-integration/feasibility-report.md` containing:
   - version matrix;
   - exact verified type/member/event/RPC contracts;
   - results for every probe;
   - measured enumeration and mutation costs;
   - known failure modes; and
   - `GO`, `GO WITH CONSTRAINTS`, or `BLOCKED` for P3.03, P3.04, P3.06, and
     P3.07 separately.
4. Store sanitized representative logs or machine-readable probe results under
   `docs/live-integration/evidence/`.
5. Add a threat boundary note covering loopback client, game client, and
   authoritative server.

## Validation and evidence

The item must define exact C# restore/build commands once the project exists.
Run repository checks affected by the scaffold plus game probes for every
supported topology. Record failures as results; do not make a probe pass by
catching and hiding an exception.

For performance, record:

- entity/anchor/chip counts;
- total enumeration duration;
- maximum single-tick main-thread work;
- allocations if measurable;
- source read/write duration; and
- RPC payload size and round-trip latency.

The evidence is comparative discovery data, not a final release budget.

## Acceptance criteria

- [x] Every probe row has reproducible evidence or an explicit blocker.
- [x] All production game type/member names proposed by the report were
      observed in the supported build.
- [x] A custom test prefab survives save/load without changing vanilla Logic
      Memory.
- [x] Network traversal and IC10 source access work without StationeersLua.
- [x] Remote-client mutation is shown to execute on the authoritative
      host/server, or P3.07 is marked blocked.
- [x] The project compiles without committed proprietary assemblies.
- [x] Downstream tasks have explicit `GO`/`BLOCKED` gates.

## Stop conditions

- Stop if the required custom-prefab, network, source, or authority hook cannot
  be verified. Do not replace it with an unrestricted reflection scan in
  production.
- Stop for user action if no legally usable local game references are
  available.
- Stop and record the exact version mismatch if a probe only works on a
  different game/mod-loader version.
- Do not implement the production REST server, final device, or VS Code UI in
  this spike.

## Non-goals

- Shipping a user-facing mod.
- Stabilizing protocol schemas.
- Supporting direct IDE connections to dedicated servers.
- Implementing or configuring MCP.

## Decisions

- All game-internal assumptions are gates, not coding prompts.
- Reflection may be useful inside a diagnostic probe, but production reflection
  requires a separately documented compatibility and failure strategy.
