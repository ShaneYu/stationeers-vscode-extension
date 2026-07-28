import assert from "node:assert/strict";
import { test } from "node:test";
import { liveSourceIdentity, liveSourceKey, liveSourceLabel } from "./liveSourceModel.ts";

test("builds stable identity keys without exposing credentials", () => {
  const source = { worldEpoch: "world-1", chipId: "chip-7", housingReferenceId: "housing-8", language: "ic10", version: "12", length: 3, sha256: "a".repeat(64), source: "nop" };
  const identity = liveSourceIdentity(source);
  assert.deepEqual(identity, { worldEpoch: "world-1", chipId: "chip-7", housingReferenceId: "housing-8", language: "ic10" });
  assert.equal(liveSourceKey(identity), "world-1\u001fchip-7\u001fhousing-8\u001fic10");
  assert(!liveSourceKey(identity).includes("token"));
});

test("formats the meaningful untitled editor label", () => {
  assert.equal(liveSourceLabel("P302-Network", "P302-IC10"), "P302-Network — P302-IC10");
});
