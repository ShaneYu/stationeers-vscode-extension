# P1.04 — Deployment build pipeline

## Goal

Let users maintain readable, commented IC10 source while producing a safe,
compact artefact for Stationeers without modifying the source document.

## Build command

Add `IC10: Build for Game` and `ic10 build` with stages:

1. validate source and selected environment;
2. remove comments and blank lines;
3. recalculate literal numeric relative branches;
4. optionally replace symbolic labels/defines and shorten private aliases;
5. verify every official program-size constraint available for the selected
   game version;
6. emit deployable IC10 plus a source map and build report.

Build output lives under a configurable directory beside each source program,
`build/` by default. A clipboard-only mode avoids writing an artefact.

## Safety

- Never rewrite the source file as part of a build.
- Refuse transformations whose branch behaviour cannot be preserved.
- Preserve special values and exact numeric spellings where changing them could
  alter behaviour.
- Show a preview diff for optimising transformations.
- Associate every generated line with its original source line.
- Include the source hash, tool version, game-data version, and build options in
  sidecar metadata, not in code copied to the game.

## User experience

Provide:

- status bar usage for lines and any officially sourced byte/per-line limits;
- warnings as limits approach and errors only at verified hard limits;
- `Copy Deployable Code`;
- `Open Built Code`;
- a readable optimisation report showing saved lines/bytes;
- debugger source mapping when a built artefact is used;
- a task/problem matcher suitable for CI.

## Optimisation levels

- `readable`: comments/blank lines removed, names retained;
- `compact`: safe constant/label substitutions and alias shortening;
- `none`: validate and copy source exactly.

Avoid clever global optimisation until conformance and source mapping are
proven. Every optimisation requires semantic equivalence tests.

## Acceptance criteria

- [x] Building never changes the source document.
- [x] Relative branches retain the same destinations or the build fails.
- [x] Generated lines map back to source for diagnostics and debugging.
- [x] Limits come from versioned official/generated data and are labelled when
      unknown.
- [x] Clipboard, file, and CLI builds produce identical code for identical
      options.
- [x] Optimisations have golden and property-based equivalence tests.
- [x] Build metadata records source hash, tool version, and game-data version.
- [x] The current remove-comments implementation is reused or consolidated
      rather than maintained as a divergent transformer.

## Dependencies

- [P0.01](p0-01-language-correctness.md) semantic model and control flow.
- [P0.02](p0-02-simulator-conformance.md) verified limits and semantics.

## Non-goals

- Silently changing algorithms to save lines.
- Treating undocumented community limits as authoritative errors.

## Decisions

- Generated code is an artefact; readable `.ic10` remains the source of truth.
- The build engine is the `ic10-build` library crate. Editor and the separate
  `ic10` CLI crate call it; this item does not introduce a competing binary.
- Only underscore-prefixed aliases are private and eligible for deterministic
  shortening.
- Missing official byte and per-line byte limits are reported as unknown, not
  enforced from community claims.
- Relative output directories are resolved from the source program, so nested
  projects keep their generated artefacts beside the code they belong to.
