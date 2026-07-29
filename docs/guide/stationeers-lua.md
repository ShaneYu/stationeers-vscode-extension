# StationeersLua integration

Stationeers Toolkit can coexist with StationeersLua for projects that use both
IC10 and Lua chips. This integration is optional and separate from the bundled
IC10 language server and simulator.

## What is supported

The Live Networks view can show Lua chips discovered through the local
StationeersLua service. When an exact chip and housing are selected in an open
in-game editor, the extension can pull, compare, and push Lua source according
to the supported local contract.

The extension keeps IC10 bridge traffic and StationeersLua traffic separate:
IC10 uses the authenticated local bridge, while Lua source uses the Lua service
endpoint. Both are local-only integrations.

## Configuration

| Setting | Default | Purpose |
| --- | --- | --- |
| `stationeers.bridge.url` | `http://127.0.0.1:3032` | Local authenticated bridge for IC10 discovery and source. |
| `stationeers.stationeersLua.url` | `http://127.0.0.1:3030` | Local StationeersLua source service. |

If the service is unavailable or reports an incompatible version, the toolkit
leaves the chip visible but disables live source operations. Normal IC10 editing
and simulation continue to work.
