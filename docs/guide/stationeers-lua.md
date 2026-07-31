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

<figure class="screenshot screenshot-two-thirds" style="view-transition-name: screenshot-live-network-scope">
  <img src="/screenshots/live-integration/network-scope-and-chips.png" alt="Live Networks view connected to a Stationeers world with two expanded networks and accessible and inaccessible chips" loading="lazy">
  <figcaption>Live Networks shows the connected world, remote network scopes, discovered chips, and whether each chip is currently accessible.</figcaption>
</figure>

The connected view also explains the accessibility boundary. Unpowered chips
are not accessible, and Lua chips must be in range through the Wireless
Development Board according to the StationeersLua integration. The same panel
can therefore show a discovered chip as visible but unavailable, while chips
on an in-range network are marked accessible.

## What is supported

The Live Networks view can show Lua chips discovered through the local
StationeersLua service. When an exact chip and housing are selected in an open
in-game editor, the extension can pull, compare, and push Lua source according
to the supported local contract.

The extension keeps IC10 bridge traffic and StationeersLua traffic separate:
IC10 uses the authenticated local bridge, while Lua source uses the Lua service
endpoint. Both are local-only integrations.

<figure class="screenshot" style="view-transition-name: screenshot-live-source">
  <img src="/screenshots/live-integration/live-source.png" alt="Live Networks view with a selected chip and its live source open in the editor" loading="lazy">
  <figcaption>A discovered chip can be opened as live source for inspection and, where supported, synchronisation with the game.</figcaption>
</figure>

When the game closes or the bridge connection is lost, an already-open live
source tab remains visible but is marked unavailable until the game reconnects.

<figure class="screenshot" style="view-transition-name: screenshot-live-disconnected">
  <img src="/screenshots/live-integration/disconnected-game.png" alt="Live Networks view disconnected from the game while an open live source tab is marked unavailable" loading="lazy">
  <figcaption>Connection loss leaves live source open for reference but marks it unavailable until the bridge reconnects.</figcaption>
</figure>

## Configuration

| Setting | Default | Purpose |
| --- | --- | --- |
| `stationeers.bridge.url` | `http://127.0.0.1:3032` | Local authenticated bridge for IC10 discovery and source. |
| `stationeers.stationeersLua.url` | `http://127.0.0.1:3030` | Local StationeersLua source service. |

If the service is unavailable or reports an incompatible version, the toolkit
leaves the chip visible but disables live source operations. Normal IC10 editing
and simulation continue to work.
