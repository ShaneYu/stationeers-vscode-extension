# P3.03 — RemoteNetwork game device

## Status and dependencies

- **Status:** implementation slice complete — `GO WITH CONSTRAINTS` from P3.02
  supports prefab/recipe/localization packaging, save/load-safe cloning, and
  main-thread discovery DTO/grouping; duplicate-topology and multiplayer
  runtime evidence remain open
- **Depends on:** [P3.02](p3-02-game-api-feasibility-probes.md)
- **Blocks:** P3.04 and P3.05
- **AI execution size:** medium game-mod vertical slice

## Goal

Add a deliberately placeable and labelable `RemoteNetwork` device that marks
which physical data networks the bridge may expose. It behaves like a passive
discovery beacon, not a memory value or network router.

## Context an agent must load

- [P3 epic decisions](p3-00-live-integration-epic.md)
- P3.02 feasibility report and exact supported API contracts
- generated Logic Memory device and recipe records
- the C# mod scaffold, tests, packaging, and localization conventions

## Device contract

- Register distinct kit and structure prefabs, provisionally
  `ItemKitRemoteNetwork` and `StructureRemoteNetwork`.
- Reuse the supported Logic Memory model, placement rules, two data ports, and
  labeller interaction only through the verified mechanism from P3.02.
- Change the visible/localized product and faceplate text to
  `Remote Network`/`Network`.
- Match the supported Logic Memory recipe: 1 g gold plus 1 g copper.
- Match Logic Memory's passive power behaviour. The current generated export
  has two data connections and no power connection or power field; reconfirm
  this for the targeted game version rather than adding an artificial draw.
- Remove or disable inherited `Setting` interaction if the verified prefab API
  permits it. It must not be presented as data storage.
- Preserve both ports. A port attached to a physical network is a discovery
  attachment; an unattached port contributes nothing.
- Persist the labeller name through the ordinary world save. Do not create a
  custom persistent network GUID.
- Continue to exist and be structurally discoverable when unpowered.

Never patch or rename `StructureLogicMemory` or its kit globally.

## Discovery index

Add a main-thread-owned index that emits immutable snapshots:

1. Enumerate `RemoteNetwork` instances through the verified lifecycle/index.
2. Resolve each connected port to a physical runtime network handle.
3. Trim the device label.
4. Ignore unlabeled attachments for deployable scopes, but emit a visible
   configuration warning identifying the device.
5. Group by `(worldEpoch, physicalNetworkHandle, exactTrimmedLabel)`.
6. Enumerate compatible IC housings/chips once per physical network and
   reference the resulting chip records from every label alias.

Expected cases:

- same physical network + same label -> one scope with `anchorCount > 1`;
- same physical network + different labels -> separate scopes with the same
  chips;
- same label + different physical networks -> separate scopes that the client
  disambiguates;
- one device with both ports on the same network -> one scope and two
  attachments, not duplicated chips;
- one device with ports on different networks -> one label producing two
  distinct scopes.

## Deliverables

1. Production prefab/recipe/localization/packaging implementation.
2. Incremental anchor lifecycle index with bounded reconciliation.
3. Internal DTOs for scope warnings, anchors, networks, and chip summaries.
4. Automated pure grouping tests independent of Unity objects.
5. Game test fixture/checklist covering placement, deconstruction, labelling,
   cabling, duplicate cases, save/load, and world switch.
6. Player documentation explaining that label aliases can intentionally show
   the same chip more than once.

## Validation and evidence

Run the C# build/tests established in P3.02 and repository checks affected by
packaging. Capture sanitized game evidence for every expected grouping case.
Record idle and topology-change main-thread costs for a small and large
anchor/chip fixture.

## Acceptance criteria

- [x] The device is craftable for exactly the confirmed Logic Memory cost.
- [x] Its localized product name communicates `Remote Network`, and vanilla Logic Memory is
      unchanged.
- [ ] Both data ports and labeller work after save/reload (runtime checklist
      still required for the production prefab).
- [x] The device is passive and does not add a power draw.
- [x] Scope grouping matches the supported deterministic cases in pure tests.
- [x] Empty labels yield actionable warnings and no deployable scope.
- [ ] Device and cable changes update the index without a periodic full-world
      scan.
- [x] No custom persistent scope ID is stored in a save.

## Stop conditions

- Stop if deriving the model also mutates the vanilla prefab or inherited
  behaviour cannot be safely separated.
- Stop if a physical network handle is not stable for the duration required to
  service one request; revise the runtime routing contract before continuing.
- Do not invent power draw, recipe values, or prefab APIs when current evidence
  differs from the backlog.

## Non-goals

- Rendering the VS Code tree.
- Exposing a public API.
- Routing data between physical networks.
- Discovering networks without a `RemoteNetwork`.

## Decisions

- The device name is singular `RemoteNetwork` in code and `Remote Network` in
  user-facing text unless verified game naming constraints require otherwise.
- Label aliases are a feature, not a data-cleanup error.

## Evidence links

- [P3.02 feasibility gate](../docs/live-integration/feasibility-report.md)
- [P3.03 game checklist](../docs/live-integration/p303-remotenetwork-checklist.md)
- [RemoteNetwork mod README](../mods/StationeersBridge.RemoteNetwork/README.md)
- [Pure grouping tests](../mods/StationeersBridge.RemoteNetwork.Tests/Program.cs)
