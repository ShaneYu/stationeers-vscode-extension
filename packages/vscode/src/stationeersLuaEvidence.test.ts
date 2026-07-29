import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";

const evidenceDir = path.resolve(process.cwd(), "../../docs/live-integration/stationeers-lua");

function fixture<T>(name: string): T {
  return JSON.parse(readFileSync(path.join(evidenceDir, "fixtures", name), "utf8")) as T;
}

test("StationeersLua observed fixtures preserve editor-to-chip correlation", () => {
  const editor = fixture<{ response: { selected_chip_ref_id: number; selected_housing_ref_id: number } }>("editor.selected-ticker.success.json");
  const chips = fixture<{ response: Array<{ ref_id: number; housing_ref_id: number; is_selected: boolean; is_lua: boolean }> }>("chips.wireless.success.json");
  const ticker = chips.response.find((chip) => chip.ref_id === editor.response.selected_chip_ref_id);

  assert.equal(editor.response.selected_chip_ref_id, 882);
  assert.equal(editor.response.selected_housing_ref_id, 888);
  assert.ok(ticker);
  assert.equal(ticker.ref_id, 882);
  assert.equal(ticker.housing_ref_id, 888);
  assert.equal(ticker.is_selected, false);
  assert.equal(ticker.is_lua, true);
});

test("Scripted Screen evidence preserves the composite identity mismatch", () => {
  const capture = fixture<{
    evidenceStatus: string;
    bridgeBeforeFix: Array<{ housingName: string; chipId: string; housingReferenceId: string }>;
    stationeersLua: Array<{ housing_name: string; ref_id: number; housing_ref_id: number; is_lua: boolean }>;
    result: string;
    postFixLiveVerification: { evidenceStatus: string; fixture: string };
  }>("correlation.scripted-screens.pre-fix.json");

  assert.equal(capture.evidenceStatus, "observed");
  assert.equal(capture.postFixLiveVerification.evidenceStatus, "observed");
  assert.equal(capture.postFixLiveVerification.fixture, "correlation.scripted-screens.post-fix.success.json");
  assert.equal(capture.bridgeBeforeFix.length, 2);
  assert.equal(capture.stationeersLua.length, 2);
  for (const bridgeChip of capture.bridgeBeforeFix) {
    assert.equal(
      capture.stationeersLua.some((candidate) =>
        String(candidate.ref_id) === bridgeChip.chipId
        || String(candidate.housing_ref_id) === bridgeChip.housingReferenceId),
      false,
      `${bridgeChip.housingName} must preserve the observed pre-fix mismatch`,
    );
  }
  assert.match(capture.result, /failed closed/);
});

test("Scripted Screens resolve to one StationeersLua chip by housing after the bridge fix", () => {
  const capture = fixture<{
    evidenceStatus: string;
    scope: { selected_chip_ref_id: null; selected_housing_ref_id: null; network_id: number };
    correlations: Array<{
      bridge: { chipId: string; housingReferenceId: string; identitySource: string };
      stationeersLua: { ref_id: number; housing_ref_id: number; is_lua: boolean; network_id: number };
      candidateCount: number;
      result: string;
    }>;
  }>("correlation.scripted-screens.post-fix.success.json");

  assert.equal(capture.evidenceStatus, "observed");
  assert.equal(capture.scope.selected_chip_ref_id, null);
  assert.equal(capture.scope.selected_housing_ref_id, null);
  assert.equal(capture.correlations.length, 2);
  assert.deepEqual(
    capture.correlations.map((entry) => [
      entry.bridge.housingReferenceId,
      String(entry.stationeersLua.ref_id),
      String(entry.stationeersLua.housing_ref_id),
    ]),
    [["1626", "1702", "1626"], ["1589", "1590", "1589"]],
  );
  for (const entry of capture.correlations) {
    assert.equal(entry.bridge.identitySource, "housing");
    assert.equal(entry.bridge.chipId, entry.bridge.housingReferenceId);
    assert.equal(entry.bridge.housingReferenceId, String(entry.stationeersLua.housing_ref_id));
    assert.equal(entry.stationeersLua.is_lua, true);
    assert.equal(entry.stationeersLua.network_id, capture.scope.network_id);
    assert.equal(entry.candidateCount, 1);
    assert.equal(entry.result, "unique_housing");
  }
});

