# Workspace format fixtures

These files exercise the neutral naming and metadata contract. They are
documentation fixtures, not executable simulator tests: Lua
execution remains explicitly unsupported until P3.09.

- `canonical-ic10.icsim` and its test show the canonical IC10
  shape.
- `mixed-lua.ictest` shows a mixed-language selection that must
  report an unsupported Lua runtime before P3.09.
- `canonical-layout.icsimlayout` and
  `layout-example.icsimlayout` cover the supported layout sidecar format.
