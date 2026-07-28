# P3.10 release evidence report — `VERSION`

## Decision

- Release status: `draft | blocked | ready`
- Real-game acceptance: `not-run | blocked | observed`
- Decision owner/date: `REPLACE-ME`
- Evidence JSON: `../evidence/REPLACE-ME.json`

`observed` is valid only when every required sequence in the P3.10 checklist
has attributable sanitized evidence and no blocker. Do not infer it from CI.

## Inputs and compatibility

| Component | Version/build | Evidence |
| --- | --- | --- |
| Extension | `REPLACE-ME` | `REPLACE-ME` |
| Bridge mod | `REPLACE-ME` | `REPLACE-ME` |
| Game / loader / LaunchPad | `REPLACE-ME` | `REPLACE-ME` |
| StationeersLua / `sumneko.lua` | `REPLACE-ME` | `REPLACE-ME` |
| OS / architecture | `REPLACE-ME` | `REPLACE-ME` |
| Commit / build inputs | `REPLACE-ME` | `REPLACE-ME` |

## Automated results

| Gate | Result | Command or artifact | Notes/blocker |
| --- | --- | --- | --- |
| Docs/schema/fixtures | `pending` | `node tools/verify-p310-docs.mjs` | |
| Package contents/hashes | `pending` | `REPLACE-ME` | |
| Security/abuse | `pending` | `REPLACE-ME` | |
| Performance/recovery | `pending` | `REPLACE-ME` | |

## Real-game sequence results

| Required sequence | Result | Evidence | Blocker |
| --- | --- | --- | --- |
| Single-player conflict-safe write | `not-run` | | |
| World reload/chip replacement | `not-run` | | |
| Listen-server host/remote player | `not-run` | | |
| Dedicated server/no IDE listener | `not-run` | | |
| Unmodded server fail-closed | `not-run` | | |
| Two simultaneous stale writers | `not-run` | | |
| Workspace-host matrix | `not-run` | | |

## Security, deviations, and recovery

- Auth/token/redaction review: `REPLACE-ME`
- Conflict/authority review: `REPLACE-ME`
- Known deviations and deferred capabilities: `REPLACE-ME`
- Rollback, save-load, disable/uninstall result: `REPLACE-ME`
- Package contents audit and hashes: `REPLACE-ME`

## Final gate statement

`REPLACE-ME`: state explicitly whether release is blocked or ready. If any
real-game row above is not `observed`, this statement must remain blocked.
