import assert from "node:assert/strict";
import { test } from "node:test";
import type { BridgeHello, BridgeSnapshot } from "./bridge.ts";
import { buildLiveExplorerRows, formatChipDescription, getLiveChipContext, projectLuaAccessibility } from "./liveExplorerModel.ts";

const luaChip = {
  chipId: "882",
  housingReferenceId: "888",
  housingName: "Ticker",
  housingPrefab: "CircuitHousing",
  chipPrefab: "Lua",
  language: "lua",
  powered: true,
  source: { readable: false, writable: false, version: "", sha256: "" },
} as const;

const activeLuaScope = {
  serviceAvailable: true,
  scopeAvailable: true,
  chips: [{ ref_id: 882, housing_ref_id: 888, is_lua: true, source_length: 60, source_version: 3 }],
} as const;

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

test("shows optional source length metadata on chip rows", () => {
  const chip = { chipId: "9", housingReferenceId: "17", housingName: "Climate Controller", housingPrefab: "Housing", chipPrefab: "IC10", language: "ic10", powered: true, source: { readable: true, writable: false, version: "1", sha256: "x", length: 42 } } as const;
  assert.equal(formatChipDescription(chip), "IC10 · powered · 42 bytes");
  assert.equal(formatChipDescription({ ...chip, source: { ...chip.source, length: undefined, bytes: 7 } }), "IC10 · powered · 7 bytes");
  assert.equal(formatChipDescription({ ...chip, source: { ...chip.source, length: -1 } }), "IC10 · powered");
});

test("marks a Lua chip accessible only on exact chip and housing identity", () => {
  assert.deepEqual(projectLuaAccessibility(luaChip, activeLuaScope), {
    accessible: true,
    reason: "accessible",
    accessStatus: "network_scope",
    correlation: "chip_and_housing",
    target: {
      refId: "882",
      housingRefId: "888",
      sourceLength: 60,
      sourceVersion: "3",
    },
    status: "Lua chip accessible",
    tooltip: "This Lua chip is accessible through the current StationeersLua network scope.",
    iconState: "accessible",
  });

  const mismatch = projectLuaAccessibility(luaChip, {
    ...activeLuaScope,
    chips: [{ ref_id: 882, housing_ref_id: 889, is_lua: true }],
  });
  assert.equal(mismatch?.accessible, false);
  assert.equal(mismatch?.reason, "identity-mismatch");
});

test("maps a console-hosted Lua board through one exact housing candidate", () => {
  const screen = {
    ...luaChip,
    chipId: "1626",
    housingReferenceId: "1626",
    housingName: "Screen 1",
    housingPrefab: "StructureConsole3x3",
    chipPrefab: "ProgrammableChipMotherboard",
    identitySource: "housing",
  } as const;
  const projection = projectLuaAccessibility(screen, {
    ...activeLuaScope,
    chips: [{
      ref_id: 1702,
      housing_ref_id: 1626,
      is_lua: true,
      source_length: 9166,
      source_version: 1,
    }],
  });
  assert.deepEqual(projection, {
    accessible: true,
    reason: "accessible",
    accessStatus: "network_scope",
    correlation: "unique_housing",
    target: {
      refId: "1702",
      housingRefId: "1626",
      sourceLength: 9166,
      sourceVersion: "1",
    },
    status: "Lua chip accessible",
    tooltip: "This Lua chip is accessible through the current StationeersLua network scope.",
    iconState: "accessible",
  });
  assert.equal(formatChipDescription(screen, projection?.target?.sourceLength), "LUA · powered · 9166 bytes");

  const selected = projectLuaAccessibility(screen, {
    ...activeLuaScope,
    chips: [{ ref_id: 1702, housing_ref_id: 1626, is_lua: true }],
    editorOpen: true,
    selectedChipRefId: 1702,
    selectedHousingRefId: 1626,
  });
  assert.equal(selected?.accessStatus, "selected_editor");
  assert.equal(selected?.target?.refId, "1702");
});

