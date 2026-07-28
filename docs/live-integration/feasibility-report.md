# Live integration feasibility report

## Decision

P3.02 has a compiling diagnostic mod, exact metadata evidence, a successful
single-player baseline, and a populated two-port save/reload fixture on the
supported local stack. Loader, main-thread dispatch, prefab registration,
printing, placement, labelling, save reconstruction, two-network traversal,
IC10/Lua classification, and read-only source access are runtime-observed.
Reversible IC10 source mutation is also runtime-observed. Duplicate/bridged
topology and multiplayer evidence remain explicit blockers.

| Downstream item | Gate | Reason |
| --- | --- | --- |
| P3.03 RemoteNetwork device | `GO WITH CONSTRAINTS` | The runtime clone printed, placed, labelled, connected on both ports, and survived save/reload. Production work must preserve the prefab-ready registration and isolated build metadata pattern. |
| P3.04 read-only bridge | `GO WITH CONSTRAINTS` | Two distinct physical networks and three chips were traversed authoritatively in 0.3884 ms. Duplicate/bridged/reconnect and larger-world cost fixtures remain required. |
| P3.06 IC10 synchronization | `GO WITH CONSTRAINTS` | Authoritative mutation compiled and restored in 0.3919 ms. `LastEditedId` and `LastEditedBy` remained zero, so synchronization must use expected SHA-256 with a final main-thread re-read; no atomic compare-and-set was observed. |
| P3.07 multiplayer relay | `BLOCKED` | The typed RPC must complete from a remote client on an authoritative host and dedicated server. |

## Supported version matrix

| Component | Observed version |
| --- | --- |
| Stationeers / `Assembly-CSharp` | `0.2.6403.27689` |
| Steam build | `24014594` |
| Unity | `2022.3.62.9860879` |
| BepInEx | `5.4.23.3` |
| StationeersLaunchPad | `0.5.0.0` |
| LaunchPadBooster | `0.2.0.0` |
| StationeersLua (optional) | `0.9.5.0` |

Assembly MVIDs and SHA-256 fingerprints are in
`evidence/installed-metadata-2026-07-28.json`.

## Verified compile-time contracts

The project builds against the installed assemblies with zero warnings. The
runtime contract probe checks these exact names again on load and emits one
record per member.

| Area | Observed contract |
| --- | --- |
| Loader | `MonoBehaviour.OnLoaded(List<GameObject>, ConfigFile, List<Assembly>)` |
| Main thread | `UnityMainThreadDispatcher.Exists()`, `Instance()`, `Enqueue(Action)` |
| Prefabs | `Prefab.OnPrefabsLoaded`, `Find<T>(string)`, `RegisterExisting(Thing)` |
| Identity | `Thing.PrefabName`, `PrefabHash`, `ReferenceId`, `CustomName`, `HasAuthority` |
| World enumeration | `Device.AllDevices.Active()` |
| Port traversal | `Device.GetNetwork(int)` |
| Physical network | `CableNetwork.ReferenceId`, `DataDeviceList` |
| IC housing | `CircuitHousing._ProgrammableChipSlot`, `GetSourceCode()`, `SetSourceCode(string)`, `LastEditedBy` |
| IC10 chip | `ProgrammableChip.GetSourceCode()`, `SetSourceCode(string)`, `LastEditedId`, `CompilationError`, `SendUpdate()` |
| Authority role | `NetworkManager.IsClient`, `IsServer`, `NetworkRole`, `LocalClientId` |
| RPC | `LaunchPadBooster.Networking.INetworkRPC`, `RegisterRPC<T>()`, `CallHost()` |
| Optional Lua | assembly `StationeersLua`; type `StationeersLua.IntegratedCircuitLua`; event `LuaChipRuntimePublicHooks.ChipSourceChanged` |

