# Stationeers bridge feasibility probes

This is a development-only StationeersLaunchPad code mod for P3.02. It records
the exact game-thread, prefab, network, source, authority, RPC, and optional
StationeersLua contracts required by later live-integration work.

The build resolves Stationeers, Unity, BepInEx, LaunchPad, and optional
StationeersLua references from a configured local game installation.

## Build

PowerShell:

```powershell
$env:STATIONEERS_DIR = 'C:\Program Files (x86)\Steam\steamapps\common\Stationeers'
dotnet restore .\mods\StationeersBridge.Feasibility\StationeersBridge.Feasibility.sln
dotnet build .\mods\StationeersBridge.Feasibility\StationeersBridge.Feasibility.sln `
  --configuration Release `
  --no-restore
```

The output contains only `StationeersBridge.Feasibility.dll`.

## Install and enable

Copy `About`, `GameData`, and the built DLL into a local mod directory:

```text
%USERPROFILE%\Documents\My Games\Stationeers\mods\StationeersBridge.Feasibility\
```

Launch the game once, then enable `Development.Enabled` in the generated
`dev.stationeers.bridge.feasibility.cfg`. Keep source mutation and RPC probes
off until the named test fixtures described in
`docs/live-integration/evidence/README.md` are ready.

The source mutation probe also requires the exact housing name and
`SourceMutationConfirmation = MUTATE_AND_RESTORE_P302_SOURCE`.

For an initial metadata/prefab startup run, copy
`dev.stationeers.bridge.feasibility.cfg.example` to
`BepInEx/config/dev.stationeers.bridge.feasibility.cfg`. It enables read-only
startup probes and prefab registration while leaving source mutation and RPC
disabled.

Every log record starts with `[P3.02]` and contains one compact JSON object.
Sanitize personal paths, player names, save names, addresses, and credentials
before copying evidence into the repository.
