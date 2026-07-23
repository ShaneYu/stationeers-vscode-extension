# Releasing Stationeers IC10

Releases are built from a protected version tag and publish the same
target-specific VSIX files to the Visual Studio Marketplace, Open VSX, and
GitHub Releases.

Ordinary pushes to `main` never publish an extension.

## One-time publisher setup

### Visual Studio Marketplace

1. Create or select the `shaneyu` publisher in the
   [Marketplace management portal](https://marketplace.visualstudio.com/manage/publishers/).
2. Confirm that the extension manifest identifies the extension as
   `shaneyu.stationeers`.
3. Sign in to [Azure DevOps](https://aex.dev.azure.com/me) with the same
   Microsoft account that owns or belongs to the Marketplace publisher.
4. Create or open an Azure DevOps organization, then open **User settings**
   (the person-and-gear icon in the top-right) → **Personal access tokens**.
   The direct URL is
   `https://dev.azure.com/<YOUR_ORGANIZATION>/_usersSettings/tokens`.
5. Select **New Token** and configure it:
   - select **All accessible organizations**;
   - select the **Marketplace: Manage** scope;
   - use the shortest practical expiry; and
   - copy the token when it is first displayed.
6. Store the raw token as the `VSCE_PAT` secret in the
   `marketplace-production` GitHub environment.
7. Test the token without committing or printing it:

   ```text
   vsce verify-pat shaneyu
   ```

`vsce` reads the token from the `VSCE_PAT` environment variable. Never place
the value in a script, command committed to the repository, or workflow file.
Record its expiry and rotate it before that date.

Microsoft has announced retirement of global Azure DevOps personal access
tokens on December 1, 2026. Revisit the
[VS Code publishing guidance](https://code.visualstudio.com/api/working-with-extensions/publishing-extension)
before that date in case the Marketplace requires migration to Microsoft Entra
authentication.

### Open VSX

1. Create an [Eclipse account](https://accounts.eclipse.org/user/register).
2. Sign in to [Open VSX](https://open-vsx.org/) with the same GitHub identity.
3. Link the Eclipse account and accept the Publisher Agreement.
4. Generate an access token dedicated to GitHub Actions.
5. Create the namespace:

   ```text
   npx ovsx create-namespace shaneyu -p <token>
   ```

   If Open VSX rejects the name as too similar to an existing namespace, open
   a namespace ownership request in the
   [Open VSX repository](https://github.com/EclipseFdn/open-vsx.org/issues).
   There is no CLI bypass or separate `ovsx claim` command. An administrator
   can create the namespace and assign the requester as its owner.
6. Claim verified ownership through the public namespace request process. If
   an administrator created the namespace while granting that request, do not
   run `create-namespace` again; confirm that the namespace appears in the
   Open VSX profile with the requester listed as its owner.
7. Store the CI token as the `OVSX_PAT` secret in the
   `marketplace-production` GitHub environment.

See the
[Open VSX publishing guide](https://github.com/EclipseFdn/open-vsx.org/wiki/Publishing-Extensions).

### GitHub environment

Create an environment named `marketplace-production`.

- Restrict deployment branches and tags to protected release tags.
- Add the Marketplace and Open VSX credentials listed above.
- Set `PUBLISH_TO_MARKETPLACE=true` only when the release should be uploaded to
  the Visual Studio Marketplace. Leave it unset or set it to `false` to skip
  that registry.
- Set `PUBLISH_TO_OPENVSX=true` only when the release should be uploaded to
  Open VSX. Leave it unset or set it to `false` while namespace ownership is
  pending.
- Leave the `THIRD_PARTY_CONTENT_APPROVED` environment variable unset until the
  third-party content gate below has been completed. Then set it to `true`.
- Require approval before deployment when another maintainer is available.
- Never expose publishing credentials to pull-request workflows.

The two publishing variables are independent and are deliberately opt-in. The
workflow compares each value with the exact lowercase string `true`; missing
values and all other values skip that marketplace. GitHub Release creation and
build-provenance attestations still run regardless of these two settings.

Also configure a GPG or SSH signing key on the maintainer account and add its
public key to GitHub. The release workflow rejects lightweight tags and signed
tags that GitHub cannot verify.

### Repository protection

Create a branch ruleset for `main`:

- require pull requests and passing CI checks;
- prevent force pushes and deletion;
- require conversations to be resolved; and
- restrict direct pushes when the repository has more than one maintainer.

Create a tag ruleset for `v*` that restricts tag creation, update, and deletion
to release maintainers. The workflow also verifies that the tagged commit is
contained in `main`.

## Signing and provenance

There is no private code-signing certificate to store in this repository.

- The maintainer signs the release tag, and the workflow requires GitHub to
  report that signature as verified.
- GitHub Actions creates a cryptographic build-provenance attestation for each
  VSIX attached to the GitHub Release.
- The Visual Studio Marketplace signs every accepted extension package, and
  VS Code verifies that Marketplace signature during installation.

`VSCE_PAT` authenticates the publisher upload only; it does not sign the VSIX.
The Marketplace signature is applied by Microsoft after upload; it should not
be replaced with an unrelated self-signed certificate. See
[Extension runtime security](https://code.visualstudio.com/docs/configure/extensions/extension-runtime-security#_marketplace-protections).

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

Windows ARM64 currently uses a preview GitHub-hosted runner. Remove that matrix
entry temporarily if the runner is unavailable; do not replace it with a
package containing the wrong executable.

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

## Third-party content gate

Before tagging, explicitly complete one of these actions:

- record the licence or permission that covers bundled Stationeers descriptions
  and images; or
- remove or replace material that is not authorised for redistribution; or
- obtain appropriate legal advice and consciously accept the documented risk.

`THIRD_PARTY_NOTICES.md` supplies attribution and excludes third-party material
from the MIT licence. It does not grant redistribution rights.

After completing and documenting the chosen action, set the protected
`marketplace-production` environment variable
`THIRD_PARTY_CONTENT_APPROVED=true`. The workflow refuses to publish while it
is absent or has another value.

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

## Verify the release

After the workflow succeeds:

1. Check both Marketplace pages for icon, README, changelog, licence, links,
   target platforms, and version.
2. Install from Visual Studio Marketplace on at least one clean machine.
3. Install from Open VSX in Antigravity.
4. Download a GitHub Release VSIX and verify it:

   ```powershell
   gh attestation verify <file>.vsix --repo ShaneYu/stationeers-vscode-extension
   ```

5. Confirm that the extension starts the correct bundled server.

Marketplace propagation and scanning can take time. Do not publish a new
version merely because a listing is briefly unavailable.

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