test("fails closed when housing correlation is ambiguous or not explicitly permitted", () => {
  const composite = {
    ...luaChip,
    chipId: "1626",
    housingReferenceId: "1626",
    identitySource: "housing",
  } as const;
  const duplicate = projectLuaAccessibility(composite, {
    ...activeLuaScope,
    chips: [
      { ref_id: 1702, housing_ref_id: 1626, is_lua: true },
      { ref_id: 1703, housing_ref_id: 1626, is_lua: true },
    ],
  });
  assert.equal(duplicate?.reason, "duplicate-chip");
  assert.equal(duplicate?.target, null);

  const unmarked = projectLuaAccessibility(
    { ...composite, identitySource: undefined },
    { ...activeLuaScope, chips: [{ ref_id: 1702, housing_ref_id: 1626, is_lua: true }] },
  );
  assert.equal(unmarked?.reason, "missing-chip");

  const mismatchedExactIdentity = projectLuaAccessibility(composite, {
    ...activeLuaScope,
    chips: [
      { ref_id: 1626, housing_ref_id: 9999, is_lua: true },
      { ref_id: 1702, housing_ref_id: 1626, is_lua: true },
    ],
  });
  assert.equal(mismatchedExactIdentity?.reason, "identity-mismatch");
  assert.equal(mismatchedExactIdentity?.target, null);
});

test("distinguishes the exact selected editor chip from network-scope access", () => {
  const selected = projectLuaAccessibility(luaChip, {
    ...activeLuaScope,
    editorOpen: true,
    selectedChipRefId: 882,
    selectedHousingRefId: 888,
  });
  assert.equal(selected?.accessStatus, "selected_editor");
  assert.match(selected?.tooltip ?? "", /selected in the Stationeers editor/);

  const wrongHousing = projectLuaAccessibility(luaChip, {
    ...activeLuaScope,
    editorOpen: true,
    selectedChipRefId: 882,
    selectedHousingRefId: 889,
  });
  assert.equal(wrongHousing?.accessible, true);
  assert.equal(wrongHousing?.accessStatus, "network_scope");

  const partialIdentity = projectLuaAccessibility(luaChip, {
    ...activeLuaScope,
    editorOpen: true,
    selectedChipRefId: 882,
    selectedHousingRefId: null,
  });
  assert.equal(partialIdentity?.accessStatus, "network_scope");

  const closedEditor = projectLuaAccessibility(luaChip, {
    ...activeLuaScope,
    editorOpen: false,
    selectedChipRefId: 882,
    selectedHousingRefId: 888,
  });
  assert.equal(closedEditor?.accessStatus, "network_scope");
});

test("keeps duplicate, missing, and out-of-scope Lua chips inaccessible", () => {
  const duplicate = projectLuaAccessibility(luaChip, {
    ...activeLuaScope,
    chips: [
      { ref_id: 882, housing_ref_id: 888, is_lua: true },
      { ref_id: "882", housing_ref_id: "888", is_lua: true },
    ],
  });
  assert.equal(duplicate?.reason, "duplicate-chip");
  assert.equal(duplicate?.accessible, false);
  assert.equal(duplicate?.accessStatus, null);

  const missing = projectLuaAccessibility(luaChip, { ...activeLuaScope, chips: [] });
  assert.equal(missing?.reason, "missing-chip");
  assert.equal(missing?.status, "Lua chip out of scope");

  const noScope = projectLuaAccessibility(luaChip, { ...activeLuaScope, scopeAvailable: false, chips: [] });
  assert.equal(noScope?.reason, "no-scope");
  assert.match(noScope?.tooltip ?? "", /Wireless Development Board/);
});

test("reports the StationeersLua service as unavailable", () => {
  const projection = projectLuaAccessibility(luaChip, {
    serviceAvailable: false,
    scopeAvailable: false,
    chips: [],
  });
  assert.equal(projection?.reason, "service-unavailable");
  assert.equal(projection?.iconState, "unavailable");
  assert.equal(projection?.accessible, false);
});

test("does not project Lua accessibility onto IC10 chips", () => {
  const chip = { ...luaChip, language: "ic10" } as const;
  assert.equal(projectLuaAccessibility(chip, activeLuaScope), undefined);

  const hello = { capabilities: { ic10SourceRead: true, ic10SourceWrite: false } } as BridgeHello;
  assert.deepEqual(getLiveChipContext(chip, "connected", hello), {
    language: "ic10",
    stale: false,
    available: true,
    canRead: false,
    canCompare: false,
    luaDebugEligible: false,
  });
});
