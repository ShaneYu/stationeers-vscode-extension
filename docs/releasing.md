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

1. Start from a clean branch based on `main`.
2. Choose the next Semantic Version.
3. Set the same version in:
   - root `package.json`;
   - `packages/vscode/package.json`; and
   - `[workspace.package].version` in `Cargo.toml`.
4. Run `cargo check --workspace --locked` so `Cargo.lock` records the workspace
   package version.
5. Move relevant entries from `Unreleased` into a dated version section in
   `packages/vscode/CHANGELOG.md`.
6. Update the changelog comparison links.
7. Run:

   ```powershell
   npm ci
   npm run release:check -- v0.1.0
   npm run check
   npm test
   npm run package:extension
   ```

8. Validate the local package using `tools/verify_vsix.py`.
9. Sideload it into a clean editor profile and exercise completion, hover,
   diagnostics, restart, and settings.
10. Open and merge a release pull request.


## Publish

After the release commit is on `main`, create and push a signed annotated tag:

```powershell
git tag -s v0.1.0 -m "Stationeers IC10 v0.1.0"
git push origin v0.1.0
```

The tagged commit must be contained in `main`, and GitHub must show the tag
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
