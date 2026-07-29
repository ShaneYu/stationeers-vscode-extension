# Live-validation checklist

This checklist covers the remaining game/install gates. Contract fixtures and
unit tests prove request and response handling only; they do not replace a
supported Stationeers runtime capture.

## RemoteNetwork reload and incremental discovery

- [ ] Start with one labeled RemoteNetwork anchor and capture `/hello` and
      `/scopes` as the initial snapshot.
- [ ] Add or remove an anchor/chip, reconnect a cable, and refresh discovery.
      Revision must advance, unchanged identities must remain stable, and the
      new attachment must appear exactly once.
- [ ] Save, return to menu, reload the same world, and refresh discovery.
      Record whether `worldEpoch` is retained or replaced; never reuse a
      target across an epoch change.
- [ ] Deconstruct/rebuild an anchor and verify stale references disappear at
      the lifecycle-safe refresh boundary.
- [ ] Capture sanitized evidence under `docs/live-integration/evidence/`.

## Authenticated bridge

- [ ] Verify `/pair` is available only on loopback and stores no token in
      workspace files, URLs, errors, or routine logs.
- [ ] Verify `/hello`, `/scopes`, and source routes reject missing, malformed,
      expired, and wrong bearer tokens with safe `401` responses.
- [ ] Verify the extension sends `Authorization: Bearer <token>` on every
      protected request and keeps pairing unauthenticated only for `/pair`.
- [ ] Verify IPv4/IPv6 loopback works as supported and non-loopback access is
      denied.

## Stale-push sequence

- [ ] Discover a writable IC10 target and record `worldEpoch`, version, and
      SHA-256 from the same snapshot/source read.
- [ ] Change world or replace the target before Push; capture retryable `410`
      (`stale_world` or `stale_target`) and confirm no mutation occurred.
- [ ] Refresh discovery, discard the old target/base, and Push only against
      the new epoch and current version/hash.
- [ ] Verify a concurrent source change yields `409 source_conflict`, with no
      automatic overwrite and a current-version/hash response.

## StationeersLua coexistence and installation matrix

- [ ] Execute every row in `stationeers-lua/fixtures/installation-matrix.json`
      on a clean extension profile and record pass/fail/blocked.
- [ ] With both mods installed, confirm IC10 bridge scopes and StationeersLua
      Lua scopes are visible together without duplicate commands or ownership.
- [ ] Confirm Lua Pull/Push uses the StationeersLua route and IC10 Pull/Push
      uses the bridge route; a missing service leaves the other workflow usable.
- [ ] Test supported `0.9.5.0`, an unsupported StationeersLua version, and a
      disabled bridge against the same save. Version mismatch must fail closed.
- [ ] Test clean install, upgrade, disable/uninstall, and save reopen; record
      exact extension/mod versions and sanitized evidence.

## Evidence status

- Automated fixture/unit result: contract-only.
- Development Extension Host: UI smoke only.
- Packaged extension plus real game/mod stack: required for runtime pass.