test("StationeersLua service fixtures distinguish observed and not-run states", () => {
  const success = fixture<{ evidenceStatus: string; response: { name: string; version: string; status: string } }>("status.success.json");
  const unavailable = fixture<{ evidenceStatus: string; transport: { httpStatus: null; error: string } }>("status.unavailable.connection-refused.json");
  const incompatible = fixture<{ evidenceStatus: string; liveResponse: null; implementedResult: string }>("status.incompatible.not-run.json");

  assert.deepEqual(
    [success.evidenceStatus, success.response.name, success.response.version, success.response.status],
    ["observed", "StationeersLua", "0.9.5.0", "ok"],
  );
  assert.equal(unavailable.evidenceStatus, "observed");
  assert.equal(unavailable.transport.httpStatus, null);
  assert.match(unavailable.transport.error, /connection refused/);
  assert.equal(incompatible.evidenceStatus, "not-run");
  assert.equal(incompatible.liveResponse, null);
  assert.match(incompatible.implementedResult, /incompatible state/);
});

test("StationeersLua best-effort writes remain explicitly non-CAS", () => {
  const blocked = fixture<{ evidenceStatus: string; operation: string; requiredButUnverified: string[]; result: string }>("source-write.precondition-unknown.json");
  const restart = fixture<{
    evidenceStatus: string;
    beforeRestart: { source_length: number; source_version: number };
    afterRestartAndSameWorldReload: { source_length: number; source_version: number };
    sourceBodyReadDuringRestartCheck: boolean;
  }>("source-version.game-restart.observed.json");

  assert.equal(blocked.evidenceStatus, "blocked");
  assert.equal(blocked.operation, "atomic source write conflict handling");
  assert.ok(blocked.requiredButUnverified.includes("expected source version request field"));
  assert.match(blocked.result, /best-effort PUT/);
  assert.match(blocked.result, /overwrite newer in-game source/);
  assert.equal(restart.evidenceStatus, "observed");
  assert.equal(restart.beforeRestart.source_length, restart.afterRestartAndSameWorldReload.source_length);
  assert.notEqual(restart.beforeRestart.source_version, restart.afterRestartAndSameWorldReload.source_version);
  assert.equal(restart.sourceBodyReadDuringRestartCheck, false);
});

test("StationeersLua source fixtures preserve the observed JSON contract", () => {
  const read = fixture<{
    http: { method: string; path: string; status: number; contentType: string };
    response: { ref_id: number; source: string; is_lua: boolean; is_library: boolean };
    sourceBytes: number;
    sourceSha256: string;
  }>("source-read.mode-chip.success.json");
  const chipWrite = fixture<{
    response: { success: boolean; ref_id: number; mode: string; editor_synced: boolean; source_version: number };
  }>("source-write.mode-chip.success.json");
  const editorWrite = fixture<{
    precondition: { editor_open: boolean; selected_chip_ref_id: number; selected_housing_ref_id: number };
    response: { success: boolean; ref_id: number; mode: string; editor_synced: boolean; editor_sync_path: string; source_version: number };
  }>("source-write.editor-then-chip.success.json");

  assert.deepEqual(read.http, {
    method: "GET",
    path: "/api/chips/882/code?mode=chip",
    status: 200,
    contentType: "application/json",
  });
  assert.equal(read.response.ref_id, 882);
  assert.equal(read.response.is_lua, true);
  assert.equal(Buffer.byteLength(read.response.source), read.sourceBytes);
  assert.equal(createHash("sha256").update(read.response.source).digest("hex"), read.sourceSha256);

  assert.deepEqual(
    [chipWrite.response.mode, chipWrite.response.editor_synced, chipWrite.response.source_version],
    ["chip", false, 2],
  );
  assert.equal(editorWrite.precondition.editor_open, true);
  assert.equal(editorWrite.precondition.selected_chip_ref_id, 882);
  assert.equal(editorWrite.precondition.selected_housing_ref_id, 888);
  assert.deepEqual(
    [
      editorWrite.response.mode,
      editorWrite.response.editor_synced,
      editorWrite.response.editor_sync_path,
      editorWrite.response.source_version,
    ],
    ["editor_then_chip", true, "vanilla", 3],
  );
});
