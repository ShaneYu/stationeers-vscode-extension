# Stationeers Bridge Remote Network

This separately installed code mod adds `StructureRemoteNetwork` and
`ItemKitRemoteNetwork`. The device is cloned from the observed Logic Memory
prefab at prefab-ready time, with isolated build metadata and distinct hashes;
the vanilla Logic Memory prefab and recipe are never edited.

The device keeps Logic Memory's two data ports and passive zero-power behavior.
Its ordinary labeller name is the discovery label. An empty or whitespace-only
label is retained on the device but produces a configuration warning and no
deployable scope. Labels are intentionally allowed to alias the same physical
network, so the same chip can appear in more than one scope.

## Build

```powershell
$env:STATIONEERS_DIR = 'C:\Program Files (x86)\Steam\steamapps\common\Stationeers'
dotnet restore .\mods\StationeersBridge.RemoteNetwork\StationeersBridge.RemoteNetwork.sln
dotnet build .\mods\StationeersBridge.RemoteNetwork\StationeersBridge.RemoteNetwork.sln --configuration Release --no-restore
dotnet run --project .\mods\StationeersBridge.RemoteNetwork.Tests\StationeersBridge.RemoteNetwork.Tests.csproj
```

The Debug build automatically deploys `About/`, `GameData/`,
`StationeersBridge.RemoteNetwork.dll`, `StationeersBridge.RemoteNetwork.Core.dll`,
and the dependency manifest into:

```text
%USERPROFILE%\Documents\My Games\Stationeers\mods\StationeersBridge.RemoteNetwork
```

Each Release build replaces that generated mod directory first, removing stale
files from older builds while leaving the parent `mods` directory untouched.

The destination can be changed without editing the project:

```powershell
$env:STATIONEERS_DOCUMENTS_DIR = 'D:\Profiles\Shane\Documents'
# Or point directly at the final mods directory:
$env:STATIONEERS_MODS_DIR = 'D:\Stationeers\mods'
dotnet build .\mods\StationeersBridge.RemoteNetwork\StationeersBridge.RemoteNetwork.sln --configuration Release --no-restore
```

To build without copying files, pass `-p:DeployRemoteNetworkMod=false`.

When enabled, the mod also starts a read-only authenticated loopback bridge at
`http://127.0.0.1:3032/bridge/v1`. VS Code first discovers the local bridge and
retrieves the pairing token through the loopback-only `/pair` route, storing it
in SecretStorage. If automatic pairing is unavailable, the generated token is
persisted in the BepInEx configuration file under the `Bridge` section and can
be entered through `Stationeers: Pair Bridge`; never commit or share the token.
The current runtime exposes `hello` and `scopes` only. IC10 source reads,
writes, WebSocket events, and multiplayer relay remain disabled until their
game-thread integrations are verified.

## Scope and evidence constraints

The discovery index is main-thread-owned and emits immutable snapshots. Numeric
game references are converted to strings at the DTO boundary. Physical network
references are session handles and are never saved as scope authority.

This slice uses only contracts observed by P3.02: `Prefab.Find`,
`Prefab.RegisterExisting`, `Device.AllDevices.Active`, `Device.GetNetwork`,
`CableNetwork.ReferenceId`, and circuit housing chip access. Duplicate/bridged
topology and multiplayer authority remain P3.02 evidence gaps and are not
claimed as supported here. No global Logic Memory mutation, REST service, or
unsupported setting-removal hook is included.
