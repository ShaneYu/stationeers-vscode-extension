# StationeersLua REST contract — 0.9.5.0

Capture date: 2026-07-29. Base URL: `http://127.0.0.1:3030`.

## Service status

`GET /api/status` returned HTTP 200 and the sanitized fixture
[`status.success.json`](fixtures/status.success.json).

Observed fields:

| Field | Observed type | Meaning/status |
| --- | --- | --- |
| `name` | string | `StationeersLua` |
| `status` | string | `ok` for the reachable service |
| `version` | string | `0.9.5.0` |
| `debugger_enabled` | boolean | `true` in this world |

The response does not report an API version, source capability bitset, write
capability, world/session epoch, or configured port. Those values remain
unknown. A service `200` therefore means reachable and identified, not safe to
write. The client accepts the exact observed service name and version
(`StationeersLua` `0.9.5.0`) and reports other versions as incompatible until
their wire contract is captured and validated.

## Scope and correlation

`GET /api/editor` returned HTTP 200 and the wireless-only fixture
[`editor.wireless.success.json`](fixtures/editor.wireless.success.json).

Observed scope fields include:

- `editor_open`
- `allow_network_chip_access`
- `allow_network_chip_access_only_for_wireless_boards`
- `mcp_debug_tools_available` and `mcp_debug_tools_reason`
- `wireless_remote_access_only`
- nullable `selected_chip_ref_id`, `selected_housing_ref_id`, and
  `selected_housing_name`
- `network_id`, `network_ids`, `network_names`
- `accessible_chip_count`
- `selected_chip_debugger_available` and
  `selected_chip_debugger_reason`

The wireless capture had `selected_chip_ref_id: null` and still exposed four
accessible chips. This supports the product boundary that global bridge
discovery can be broader than an editor selection.

After the disposable `Ticker` Lua program was selected in the in-game editor,
the same endpoint returned `selected_chip_ref_id: 882`,
`selected_housing_ref_id: 888`, and `selected_housing_name: "Ticker"`; see
[`editor.selected-ticker.success.json`](fixtures/editor.selected-ticker.success.json).
The corresponding `/api/chips` record reported the same `ref_id: 882`,
`housing_ref_id: 888`, `is_selected: true`, and `network_id: 1607`. This is a
successful observed correlation fixture for this disposable world. It does
not yet establish reload/session stability of those numeric IDs.

`GET /api/chips` returned HTTP 200 and
[`chips.wireless.success.json`](fixtures/chips.wireless.success.json). Each
record observed in this capture contains:

- chip `ref_id`;
- `is_lua`, `is_library`, `has_error`, and `is_selected`;
- `source_length` and `source_version`;
- `housing_name`, `housing_ref_id`, and `housing_type`;
- `network_id`;
- `modules` and `loaded_libraries`;
- optional `screen_size` for ScriptedScreens hosts;
- `is_on`.

For direct `CircuitHousing` targets, the active-scope cross-service
correlation is the pair of upstream chip `ref_id` and host `housing_ref_id`;
both matched the bridge's game ReferenceIds for Ticker in the observed
disposable world.

Scripted Screens are a documented-by-observation exception to the identity
shape. Before the bridge fix, Screen 1 and Screen 2 exposed outer-console IDs
that did not equal StationeersLua's nested chip `ref_id` or circuitboard
`housing_ref_id`; see
[`correlation.scripted-screens.pre-fix.json`](fixtures/correlation.scripted-screens.pre-fix.json).
The bridge now publishes the public `CurrentMotherboard.ReferenceId` as an
explicitly marked housing identity for a Lua console host. The extension may
resolve that identity only when exactly one current StationeersLua Lua record
has the same `housing_ref_id`, and it uses the returned `ref_id` for the REST
source route. Exact chip-and-housing identity takes priority; missing,
duplicate, mismatched, and non-Lua candidates fail closed. No names or network
labels participate in correlation.

The implementation revalidates correlation on every scope refresh and does
not persist the mapping across sessions. Post-deployment live validation
observed Screen 1 mapping housing `1626` to chip `1702` and Screen 2 mapping
housing `1589` to chip `1590`, each with one candidate and no selected editor;
see
[`correlation.scripted-screens.post-fix.success.json`](fixtures/correlation.scripted-screens.post-fix.success.json).
The same chip and housing ReferenceIds survived this one restart and reload of
the same world. Durable stability across chip replacement, save migration, or
arbitrary sessions remains undocumented and unverified.

## Source read/write contract

The official guide documents explicit Pull and Export actions and says the
service tracks `source_version`. The raw source write contract used here is
`PUT /api/chips/{refId}/code` with the Lua source as a `text/plain` request
body. The currently available public guide does not publish an expected
version/hash request field, world/session precondition, status-code matrix, or
atomicity guarantee.

