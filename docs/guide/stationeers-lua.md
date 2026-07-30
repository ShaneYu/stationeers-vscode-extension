# StationeersLua integration

Stationeers Toolkit can coexist with StationeersLua for projects that use both
IC10 and Lua chips. This integration is optional and separate from the bundled
IC10 language server and simulator.

## Lua editing support

StationeersLua is not required for Lua editing. Stationeers Toolkit includes a
small built-in Lua metadata library so common Stationeers globals and the
Toolkit's Lua-facing APIs provide completion, hover help, and diagnostics in a
fresh installation.

For the most complete and current StationeersLua API knowledge, install both
extensions. When the StationeersLua extension is present, the Toolkit keeps its
library as the authoritative Stationeers API and adds only its own lightweight
editor metadata. This means updates to the StationeersLua extension can be
picked up without requiring a matching Toolkit release. Without it, the
Toolkit's bundled fallback remains available.

## Choose one live integration owner

Both extensions can be installed, but avoid using their live network, chip
editor, or source synchronisation tools against the same game session at the
same time. Choose one extension to own live operations so both tools do not
poll, edit, or push the same devices and Lua chips concurrently.

The extensions use separate activity-bar containers, so both panels can remain
available at the same time. **Stationeers Toolkit** appears under its own
activity-bar icon as **Live Networks**; **StationeersLua** keeps its own chip
explorer panel.

For work involving IC10, simulations, scenario tests, topology, and debugging,
use Stationeers Toolkit as the live integration owner. Keep StationeersLua
installed when you want its latest Lua editing metadata, but leave its live
network/source tools unused for that session.

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
