# Deployment builds

Deployment builds let the checked-in `.ic10` file remain readable while a
deterministic artefact is produced for Stationeers. A build never writes to or
edits its source. By default, each program writes to a `build/` directory
beside that source file. For example, `programs/multi-ic/item-requester.ic10`
writes `programs/multi-ic/build/item-requester.ic10` and its sidecars.

## Build levels

- `none` validates the program and preserves comments, line endings, numeric
  spellings, and special values.
- `readable` removes comments and blank lines and recalculates literal numeric
  `br...` and `jr` offsets.
- `compact` additionally resolves safe define and absolute-label references
  and shortens aliases whose names begin with `_`. Underscore-prefixed aliases
  are the explicit convention for private aliases.

Every level removes leading spaces and tabs from each emitted line. IC10 does
not use indentation semantically, and the flush-left output uses less
horizontal space in the in-game editor.

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

## Headless and CLI builds

The `ic10` executable uses the same `ic10-build` library as the editor:

```text
ic10 build program.ic10
ic10 build program.ic10 --optimization compact
ic10 build program.ic10 --output-dir deploy
ic10 build program.ic10 --output deploy/program.ic10
ic10 build program.ic10 --stdout
```

Without an output option, the CLI writes `build/<source-name>` and its three
sidecars beside the source program, independent of the shell's current
directory. `--output-dir` changes that directory and `--output` selects the
exact code path. `--no-sidecars` writes only the code. `--stdout` (or
`--output -`) emits only deployable code and performs no writes, making it
suitable for pipes. `--quiet` suppresses the file-build summary.

The remaining reproducibility options are `--optimization
none|readable|compact`, `--game-version VERSION`, and `--environment NAME`.
The CLI records the canonical source path in metadata for debugger mapping and
refuses an output path that resolves to the source.

Library consumers can use the same engine without filesystem or argument
parser dependencies:

```rust
let output = ic10_build::build(&source, &options, &knowledge)?;
```

CI can use the contributed `$ic10-build` problem matcher for diagnostics in
the form:

```text
path/file.ic10:12:1: error[unsafe-relative-branch]: message
```

Source files are never rewritten by the library. The older **Remove All
Comments** editing command is retained for users who explicitly request an
edit, but it now calls the same `readable` Rust pipeline rather than maintaining
a second transformer.
