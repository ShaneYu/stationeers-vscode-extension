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

The mod package contains `About/`, `GameData/`, and the built
`StationeersBridge.RemoteNetwork.dll`.

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
