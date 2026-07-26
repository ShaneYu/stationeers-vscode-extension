import assert from "node:assert/strict";
import test from "node:test";

import { scenarioFromEnvironmentProposal } from "./environmentProposalApplyModel.ts";
import {
  buildEnvironmentProposalPreview,
  type EnvironmentProposal,
} from "./environmentProposalModel.ts";

const proposal: EnvironmentProposal = {
  schemaVersion: 1,
  sourceUri: "file:///workspace/controller.ic10",
  previewOnly: true,
  housing: {
    suggestedId: "controller",
    suggestedName: "Controller",
    programUri: "file:///workspace/controller.ic10",
    prefab: {
      prefabName: "StructureCircuitHousing",
      prefabHash: 0,
      displayName: "Housing",
      confidence: 1,
      reason: "program host",
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
          prefabName: "SensorA",
          prefabHash: 1,
          displayName: "Sensor A",
          confidence: 0.9,
          reason: "exact alias",
        },
        {
          prefabName: "SensorB",
          prefabHash: 2,
          displayName: "Sensor B",
          confidence: 0.4,
          reason: "field compatible",
        },
      ],
      requiredFields: [{ name: "Setting", read: true, write: false }],
      requiredSlotFields: [],
      requiresMemory: false,
      confidence: 0.9,
      reasons: ["exact alias"],
      evidence: [],
    },
  ],
  batchGroups: [],
  networks: [
    {
      suggestedId: "shared-data",
      kind: "cable",
      cableRole: "data",
      participants: ["controller", "sensor"],
      channels: [0],
      reason: "shared data access",
    },
  ],
  unresolved: [
    {
      code: "ambiguous-device-prefab",
      message: "Confirm the sensor prefab.",
      reference: "d0",
    },
  ],
};
const catalog = {
  StructureCircuitHousing: {
    connections: [{ type: "Power" }, { type: "Data" }],
    slots: {},
  },
  SensorA: {
    connections: [{ type: "Data" }],
    slots: {},
  },
  SensorB: {
    connections: [{ type: "Power" }, { type: "Pipe" }, { type: "Data" }],
    slots: {},
  },
};

test("applies only explicit ranked candidate choices to one scenario value", () => {
  const preview = buildEnvironmentProposalPreview(proposal);
  assert.equal(preview.canApply, false);
  const scenario = scenarioFromEnvironmentProposal(
    preview,
    { d0: "SensorB" },
    "./controller.ic10",
    catalog,
  );
  assert.equal(scenario.devices[0]?.ic?.program, "./controller.ic10");
  assert.deepEqual(scenario.devices[0]?.ic?.pins, { d0: "sensor" });
  assert.equal(scenario.devices[1]?.prefab, "SensorB");
  assert.deepEqual(scenario.devices[0]?.connections, { "1": "shared-data" });
  assert.deepEqual(scenario.devices[1]?.connections, { "2": "shared-data" });
  assert.equal(scenario.devices[1]?.fields?.Setting, 0);
  assert.throws(
    () =>
      scenarioFromEnvironmentProposal(
        preview,
        { d0: "InventedPrefab" },
        "./controller.ic10",
        catalog,
      ),
    /valid prefab/,
  );
});

test("rejects duplicate stable IDs before writing the proposed scenario", () => {
  const duplicate: EnvironmentProposal = {
    ...proposal,
    devices: [
      {
        ...proposal.devices[0]!,
        suggestedId: proposal.housing.suggestedId,
      },
    ],
  };
  assert.throws(
    () =>
      scenarioFromEnvironmentProposal(
        buildEnvironmentProposalPreview(duplicate),
        { d0: "SensorA" },
        "./controller.ic10",
        catalog,
      ),
    /duplicate device ID/,
  );
});

test("derives Vending Machine data and slot indices from metadata", () => {
  const vendingProposal: EnvironmentProposal = {
    ...proposal,
    devices: [
      {
        ...proposal.devices[0]!,
        candidates: [
          {
            prefabName: "StructureVendingMachine",
            prefabHash: -443130773,
            displayName: "Vending Machine",
            confidence: 1,
            reason: "slot access",
          },
        ],
        requiredSlotFields: ["Occupied", "PrefabHash"],
      },
    ],
  };
  const scenario = scenarioFromEnvironmentProposal(
    buildEnvironmentProposalPreview(vendingProposal),
    { d0: "StructureVendingMachine" },
    "./controller.ic10",
    {
      ...catalog,
      StructureVendingMachine: {
        connections: [
          { type: "Chute" },
          { type: "Chute" },
          { type: "Data" },
          { type: "Power" },
        ],
        slots: {
          "5": {
            logicTypes: {
              Occupied: {},
              PrefabHash: {},
            },
          },
        },
      },
    },
  );
  assert.deepEqual(scenario.devices[1]?.connections, { "2": "shared-data" });
  assert.deepEqual(scenario.devices[1]?.slots, {
    "5": { Occupied: 0, PrefabHash: 0 },
  });
});
