# Workspace format fixtures

These files exercise the P3.01 neutral naming and metadata contract. They are
documentation and migration fixtures, not executable simulator tests: Lua
execution remains explicitly unsupported until P3.09.

- `canonical-ic10.stationeerssim.json` and its test show the canonical IC10
  shape.
- `mixed-lua.stationeerstest.json` shows a mixed-language selection that must
  report an unsupported Lua runtime before P3.09.
- `canonical-layout.stationeerssim.layout.json` and
  `legacy-layout.ic10sim.layout.json` cover the canonical and legacy layout
  sidecars without changing their non-semantic contents.
