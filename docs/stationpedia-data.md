# Stationpedia data contract

The generator consumes:

- `stationpedia.json` for commands, constants, pages, logic capabilities, slots,
  modes, connections, memory, prefab names, and prefab hashes;
- `enums.json` for IC10 enum names, values, deprecation state, and descriptions;
- `Textures/<PrefabName>.png` for hover thumbnails.

It produces:

- `data/generated/instructions.json`
- `data/generated/devices.json`
- `data/generated/manifest.json`
- `packages/vscode/syntaxes/ic10.tmLanguage.json`
- `packages/vscode/assets/devices/*.png`

All outputs are deterministic for the same export and override file. A
generation timestamp is intentionally omitted to keep review diffs meaningful.

`STATIONEERS_DIR` is resolved in this order:

1. `--stationeers-dir`
2. the existing process environment
3. the root `.env` file

The dotenv loader does not overwrite a real environment variable. Both the game
root and its `Stationpedia` child are accepted.

The one currently missing logicable thumbnail is recorded in
`data/generated/manifest.json` rather than treated as fatal. Missing metadata,
unknown access values, duplicate prefab names, or hash collisions are fatal.

