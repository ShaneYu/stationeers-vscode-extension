# P3.10 release evidence report — 0.3.1

## Decision

- Release status: `blocked`
- Real-game acceptance: `blocked`
- Decision date: 2026-07-29
- Evidence JSON: [p310-release-evidence-2026-07-29.json](../evidence/p310-release-evidence-2026-07-29.json)

The locally provable authority, contract, redaction, packaging, and release
metadata checks pass. Single-player runtime evidence is now captured, but this
is not a supported-release acceptance report because multiplayer, dedicated
server, performance, and installation-matrix evidence remain open.

## Automated results

| Gate | Result | Evidence |
| --- | --- | --- |
| Documentation/schema/fixtures | observed | `node tools/verify-p310-docs.mjs`; `npm test` |
| Release metadata and hardening | observed | `npm run release:check`; `node tools/verify-release-hardening.mjs` |
| Authority/identity/queue/conflict contracts | observed | Relay 10 cases; RemoteNetwork 18 cases |
| Extension/remote-workspace contract tests | observed | 127 VS Code tests and TypeScript check |
| Large-world/multiplayer performance | not-run | Requires representative worlds and multiple users |

## Real-game sequence results

The single-player conflict-safe write and world reload sequences are
`observed`; sanitized details are in
`../evidence/runtime-live-2026-07-29.json`. Listen-server remote player,
dedicated-server listener suppression, unmodded fail-closed behavior,
concurrent stale writers, chip replacement, and the workspace-host matrix
remain `not-run`.

## Security and recovery

The local implementation now fails closed for unauthenticated/non-authoritative
transport, unverified ownership, revoked players, disabled policy, stale
sessions/worlds, oversized input, unbounded retries, and mismatched response
correlation. Evidence checks reject credentials, source text, identities,
addresses, and absolute paths. Runtime listener, credential storage, server
role, save recovery, and large-world behavior still require manual validation.

## Reproducibility

Automated inputs are pinned by `package-lock.json` and Cargo.lock. The report
was captured from commit `c315bb0` plus the live-validation working-tree
changes listed by `git status`; it is therefore not a release artifact or claim
of reproducible published packages.

## Final gate statement

`blocked`: real-game acceptance is not complete, so P3.10 and P3.07 remain
open despite the passing local contract, release hardening, and single-player
runtime checks.