StationeersLaunchPad’s preferred entrypoint and LaunchPadBooster’s prefab/RPC
surface are documented by the
[code-mod guide](https://stationeerslaunchpad.github.io/docs/modding/codemod/)
and [LaunchPadBooster API examples](https://github.com/StationeersLaunchPad/LaunchPadBooster).
StationeersLua chip/network behavior is documented in its
[getting-started guide](https://orbitalfoundrymodteam.github.io/StationeersLuaDocs/guide/getting-started.html)
and [network guide](https://orbitalfoundrymodteam.github.io/StationeersLuaDocs/api/net-messaging.html).

## Probe implementation

The development mod is under `mods/StationeersBridge.Feasibility/`. All actions
are disabled by default.

- The prefab probe clones `StructureLogicMemory` and `ItemKitLogicMemory`,
  assigns distinct names and hashes, leaves the vanilla objects unchanged, and
  registers an electronics-printer recipe with 1 g gold and 1 g copper.
- The thread probe starts off-thread work and returns through the game’s
  dispatcher, recording all thread IDs.
- The world probe enumerates only `P302-` fixtures and the custom probe prefab,
  follows each port’s physical `CableNetwork`, records duplicate appearances,
  classifies chips, hashes source, and measures duration and allocation delta.
- The source probe is separately opt-in, requires the exact configured IC10
  housing name and `MUTATE_AND_RESTORE_P302_SOURCE` confirmation, runs once per
  chip, appends a valid comment, observes compilation state, and restores the
  original source in a `finally` block.
- The RPC probe uses one 8-byte request and a 21-byte response and records
  caller ID, handler thread, authority role, and round-trip time.
- StationeersLua is discovered from already loaded assemblies by exact string
  names; the probe has no compile-time StationeersLua reference.

## Current performance evidence

The 2026-07-28 single-player baseline observed:

- worker thread 9 returned through the dispatcher to main thread 1;
- world enumeration completed on thread 1 in 0.2341 ms;
- managed allocation delta was 0 bytes;
- zero anchors, networks, and chips were present in the unprepared world;
- the dispatcher became usable after prefab initialization, 8.7 seconds after
  the `Base` scene callback.

The populated two-port save/reload fixture observed:

- one labelled custom anchor and two distinct physical networks;
- seven attached-device appearances and three circuit housings;
- one Lua chip and two IC10 chips, all with authoritative source reads;
- 0.3884 ms total enumeration time on main thread 1;
- 262,144 bytes managed allocation delta.

The guarded IC10 mutation fixture observed:

- the exact `P302-IC10` housing accepted one temporary valid comment;
- compilation remained error-free;
- the original empty-source SHA-256 was restored;
- mutation plus restoration completed in 0.3919 ms;
- `LastEditedId` and `LastEditedBy` remained zero before and after;
- the installed configuration was returned to read-only immediately afterward.

The implemented fixture records additionally include:

- anchor, physical-network, attached-device, housing, appearance, and unique
  chip counts;
- total enumeration time and maximum per-anchor time;
- managed allocation delta;
- source length, read hash, mutation duration, and compilation result;
- RPC payload size and round-trip latency.

The zero-fixture baseline validates lifecycle and overhead only. Representative
fixture measurements are still required for the topology gates.

## Known failure modes

- The default entrypoint runs before the primary scene, so game objects must be
  deferred until prefab/world readiness.
- Runtime-cloned prefab registration may occur too late for recipe resolution;
  failure blocks P3.03 and indicates that an asset-bundle prefab is required.
- A disconnected port returns no network and is an expected result.
- `CableNetwork.ReferenceId` is a session routing handle and must not be saved.
- A chip can appear through multiple anchors and labels; appearance count and
  unique chip count intentionally differ.
- `LastEditedId` and `LastEditedBy` remained zero during a successful source
  mutation and are rejected as concurrency versions for this build. Use an
  expected source SHA-256 and re-read on the authoritative main thread.
- A successful client-side method call does not establish authority; the RPC
  handler must report server context.
- StationeersLua type/event names can change independently and must remain an
  optional capability.

## Threat boundary

The loopback IDE client is a separate local process and must authenticate to
the future bridge. The local game client is not authoritative in multiplayer;
it may request bounded operations but cannot decide permissions or apply
mutations. The host/dedicated server owns world reads, target resolution,
source preconditions, mutation, audit, and bounded RPC responses. Reference
IDs from any process are session-only and are rejected after a world change.

## Build and validation

```powershell
$env:STATIONEERS_DIR = 'C:\Program Files (x86)\Steam\steamapps\common\Stationeers'
dotnet restore .\mods\StationeersBridge.Feasibility\StationeersBridge.Feasibility.sln
dotnet build .\mods\StationeersBridge.Feasibility\StationeersBridge.Feasibility.sln `
  --configuration Release `
  --no-restore
```

Observed result on 2026-07-28: build succeeded with zero warnings and zero
errors.

Repository validation remains:

```text
npm run check
npm test
npm run build
npm run package:extension
```
