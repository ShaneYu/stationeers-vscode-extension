# Contributing to Stationeers IC10

Thank you for helping improve the IC10 toolchain. Bug fixes, tests,
documentation, language features, and corrections to generated reference data
are welcome.

By contributing, you agree that your contribution may be distributed under
the project's MIT License and that you have the right to submit it.

## Development workflow

Use a short-lived branch and open a pull request into `main`. Keep `main`
releasable: pull requests should pass CI and include tests or documentation
appropriate to the change.

Public releases are created from protected version tags, not ordinary branch
pushes. Maintainers should follow [docs/releasing.md](docs/releasing.md).

## Repository layout

```text
crates/
  ic10-data/       Typed, embedded generated data
  ic10-core/       Parser, symbols, and diagnostics
  ic10-lsp/        LSP protocol adapter and server binary
packages/
  vscode/          VS Code extension, grammar, hover assets, and Marketplace docs
tools/
  stationpedia/    Python export transformer and overrides
data/generated/    Versioned JSON consumed by Rust builds
docs/              Architecture, data-pipeline, and release notes
examples/          Example IC10 programs
```

## Prerequisites

- Rust 1.90, pinned by `rust-toolchain.toml`
- Node.js 22 or newer
- Python 3.11 or newer
- Visual Studio Code when running the Extension Development Host

Python is only a runtime dependency for tests, data generation, and development.
It is not required by the published extension.

## Install dependencies

From the repository root:

```powershell
npm ci
```

Cargo downloads Rust dependencies automatically when a build is first run.

## Build and test

Run the complete test suite:

```powershell
npm test
```

Run formatting and static checks:

```powershell
npm run check
```

Build release versions of the language server and extension:

```powershell
npm run build
```

The test suite includes Rust unit tests, Stationpedia generator tests, a
stdio JSON-RPC smoke test against the real language server, and TypeScript type
checking.

## Extension development

Open the repository root in Visual Studio Code and run the
**Run IC10 Extension** launch configuration. Its build task compiles the debug
language server and bundles the TypeScript client.

For continuous TypeScript bundling:

```powershell
npm run dev --workspace packages/vscode
```

The development host prefers `target/debug/ic10-lsp`. To test another server,
set `ic10.server.path` in the development host's settings.

## Create a local VSIX

```powershell
npm run package:extension
```

This command:

1. builds a locked release version of `ic10-lsp`;
2. stages only the current operating-system and architecture binary;
3. creates a clean production extension bundle; and
4. packages a target-specific VSIX.

The output is written under `packages/vscode` and is ignored by Git.

Validate a package before sharing it:

```powershell
python tools/verify_vsix.py packages/vscode/<package>.vsix <target>
```

For example, the target on a normal 64-bit Windows machine is `win32-x64`.

## Refresh Stationpedia data

Copy `.env.example` to `.env` and point it at either the game installation or
an export directory:

```dotenv
STATIONEERS_DIR="C:\Program Files (x86)\Steam\steamapps\common\Stationeers"
```

Then run:

```powershell
python tools/stationpedia/generate.py
```

The generator validates the source, applies the reviewable corrections in
`tools/stationpedia/overrides.json`, writes deterministic JSON and TextMate
grammar files, and copies relevant thumbnails.

Generated files are committed so normal builds never depend on a local game
installation. Run the generator twice and confirm that the second run produces
no diff before submitting generated changes.

Use `python tools/stationpedia/generate.py --help` for path overrides and the
`--no-assets` option.

## Preparing a release

Record user-facing changes under `Unreleased` in
`packages/vscode/CHANGELOG.md`, then bump all npm, Cargo, lockfile, and
changelog release metadata with one command:

```powershell
npm run release:bump -- patch
```

Use `minor`, `major`, or an exact version such as `0.2.0` instead of `patch`
when appropriate. The command updates the changelog comparison links, creates
the dated release heading, and verifies that all release metadata agrees. It
does not commit, tag, push, or publish anything.

Review the resulting diff and run the displayed `release:check` command before
opening the release pull request.

## Pull request checklist

- Tests pass locally.
- New behavior has focused tests where practical.
- User-facing changes are documented in `packages/vscode/CHANGELOG.md`.
- Generated data changes include their source version and are deterministic.
- No credentials, `.env` files, game executables, or unreviewed binary assets
  are included.
- The extension version is changed only as part of a release.
