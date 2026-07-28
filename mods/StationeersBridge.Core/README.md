# Stationeers bridge transport core

This project contains the game-independent P3.04 HTTP boundary plus the bounded
P3.06 conditional IC10 source-write slice. It accepts
immutable snapshots through `IBridgeSnapshotProvider`; Unity and Stationeers
objects must be copied into those DTOs on the verified main thread before the
provider is called. It binds only to `127.0.0.1` and `::1`, requires a bearer
pairing token, and bounds requests/connections/rate/source sizes. PUT source
writes require `IBridgeSourceMutationProvider`; its method is the authoritative
mutation boundary for world, permission, target, version, and hash checks.
There is deliberately no force-write option. Source validation allows printable
text plus tab/newline.

The current feasibility evidence does not establish multiplayer authority,
duplicate/bridged topology reconciliation, or a production game integration.
Those remain explicit integration boundaries for later work.

Build with:

```powershell
dotnet build .\mods\StationeersBridge.Core\StationeersBridge.Core.csproj --configuration Release
```
