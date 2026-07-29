# StationeersLua live contract evidence

This directory records the public StationeersLua REST contract investigated
for P3.08. Evidence is versioned by the observed StationeersLua mod version
and capture date. It deliberately separates observed wire responses from
documented claims and unresolved hypotheses.

## Current capture

| Item | Value | Evidence status |
| --- | --- | --- |
| Capture date | 2026-07-29 | observed |
| Stationeers game | `0.2.6403.27689` | documented in repository metadata |
| BepInEx | `5.4.23.3` | documented in repository metadata |
| StationeersLaunchPad | `0.5.0.0` | documented in repository metadata |
| StationeersLua | `0.9.5.0` | observed from `/api/status`; also in repository metadata |
| Bridge mod | `0.1.0` | documented from bridge manifest |
| REST base URL | `http://127.0.0.1:3030` | documented and observed |
| API/contract version | not reported | blocked |

## How the fixtures were obtained

1. Stationeers was running in a disposable world with StationeersLua and the
   bridge mod loaded.
2. The local REST service was queried with `GET` requests while an in-game
   editor scope and a Wireless Development Board scope were active.
3. After the player deliberately left that scope, two explicitly authorized
   `PUT` requests sent the tiny disposable probe to the documented chip-code
   route, first without a mode and then with `mode=chip`. Both were rejected
   with HTTP 400 before mutation and are preserved as failure fixtures.
4. The player then connected a Wireless Development Board. A read and
   explicitly authorized `mode=chip` write succeeded for Ticker `882` in
   housing `888`; read-back matched the tiny probe exactly.
5. The player next opened that exact chip in the in-game editor. An explicitly
   authorized `mode=editor_then_chip` write updated both the editor draft and
   chip, and read-back again matched exactly. No debugger mutation was sent.
6. Responses were sanitized to retain only the tiny disposable source fixture.
   Numeric reference IDs are opaque disposable-world identifiers.
7. A later comparison of the same wireless scope showed that Scripted Screens
   are composite hosts: the bridge originally reported the outer console ID,
   while StationeersLua reported a nested chip `ref_id` and circuitboard
   `housing_ref_id`. The pre-fix mismatch is preserved without source code.
8. After rebuilding the bridge mod and restarting the game, the same read-only
   capture showed Screen 1 and Screen 2 as explicitly marked housing
   identities. Each matched exactly one StationeersLua Lua record by
   `housing_ref_id` while no editor chip was selected.
9. The restart capture also showed Ticker's `source_version` changing from 3
   to 1 while its ReferenceIds and 60-byte source length remained the same.
   No source body was read for this restart check.
10. The user launched the updated extension in a Development Extension Host
    and confirmed that the corrected Scripted Screen accessibility appeared
    and worked in the Live Networks explorer. This is an `observed` manual UI
    result; packaged-extension installation remains `not-run`.

The current capture observed these documented/public resources:

- `GET /api/status` — service identity and debugger capability.
- `GET /api/editor` — editor/wireless scope and selected-chip state.
- `GET /api/chips` — all chips accessible through the active scope.
- `GET /api/chips/{refId}/code?mode=chip` — JSON-wrapped compiled source;
  successful read observed.
- `PUT /api/chips/{refId}/code?mode=chip` — raw source write for a
  wireless/network-accessible chip; success and out-of-scope rejection
  observed.
- `PUT /api/chips/{refId}/code?mode=editor_then_chip` — raw source write for
  the exact selected open editor; chip and editor synchronization observed.

The public workflow documentation describes chip Pull/Export and
`source_version`, while the confirmed raw-code write contract is
`PUT /api/chips/{refId}/code` with the Lua source as the request body. A
read-only probe of the earlier guessed `GET /api/chips/882/source` route
returned `404 Not Found`; this remains only a route-investigation fixture.
The successful `/code` request and response shapes are captured with only the
tiny disposable fixture.

For normal Push, the implementation chooses `mode=editor_then_chip` only when
`/api/editor` reports an open editor whose selected chip and housing
ReferenceIds both match the target. Wireless/network-only accessibility uses
`mode=chip`. `editor_only` is deliberately excluded because normal Push is
expected to export to the chip, not merely update an editor draft.

