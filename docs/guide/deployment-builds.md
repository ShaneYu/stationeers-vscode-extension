# Deployment builds

**IC10: Build for Game** validates a source program and writes deployable code
plus metadata beside it. The matching `ic10 build` command provides the same
pipeline for scripts and CI.

## Build modes

- `none` keeps source text as close to the input as possible.
- `readable` removes comments while retaining a reviewable layout.
- `compact` applies safe relative-branch rewriting and produces a compact
  deployable program.

Builds can include a source map, reproducibility metadata, preview diff, and
optimisation report. **IC10: Copy Deployable Code** runs the identical build
without writing an artefact.

```text
ic10 build controller.ic10
ic10 build controller.ic10 --stdout
```

Build output is written under `build/` beside the source by default. Configure
`ic10.build.optimization`, `ic10.build.outputDirectory`, and
`ic10.build.gameVersion` in settings.