When the player moved out of range and closed the editor, `GET /api/editor`
returned `editor_open: false` and `GET /api/chips` returned HTTP 400 with
`no IC editor open. The player must be interfacing with a computer that has an
IC editor motherboard.` See
[`chips.out-of-scope.editor-closed.json`](fixtures/chips.out-of-scope.editor-closed.json).
This is direct evidence that a bridge-visible chip is not automatically a
StationeersLua-addressable target when it is outside the active upstream scope.

With explicit authorization, a best-effort source write was then attempted
using the confirmed public route `PUT /api/chips/882/code` and a `text/plain`
body containing only the disposable probe. StationeersLua returned HTTP 400
with `success: false` and the same no-editor error; see
[`source-write.out-of-scope-400.json`](fixtures/source-write.out-of-scope-400.json).
No source change was applied. This confirms that possession of the game chip
ReferenceId does not bypass StationeersLua's current editor/wireless access
gate.

Repeating the same authorized write as
`PUT /api/chips/882/code?mode=chip` returned the identical HTTP 400 response;
see
[`source-write.mode-chip.out-of-scope-400.json`](fixtures/source-write.mode-chip.out-of-scope-400.json).
The `mode=chip` query parameter selects write semantics but is not an access
scope override.

`GET /api/chips/882/source` returned HTTP 404 in the live capture and is stored
as [`source-read.route-404.json`](fixtures/source-read.route-404.json). This
does not prove that source access is unsupported; it proves only that this
guessed route is not the route for this running service. The implementation
uses `GET /api/chips/{refId}/code?mode=chip` for compiled chip source.

That route returned HTTP 200 with `application/json` and these fields:

- `ref_id` — chip ReferenceId;
- `source` — Lua source string;
- `is_lua` — `true` for the tested target;
- `is_library` — library-chip flag.

See [`source-read.mode-chip.success.json`](fixtures/source-read.mode-chip.success.json).
Before mutation, the returned source was 770 UTF-8 bytes, matching
`/api/chips` metadata. After the disposable writes, the captured 60-byte
source and SHA-256 both matched the request exactly.

The wireless-only write
`PUT /api/chips/882/code?mode=chip` returned HTTP 200; see
[`source-write.mode-chip.success.json`](fixtures/source-write.mode-chip.success.json).
The JSON response fields were `success`, `ref_id`, `mode`, `editor_synced`,
`editor_sync_path`, `editor_sync_reason`, and `source_version`.
`editor_synced` was `false`, `editor_sync_path` was `not_attempted`, and
`source_version` advanced from 1 to 2.

With the exact Ticker chip open in the editor,
`PUT /api/chips/882/code?mode=editor_then_chip` also returned HTTP 200; see
[`source-write.editor-then-chip.success.json`](fixtures/source-write.editor-then-chip.success.json).
It reported `editor_synced: true`, `editor_sync_path: "vanilla"`, the reason
`editor draft updated via vanilla paste fallback`, and `source_version: 3`.
The chip read-back matched, and `/api/chips` subsequently reported
`source_length: 60`, `source_version: 3`, and `is_selected: true`; see
[`chips.ticker.after-write.success.json`](fixtures/chips.ticker.after-write.success.json).

After a game restart and reload of the same world, Ticker retained the same
chip/housing ReferenceIds and 60-byte source length, but `/api/chips` reported
`source_version: 1` rather than 3. No source body was read during this check;
see
[`source-version.game-restart.observed.json`](fixtures/source-version.game-restart.observed.json).
`source_version` must therefore be treated as session-local observation
metadata, not a durable world/session precondition.

### Write-mode rule

Normal Push applies this fail-closed mode selection:

| Current StationeersLua scope | Mode | Status |
| --- | --- | --- |
| `editor_open: true` and selected chip plus housing IDs exactly match the target | `editor_then_chip` | implemented and successful live write observed |
| Exact chip plus housing IDs appear in the active wireless/network chip list, without an exact open-editor selection | `chip` | implemented; successful write and out-of-scope rejection observed |
| Editor is closed, selected identity is partial/stale, or the chip/housing pair is absent or ambiguous | no request | implemented and fixture-tested |

`editor_only` is not used by normal Push because the public workflow
distinguishes updating the selected editor draft from explicitly exporting
source to the chip. The `editor_then_chip` branch keeps both representations
aligned when the exact target is actively open.

## Contract conclusion

Status, scope, metadata discovery, active-session chip/housing correlation,
source Pull, wireless Push, exact-editor Push, read-back, version increments,
and out-of-scope rejection are `observed`. The extension enables Pull,
Compare, and explicitly best-effort Push only for an exact current-scope
correlation.

Source hashes, expected-version/hash preconditions, world/session
preconditions, and atomicity remain `blocked`. Consequently Lua Push can
overwrite a newer in-game edit. The client does not retry, merge, or label the
operation conflict-safe, and it surfaces any upstream rejection. Debugging
remains outside this implementation slice.
