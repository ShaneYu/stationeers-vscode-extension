# Releasing Stationeers IC10

Releases are built from a protected version tag and publish the same
target-specific VSIX files to the Visual Studio Marketplace, Open VSX, and
GitHub Releases.

Ordinary pushes to `main` never publish an extension.

## Release contents

The workflow builds these target-specific packages:

| Target | Runner |
| --- | --- |
| `win32-x64` | `windows-latest` |
| `win32-arm64` | `windows-11-arm` |
| `linux-x64` | `ubuntu-24.04` |
| `linux-arm64` | `ubuntu-24.04-arm` |
| `darwin-x64` | `macos-15-intel` |
| `darwin-arm64` | `macos-15` |

_Windows ARM64 currently uses a preview GitHub-hosted runner._

## Prepare a release

Run the interactive publisher from the repository root:

```powershell
git switch main
git pull --ff-only origin main
npm run release:publish -- minor
```

Use `patch`, `minor`, `major`, or an exact version such as `0.2.0`. The
publisher ensures `main` is checked out and repeats the fast-forward-only pull
as safety checks, then runs `release:bump`. That bump updates all npm and Cargo
versions and lockfiles, moves the `Unreleased` changelog entries into a dated
release, updates the changelog comparison links, and verifies that the
metadata agrees.

The publisher then pauses before committing, tagging, or pushing. Leave its
prompt open, use another terminal to review the diff, and complete the local
checks it displays:

```powershell
git diff
git diff --cached
npm ci
npm run release:check -- v0.2.0
npm run release:hardening
npm run check
npm test
npm run package:extension
python tools/verify_vsix.py "packages/vscode/*@win32-x64.vsix" win32-x64
```

`release:hardening` checks lockfile/workspace consistency, the extension
allowlist and optional dependency boundary, committed evidence redaction, and
a disposable script-free `npm ci`. Compare two independently built VSIX files
with `python tools/verify_reproducible.py first.vsix second.vsix`; archive
metadata is ignored but member content must match exactly. These checks never
promote fixtures, mocks, or compilation into real-game acceptance.

Replace the example version and VSIX target as needed. Sideload the package
into a clean editor profile and exercise completion, hover, diagnostics,
restart, settings, and the changes included in this release. Also confirm the
Marketplace environment and third-party-content gate are ready.

Answer `yes` only when all checks pass. The publisher will:

1. stage only the release metadata files;
2. create `chore(release): v0.2.0`;
3. create a signed annotated `v0.2.0` tag;
4. require `git cat-file -t v0.2.0` to report `tag`; and
5. atomically push `main` and the tag to `origin`.

The maintainer account must be allowed to push this generated release commit
to `main` if the branch is protected.

Answering `no` leaves the prepared version and changelog changes in place. To
resume after inspecting or fixing them, run the command without a version:

```powershell
npm run release:publish
```

If the current manifest version has no matching tag on `origin`, the publisher
resumes that version instead of bumping it again. It also resumes safely if the
release commit or signed tag was created locally but a later step failed.

`release:bump` remains available as the lower-level, non-publishing command:

```powershell
npm run release:bump -- minor
```

Normally, use `release:publish` so none of the commit, tag, validation, or push
steps are missed.

## Publish

`release:publish` automates the equivalent manual Git flow:

```powershell
git switch main
git pull --ff-only origin main
npm run release:bump -- 0.2.0
git add package.json package-lock.json Cargo.toml Cargo.lock packages/vscode/package.json packages/vscode/CHANGELOG.md
git commit -m "chore(release): v0.2.0"
git tag -s v0.2.0 -m "Stationeers IC10 v0.2.0"
git cat-file -t v0.2.0
git push --atomic origin main refs/tags/v0.2.0
```

The version shown above is illustrative. The tagged commit must be contained in
`main`, `git cat-file` must report `tag`, and GitHub must show the tag
signature as verified.

The release workflow:

1. verifies that the tag and all manifests have the same version;
2. runs the test suite;
3. builds and validates all platform packages;
4. creates checksums and build-provenance attestations;
5. publishes the VSIX files to each registry explicitly enabled by its
   protected environment variable; and
6. creates the GitHub Release.

Do not move or reuse a published version tag.

## Retry a partial publication

The release workflow is designed to be rerunnable. Visual Studio Marketplace
publishing skips versions that already exist, and Open VSX publishing is
performed from the previously built VSIX artifacts.

If only one registry failed:

1. rerun the failed GitHub Actions jobs first;
2. do not change the tag or version; and
3. confirm that already-published target packages were not rebuilt from a
   different commit.

If a registry was deliberately disabled for the original run, set its
`PUBLISH_TO_*` environment variable to `true` and use **Re-run all jobs** on
the workflow for the existing tag. The enabled registry receives that same
version, while `--skip-duplicate` prevents an already-published registry from
failing the rerun.

Never delete and recreate a Marketplace extension to recover from a failed
release; extension identities and versions may remain permanently reserved.
