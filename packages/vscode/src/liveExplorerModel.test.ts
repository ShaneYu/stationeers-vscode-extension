import assert from "node:assert/strict";
import { test } from "node:test";
import type { BridgeHello, BridgeSnapshot } from "./bridge.ts";
import { buildLiveExplorerRows, getLiveChipContext } from "./liveExplorerModel.ts";

test("projects duplicate chip appearances into stable per-scope tree keys", () => {
  const snapshot = { worldEpoch: "e", revision: "1", warnings: [], scopes: [{ scopeId: "b", name: "Greenhouse", disambiguator: "Area 3", anchorCount: 1, chipIds: ["9"] }, { scopeId: "a", name: "Greenhouse", anchorCount: 2, chipIds: ["9"] }], chips: [{ chipId: "9", housingReferenceId: "17", housingName: "Climate Controller", housingPrefab: "Housing", chipPrefab: "IC10", language: "ic10", powered: true, source: { readable: true, writable: false, version: "1", sha256: "x" } }] } satisfies BridgeSnapshot;
  const rows = buildLiveExplorerRows(snapshot);
  assert.deepEqual(rows.map((row) => row.key), ["scope:a", "chip:a:9", "scope:b", "chip:b:9"]);
});

test("derives selection menu state from handshake and chip capabilities", () => {
  const chip = { chipId: "ic", housingReferenceId: "1", housingName: "IC", housingPrefab: "Housing", chipPrefab: "IC10", language: "ic10", powered: true, source: { readable: true, writable: false, version: "1", sha256: "x" } } as const;
  const hello = { capabilities: { ic10SourceRead: true, ic10SourceWrite: false }, mods: { stationeersLua: { detected: false, version: null } } } as BridgeHello;
  assert.deepEqual(getLiveChipContext(chip, "connected", hello), { language: "ic10", stale: false, available: true, canRead: true, canCompare: true, luaDebugEligible: false });
  assert.equal(getLiveChipContext(chip, "stale", hello).canCompare, false);
  assert.equal(getLiveChipContext({ ...chip, source: { ...chip.source, readable: false } }, "connected", hello).canRead, false);
});