## Fixture inventory

| Fixture | Label | What it proves |
| --- | --- | --- |
| `status.success.json` | observed | Reachable service identity, version, and debugger flag |
| `status.unavailable.connection-refused.json` | observed | Stopped/restarting service has no HTTP response and remains independent of the bridge |
| `status.incompatible.not-run.json` | not-run | Exact-version rejection is implemented and unit-tested, but not claimed as a live result |
| `editor.wireless.success.json` | observed | Wireless scope can expose multiple chips without a selected editor chip |
| `editor.selected-ticker.success.json` | observed | Exact selected chip and housing identity |
| `chips.wireless.success.json` | observed | Accessible chip metadata and chip/housing correlation |
| `chips.out-of-scope.editor-closed.json` | observed | `/api/chips` rejects a request without active editor/wireless scope |
| `chips.ticker.after-write.success.json` | observed | Source length, version 3, and selected state after writes |
| `correlation.scripted-screens.pre-fix.json` | observed/inferred | Observed outer-console mismatch and inferred composite-host cause |
| `correlation.scripted-screens.post-fix.success.json` | observed | Both screens resolve uniquely by housing in a wireless-only scope after restart |
| `source-read.route-404.json` | observed | The earlier guessed `/source` path is not valid |
| `source-read.mode-chip.success.json` | observed | `/code?mode=chip` returns JSON-wrapped source |
| `source-version.game-restart.observed.json` | observed | `source_version` is not durable across a game/service restart |
| `source-write.out-of-scope-400.json` | observed | A ReferenceId alone cannot bypass scope |
| `source-write.mode-chip.out-of-scope-400.json` | observed | `mode=chip` selects behavior but does not bypass scope |
| `source-write.mode-chip.success.json` | observed | Wireless write, no editor sync, version increment, and exact read-back |
| `source-write.editor-then-chip.success.json` | observed | Exact-editor write synchronizes editor and chip and increments the version |
| `source-write.precondition-unknown.json` | blocked | No documented expected-version/hash or atomic compare-and-set contract |

## Evidence labels

- `observed`: returned by the running service or directly captured from the
  repository's installed-version metadata.
- `documented`: stated by the official StationeersLua documentation.
- `inferred`: design interpretation, never sufficient to enable a mutation.
- `not-run`: planned but not attempted.
- `blocked`: cannot be validated under the public documented contract.

## Composite Scripted Screens

Direct `CircuitHousing` targets use exact bridge chip and housing ReferenceId
equality. Scripted Screens expose a different public identity shape: the
bridge can obtain the public `CurrentMotherboard.ReferenceId`, which
corresponds to StationeersLua's circuitboard `housing_ref_id`, but
StationeersLua alone supplies the nested Lua chip `ref_id` needed by
`/api/chips/{refId}/code`.

The bridge therefore marks this value as a housing identity. The extension
accepts it only when exactly one current `/api/chips` Lua record has that
`housing_ref_id`, then uses that record's `ref_id` for Pull, Compare, and Push.
Exact chip-and-housing matches take priority. Missing, duplicate, mismatched,
or non-Lua candidates remain unavailable. Names and network labels are never
used for identity correlation.

The unit/fixture contract and post-deployment wire capture are both
`observed`: Screen 1 mapped housing `1626` to chip `1702`, and Screen 2 mapped
housing `1589` to chip `1590`. The editor had no selected chip, so the
successful correlation did not depend on an open selected editor. Those
ReferenceIds were unchanged across this one game restart and reload of the
same world. The API does not document durable IDs across replacement, save
migration, or arbitrary sessions, so the extension still revalidates the
current responses rather than persisting the mapping.

## Official references

- Upstream StationeersLua documentation (consult the version bundled with the
  integration you are running).
- The local VS Code and REST setup is documented in this repository's
  [StationeersLua integration guide](../../guide/stationeers-lua.md).

The documentation pages currently warn that they are AI-generated and may be
inaccurate. That warning is why live responses are required before any client
or mutation contract is implemented.
