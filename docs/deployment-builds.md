# Deployment builds

Deployment builds let the checked-in `.ic10` file remain readable while a
deterministic artefact is produced for Stationeers. A build never writes to or
edits its source. The default output directory is `.ic10/build/`, which this
repository's default `.gitignore` ignores.

## Build levels

- `none` validates the program and emits its bytes exactly, including comments,
  line endings, numeric spellings, and special values.
- `readable` removes comments and blank lines and recalculates literal numeric
  `br...` and `jr` offsets.
- `compact` additionally resolves safe define and absolute-label references
  and shortens aliases whose names begin with `_`. Underscore-prefixed aliases
  are the explicit convention for private aliases.

The builder refuses a transformation when it cannot prove that a relative
branch keeps its destination. In particular, removing any line while a
relative branch uses a register, alias, define, non-integer, or out-of-range
offset is an error. It does not guess.

Run **IC10: Build for Game** to write:

- `<name>.ic10` — code to paste into the game;
- `<name>.ic10.map.json` — generated-line to source-line mappings;
- `<name>.ic10.metadata.json` — source SHA-256, toolkit version, official game
  data version, and exact options;
- `<name>.ic10.report.json` — line/byte savings, transformations, and limits.

**IC10: Copy Deployable Code** runs the same in-memory build and writes
nothing. **IC10: Open Built Code** writes the sidecars and opens the artefact.
Compact builds also open a source-to-output preview diff. Metadata is never
inserted into the deployable code copied to the game.

## Limits and versions

The official generated Stationpedia data currently supplies a hard program
line limit. The report associates it with the exact data version. No official
generated whole-program byte or per-line byte limits are currently available,
so these are explicitly recorded as unknown and never treated as errors.
Setting `ic10.build.gameVersion` makes a mismatch with bundled data fail rather
than silently applying stale limits.

## Headless and CLI integration

`ic10-build` is a library crate with no filesystem, VS Code, LSP, or argument
parser dependencies:

```rust
let output = ic10_build::build(&source, &options, &knowledge)?;
```

The `ic10` CLI crate owns argument parsing and file/clipboard policy and should
call this API for `ic10 build`. This keeps it compatible with the scenario CLI
without adding a competing binary. Writing `output.code`,
`output.source_map_json()`, `output.metadata_json()`, and
`output.report_json()` gives the same artefacts as VS Code.

CI can use the contributed `$ic10-build` problem matcher for diagnostics in
the form:

```text
path/file.ic10:12:1: error[unsafe-relative-branch]: message
```

Source files are never rewritten by the library. The older **Remove All
Comments** editing command is retained for users who explicitly request an
edit, but it now calls the same `readable` Rust pipeline rather than maintaining
a second transformer.
