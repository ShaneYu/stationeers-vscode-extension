# IC10 templates

Each directory is a self-contained starting point with source, simulation,
tests, usage notes, and a machine-readable manifest pinned to the bundled game
data version.

Run all fixture tests with:

```powershell
Get-ChildItem templates -Recurse -Filter *.ictest |
  ForEach-Object { target/debug/ic10.exe test $_.FullName }
```

Validate manifest completeness with `node --test templates/manifest.test.mjs`. Legacy
`*.icsim` and `*.ictest` files are still accepted by the tools for
compatibility fixtures.
