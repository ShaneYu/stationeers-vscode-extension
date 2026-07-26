const assert: typeof import("node:assert/strict") = require("node:assert/strict");
const { test }: typeof import("node:test") = require("node:test");

const {
  buildEnvironmentTopologyView,
  duplicateTopologySelection,
  environmentTopologyDiagnostics,
  inspectorSelectionForTopology,
  savedTopologyLayout,
  topologyLayoutFilename,
}: typeof import("./environmentTopologyController") = require("./environmentTopologyController.ts");
import type { EnvironmentScenario } from "./environmentTopologyModel";

const scenario: EnvironmentScenario = {
  schemaVersion: 1,
  networks: [
    { id: "data", kind: "cable", cableRole: "powerAndData" },
    { id: "gas", kind: "gas" },
  ],
  devices: [
    {
      id: "ic",
      prefab: "Housing",
      connections: { "0": "data", "1": "gas" },
      ic: { program: "main.ic10" },
    },
  ],
};

test("derives deterministic labelled topology without changing scenario data", () => {
  const before = structuredClone(scenario);
  const view = buildEnvironmentTopologyView(scenario, {
    Housing: {
      displayName: "IC Housing",
      connections: [
        { type: "Data", role: "Input" },
        { type: "Gas", role: "Output" },
      ],
    },
  });
  assert.equal(view.nodes.length, 3);
  assert(view.edges.some(({ label }) => label.includes("Input")));
  assert(view.edges.some(({ direction }) => direction === "toDevice"));
  assert(view.edges.some(({ networkKind }) => networkKind === "gas"));
  assert(view.focusItems.some(({ kind }) => kind === "port"));
  assert.deepEqual(scenario, before);
});

test("maps duplicate, disconnected, and incompatible topology validation", () => {
  const invalid: EnvironmentScenario = {
    schemaVersion: 1,
    networks: [
      { id: "power", kind: "cable", cableRole: "power" },
      { id: "unused", kind: "chute" },
      { id: "unused", kind: "gas" },
    ],
    devices: [
      {
        id: "sensor",
        prefab: "Sensor",
        connections: { "0": "power" },
      },
    ],
  };
  const diagnostics = environmentTopologyDiagnostics(invalid, {
    Sensor: { connections: [{ type: "Data", role: "Input" }] },
  });
  assert(diagnostics.some(({ message }) => message.includes("Duplicate network")));
  assert(diagnostics.some(({ message }) => message.includes("disconnected")));
  assert(diagnostics.some(({ message }) => message.includes("incompatible")));
  const view = buildEnvironmentTopologyView(invalid, {
    Sensor: { connections: [{ type: "Data", role: "Input" }] },
  });
  assert(view.nodes.some(({ validationState }) => validationState === "error"));
  assert(view.edges.some(({ validationState }) => validationState === "error"));
});

test("keeps layout in a sidecar and prunes stale keys", () => {
  assert.equal(
    topologyLayoutFilename("station.ic10sim.json"),
    "station.ic10sim.layout.json",
  );
  const view = buildEnvironmentTopologyView(scenario, {});
  const layout = savedTopologyLayout(scenario, {}, {
    ...view.positions,
    stale: { x: 1, y: 2 },
  });
  assert.equal(layout.nodes.stale, undefined);
  assert.equal(Object.keys(layout.nodes).length, 3);
});

test("synchronizes topology selections and duplicates guarded objects", () => {
  const view = buildEnvironmentTopologyView(scenario, {});
  const ic = view.nodes.find(({ id }) => id === "ic")!;
  assert.deepEqual(
    inspectorSelectionForTopology(scenario, {
      kind: "node",
      nodeKey: ic.key,
    }),
    { type: "device", index: 0 },
  );
  const duplicate = duplicateTopologySelection(scenario, {
    kind: "network",
    id: "data",
  });
  assert(duplicate.networks.some(({ id }) => id === "data-copy"));
  assert(duplicate.devices.some(({ id }) => id === "ic-copy"));
});
