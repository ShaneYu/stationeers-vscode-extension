import assert from "node:assert/strict";
import test from "node:test";

import {
  buildEnvironmentProposalPreview,
  validateEnvironmentProposal,
} from "./environmentProposalModel.ts";
import type { EnvironmentProposal } from "./environmentProposalModel.ts";

function proposal(): EnvironmentProposal {
  return {
    schemaVersion: 1,
    sourceUri: "file:///workspace/programs/%F0%9F%9A%80.ic10",
    previewOnly: true,
    housing: {
      suggestedId: "controller-housing",
      suggestedName: "Controller",
      programUri: "file:///workspace/programs/%F0%9F%9A%80.ic10",
      prefab: {
        prefabName: "StructureCircuitHousing",
        prefabHash: 0,
        displayName: "IC Housing",
        confidence: 1,
        reason: "IC program",
      },
      requiredFields: [],
      channels: [0],
    },
    devices: [
      {
        reference: "d0",
        aliases: ["sensor"],
        suggestedId: "sensor",
        pin: 0,
        candidates: [
          {
            prefabName: "StructureSensor",
            prefabHash: 42,
            displayName: "Sensor",
            confidence: 0.9,
            reason: "Supports Temperature",
          },
        ],
        requiredFields: [{ name: "Temperature", read: true, write: false }],
        requiredSlotFields: [],
        requiresMemory: false,
        confidence: 0.9,
        reasons: [],
        evidence: [],
      },
    ],
    batchGroups: [],
    networks: [],
    unresolved: [],
  };
}

test("builds a guarded preview without mutating proposal data", () => {
  const source = proposal();
  const preview = buildEnvironmentProposalPreview(source);
  assert.equal(preview.canApply, true);
  assert.equal(preview.selectedPrefabs.d0, "StructureSensor");
  assert.equal(preview.proposal, source);
});

test("keeps unresolved assumptions as explicit apply blockers", () => {
  const source = proposal();
  const unresolved: EnvironmentProposal = {
    ...source,
    unresolved: [
      {
        code: "dynamic-device-reference",
        message: "r0 resolves at runtime",
      },
    ],
  };
  const preview = buildEnvironmentProposalPreview(unresolved);
  assert.equal(preview.canApply, false);
  assert.deepEqual(preview.blockers, ["r0 resolves at runtime"]);
});

test("rejects cross-document and non-preview responses", () => {
  const source = proposal();
  assert.throws(
    () => validateEnvironmentProposal(source, "file:///other.ic10"),
    /cross-document/,
  );
  assert.throws(
    () =>
      validateEnvironmentProposal(
        { ...source, previewOnly: false },
        source.sourceUri,
      ),
    /incompatible/,
  );
});
