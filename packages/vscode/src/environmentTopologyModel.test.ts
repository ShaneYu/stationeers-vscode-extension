import assert from "node:assert/strict";
import test from "node:test";

import {
  applyFragmentImport,
  automaticTopologyLayout,
  buildTopologyFocusItems,
  buildTopologyGraph,
  duplicateDevice,
  duplicateSubnetwork,
  exportTopologyFragment,
  filterTopologyNodes,
  mapValidationTarget,
  moveRovingFocus,
  normalizeEnvironmentLayout,
  parseEnvironmentLayoutSidecar,
  parseTopologyFragment,
  previewFragmentImport,
  reconcileTopologySelection,
  reconcileTopologySelectionWithFallback,
  spatialFocusNeighbour,
  topologyDestinationFingerprint,
  topologyNodeKey,
  topologyReadingOrder,
  topologySelectionKey,
} from "./environmentTopologyModel.ts";
import type {
  EnvironmentLayoutSidecar,
  EnvironmentScenario,
  TopologyCatalog,
} from "./environmentTopologyModel";

function scenario(): EnvironmentScenario {
  return {
    schemaVersion: 1,
    gameVersion: "1.2.3",
    networks: [
      { id: "data", kind: "cable", cableRole: "powerAndData" },
      { id: "chute", kind: "chute" },
      { id: "unused", kind: "gas" },
    ],
    devices: [
      {
        id: "controller",
        prefab: "StructureCircuitHousing",
        name: "Main Controller",
        referenceId: 10,
        connections: { "0": "data" },
        fields: { Setting: 3 },
        ic: {
          program: "main.ic10",
          pins: { d0: "vendor" },
          registers: { r0: 2 },
        },
      },
      {
        id: "vendor",
        prefab: "StructureVendingMachine",
        name: "Iron Vendor",
        referenceId: 11,
        connections: { "1": "chute", "2": "data" },
        fields: { Activate: 0 },
        slots: { "2": { Occupied: 1, Quantity: 50 } },
      },
    ],
  };
}

const catalog: TopologyCatalog = {
  StructureCircuitHousing: {
    displayName: "IC Housing",
    connections: [{ type: "Data", role: "Input" }],
  },
  StructureVendingMachine: {
    displayName: "Vending Machine",
    connections: [
      { type: "Chute", role: "Input" },
      { type: "Chute", role: "Output" },
      { type: "Data", role: "None" },
    ],
  },
};

test("builds stable network, device, connection, and pin identities", () => {
  const graph = buildTopologyGraph(scenario(), catalog);
  assert.equal(graph.nodes.length, 5);
  assert.equal(graph.edges.length, 4);
  const controllerKey = topologyNodeKey({
    kind: "device",
    id: "controller",
    occurrence: 0,
  });
  const connection = graph.edges.find(
    (edge) =>
      edge.kind === "connection" &&
      edge.deviceKey === controllerKey &&
      edge.connectionKey === "0",
  );
  assert(connection?.kind === "connection");
  assert.deepEqual(
    {
      networkId: connection.networkId,
      type: connection.connectionType,
      role: connection.connectionRole,
    },
    { networkId: "data", type: "Data", role: "Input" },
  );
  const pin = graph.edges.find((edge) => edge.kind === "pin");
  assert(pin?.key.includes("d0"));
  assert.equal(pin?.validationState, "valid");
});

test("disambiguates duplicate source IDs with occurrence identities", () => {
  const input = scenario();
  input.devices.push({
    id: "vendor",
    prefab: "StructureVendingMachine",
  });
  const graph = buildTopologyGraph(input);
  const keys = graph.nodes
    .filter((node) => node.identity.id === "vendor")
    .map(({ key }) => key);
  assert.equal(new Set(keys).size, 2);
  assert(keys.some((key) => key.endsWith(":0")));
  assert(keys.some((key) => key.endsWith(":1")));
  const pin = graph.edges.find((edge) => edge.kind === "pin");
  assert.equal(pin?.validationState, "error");
});

test("maps structured diagnostics to exact nodes, ports, connection edges, and pins", () => {
  const graph = buildTopologyGraph(scenario(), catalog);
  const device = mapValidationTarget(graph, {
    entity: "device",
    deviceId: "vendor",
  });
  assert.equal(device?.kind, "node");
  const connection = mapValidationTarget(graph, {
    deviceId: "vendor",
    property: "connections.2",
  });
  assert.equal(connection?.kind, "edge");
  const absentPort = mapValidationTarget(graph, {
    deviceId: "vendor",
    connection: 9,
  });
  assert.equal(absentPort?.kind, "port");
  const pin = mapValidationTarget(graph, {
    entity: "pin",
    icId: "controller",
    property: "pins.d0",
  });
  assert.equal(pin?.kind, "pin");
  const network = mapValidationTarget(graph, {
    entity: "network",
    networkIndex: 1,
  });
  assert.equal(network?.kind, "node");
});

test("uses an index fallback to reveal one of several duplicate IDs", () => {
  const input = scenario();
  input.networks.push({ id: "data", kind: "cable" });
  const graph = buildTopologyGraph(input);
  assert.equal(
    mapValidationTarget(graph, { entity: "network", networkId: "data" }),
    undefined,
  );
  const selection = mapValidationTarget(graph, {
    entity: "network",
    networkId: "data",
    networkIndex: 3,
  });
  assert.equal(selection?.kind, "node");
  if (selection?.kind === "node") {
    assert.equal(graph.nodeByKey.get(selection.nodeKey)?.sourceIndex, 3);
  }
});

test("promotes validation state from an edge to both attached nodes", () => {
  const graph = buildTopologyGraph(scenario(), catalog, [
    {
      severity: "warning",
      message: "check this cable",
      target: { deviceId: "vendor", connection: 2 },
    },
    {
      severity: "error",
      message: "broken cable",
      target: { deviceId: "vendor", connection: 2 },
    },
  ]);
  const selected = mapValidationTarget(graph, {
    deviceId: "vendor",
    connection: 2,
  });
  assert(selected?.kind === "edge");
  assert.equal(graph.edgeByKey.get(selected.edgeKey)?.validationState, "error");
  const edge = graph.edgeByKey.get(selected.edgeKey);
  assert(edge?.kind === "connection");
  assert.equal(graph.nodeByKey.get(edge.deviceKey)?.validationState, "error");
  assert.equal(
    graph.nodeByKey.get(edge.networkKey!)?.validationState,
    "error",
  );
});

test("automatic layout is deterministic across source ordering", () => {
  const original = scenario();
  const reordered: EnvironmentScenario = {
    ...scenario(),
    networks: [...original.networks].reverse(),
    devices: [...original.devices].reverse(),
  };
  const first = automaticTopologyLayout(buildTopologyGraph(original));
  const second = automaticTopologyLayout(buildTopologyGraph(reordered));
  assert.deepEqual(second, first);
  const points = Object.values(first);
  assert.equal(
    new Set(points.map(({ x, y }) => `${x},${y}`)).size,
    points.length,
  );
});

test("saved positions override automatic layout only for known nodes", () => {
  const graph = buildTopologyGraph(scenario());
  const key = topologyNodeKey({
    kind: "device",
    id: "controller",
    occurrence: 0,
  });
  const layout = automaticTopologyLayout(graph, {
    schemaVersion: 1,
    nodes: {
      [key]: { x: -42, y: 73 },
      missing: { x: 1, y: 2 },
    },
  });
  assert.deepEqual(layout[key], { x: -42, y: 73 });
  assert.equal(layout.missing, undefined);
});

test("parses, validates, and prunes the non-semantic layout sidecar", () => {
  const graph = buildTopologyGraph(scenario());
  const key = graph.nodes[0]!.key;
  const parsed = parseEnvironmentLayoutSidecar(
    JSON.stringify({
      schemaVersion: 1,
      nodes: {
        [key]: { x: 10, y: 20 },
        stale: { x: 30, y: 40 },
      },
      viewport: { x: 5, y: 6, zoom: 1.5 },
    }),
    graph,
  );
  assert.deepEqual(parsed.errors, []);
  assert.deepEqual(parsed.layout, {
    schemaVersion: 1,
    nodes: { [key]: { x: 10, y: 20 } },
    viewport: { x: 5, y: 6, zoom: 1.5 },
  });
  assert(
    parseEnvironmentLayoutSidecar('{"schemaVersion":1,"nodes":{"x":{"x":"bad"}}}')
      .errors.length > 0,
  );
  assert(
    parseEnvironmentLayoutSidecar(
      '{"schemaVersion":1,"nodes":{},"viewport":{"x":0,"y":0,"zoom":0}}',
    ).errors.some((error) => error.includes("zoom from")),
  );
});

test("normalizes layout ordering without changing scenario semantics", () => {
  const graph = buildTopologyGraph(scenario());
  const [first, second] = graph.nodes;
  const input: EnvironmentLayoutSidecar = {
    schemaVersion: 1,
    nodes: {
      stale: { x: 9, y: 9 },
      [second!.key]: { x: 2, y: 2 },
      [first!.key]: { x: 1, y: 1 },
    },
  };
  const normalized = normalizeEnvironmentLayout(input, graph);
  assert.deepEqual(Object.keys(normalized.nodes), [first!.key, second!.key].sort());
  assert.equal(JSON.stringify(scenario()).includes('"nodes"'), false);
});

test("reconciles stable selection and produces accessible selection keys", () => {
  const graph = buildTopologyGraph(scenario());
  const selected = { kind: "node" as const, nodeKey: graph.nodes[0]!.key };
  assert.deepEqual(reconcileTopologySelection(graph, selected), selected);
  assert.equal(
    topologySelectionKey(selected),
    `node:${graph.nodes[0]!.key}`,
  );
  assert.equal(
    reconcileTopologySelection(graph, { kind: "node", nodeKey: "gone" }),
    undefined,
  );
  assert.deepEqual(
    reconcileTopologySelection(graph, {
      kind: "port",
      nodeKey: graph.nodes.find(({ identity }) => identity.id === "vendor")!.key,
      connectionKey: "9",
    }),
    {
      kind: "port",
      nodeKey: graph.nodes.find(({ identity }) => identity.id === "vendor")!.key,
      connectionKey: "9",
    },
  );
});

test("duplicates a device with deterministic safe identity and no ReferenceId collision", () => {
  const input = scenario();
  input.devices.push({
    id: "vendor-copy",
    prefab: "StructureVendingMachine",
    name: "Iron Vendor Copy",
  });
  const duplicate = duplicateDevice(input, "vendor");
  const copy = duplicate.scenario.devices.at(-1)!;
  assert.equal(copy.id, "vendor-copy-2");
  assert.equal(copy.name, "Iron Vendor Copy-2");
  assert.equal(copy.referenceId, undefined);
  assert.deepEqual(copy.connections, { "1": "chute", "2": "data" });
  assert.deepEqual(input.devices[1]!.slots, {
    "2": { Occupied: 1, Quantity: 50 },
  });
  copy.slots!["2"]!.Quantity = 0;
  assert.equal(input.devices[1]!.slots!["2"]!.Quantity, 50);
});

test("duplicates a complete subnetwork and remaps internal connections and pins", () => {
  const duplicate = duplicateSubnetwork(scenario(), {
    networkIds: ["data", "chute"],
    includeConnectedDevices: true,
  });
  assert.deepEqual(duplicate.networkIdRemap, {
    data: "data-copy",
    chute: "chute-copy",
  });
  assert.deepEqual(duplicate.deviceIdRemap, {
    controller: "controller-copy",
    vendor: "vendor-copy",
  });
  const controller = duplicate.scenario.devices.find(
    ({ id }) => id === "controller-copy",
  )!;
  const vendor = duplicate.scenario.devices.find(
    ({ id }) => id === "vendor-copy",
  )!;
  assert.deepEqual(controller.connections, { "0": "data-copy" });
  assert.deepEqual(controller.ic?.pins, { d0: "vendor-copy" });
  assert.deepEqual(vendor.connections, {
    "1": "chute-copy",
    "2": "data-copy",
  });
  assert.equal(controller.referenceId, undefined);
});

test("safe subnetwork duplication drops external connections and pins with warnings", () => {
  const duplicate = duplicateSubnetwork(scenario(), {
    networkIds: ["data"],
    deviceIds: ["controller"],
    includeConnectedDevices: false,
  });
  const controller = duplicate.scenario.devices.find(
    ({ id }) => id === "controller-copy",
  )!;
  assert.deepEqual(controller.ic?.pins, {});
  assert(duplicate.warnings.some((warning) => warning.includes("pin d0")));
});

test("exports a self-contained fragment and prunes unrelated layout", () => {
  const graph = buildTopologyGraph(scenario());
  const layout: EnvironmentLayoutSidecar = {
    schemaVersion: 1,
    nodes: Object.fromEntries(
      graph.nodes.map((node, index) => [
        node.key,
        { x: index * 10, y: index * 20 },
      ]),
    ),
  };
  const { fragment } = exportTopologyFragment(scenario(), {
    deviceIds: ["vendor"],
    includeReferencedNetworks: true,
    layout,
  });
  assert.deepEqual(
    fragment.networks.map(({ id }) => id),
    ["data", "chute"],
  );
  assert.deepEqual(fragment.devices.map(({ id }) => id), ["vendor"]);
  assert.equal(Object.keys(fragment.layout!.nodes).length, 3);
});

test("previews fragment conflicts and deterministically remaps all references", () => {
  const source = scenario();
  const graph = buildTopologyGraph(source);
  const { fragment } = exportTopologyFragment(source, {
    deviceIds: ["controller", "vendor"],
    includeReferencedNetworks: true,
    layout: {
      schemaVersion: 1,
      nodes: Object.fromEntries(
        graph.nodes.map((node, index) => [
          node.key,
          { x: index, y: index + 1 },
        ]),
      ),
    },
  });
  const preview = previewFragmentImport(source, fragment);
  assert.deepEqual(preview.networkIdRemap, {
    data: "data-import",
    chute: "chute-import",
  });
  assert.deepEqual(preview.deviceIdRemap, {
    controller: "controller-import",
    vendor: "vendor-import",
  });
  const controller = preview.fragment.devices.find(
    ({ id }) => id === "controller-import",
  )!;
  assert.deepEqual(controller.connections, { "0": "data-import" });
  assert.deepEqual(controller.ic?.pins, { d0: "vendor-import" });
  assert.equal(controller.referenceId, undefined);
  assert(preview.warnings.some((warning) => warning.includes("ReferenceId")));
  const applied = applyFragmentImport(source, preview);
  assert.equal(applied.networks.length, source.networks.length + 2);
  assert.equal(applied.devices.length, source.devices.length + 2);
  assert.equal(source.devices.length, 2);
  assert(
    Object.keys(preview.fragment.layout!.nodes).every((key) =>
      key.includes("-import"),
    ),
  );
});

test("parses guarded fragment JSON and rejects ambiguous duplicate identities", () => {
  const valid = parseTopologyFragment(
    JSON.stringify(
      exportTopologyFragment(scenario(), {
        deviceIds: ["vendor"],
        includeReferencedNetworks: true,
      }).fragment,
    ),
  );
  assert.deepEqual(valid.errors, []);
  assert.equal(valid.fragment?.devices[0]?.id, "vendor");
  const duplicate = parseTopologyFragment(
    JSON.stringify({
      schemaVersion: 1,
      networks: [
        { id: "same", kind: "cable" },
        { id: "same", kind: "chute" },
      ],
      devices: [],
    }),
  );
  assert(
    duplicate.errors.some((error) =>
      error.includes("duplicate network ID “same”"),
    ),
  );
  assert.throws(
    () =>
      previewFragmentImport(scenario(), {
        schemaVersion: 1,
        networks: [
          { id: "same", kind: "cable" },
          { id: "same", kind: "cable" },
        ],
        devices: [],
      }),
    /duplicate network ID/,
  );
});

test("preserves resolvable external fragment references and drops unknown ones", () => {
  const { fragment } = exportTopologyFragment(scenario(), {
    deviceIds: ["vendor"],
    includeReferencedNetworks: false,
  });
  fragment.devices[0]!.connections!["9"] = "unknown";
  const preview = previewFragmentImport(scenario(), fragment);
  assert.deepEqual(preview.fragment.devices[0]!.connections, {
    "1": "chute",
    "2": "data",
  });
  assert(preview.warnings.some((warning) => warning.includes("unknown")));
});

test("searches semantic fields and combines prefab, IC, network, and validation filters", () => {
  const graph = buildTopologyGraph(scenario(), catalog, [
    {
      severity: "warning",
      message: "review",
      target: { entity: "device", deviceId: "vendor" },
    },
  ]);
  assert.deepEqual(
    filterTopologyNodes(graph, { query: "iron quantity" }).map(
      ({ identity }) => identity.id,
    ),
    ["vendor"],
  );
  assert.deepEqual(
    filterTopologyNodes(graph, { icOnly: true }).map(
      ({ identity }) => identity.id,
    ),
    ["controller"],
  );
  assert.deepEqual(
    filterTopologyNodes(graph, {
      prefabs: ["structurevendingmachine"],
      validationStates: ["warning"],
    }).map(({ identity }) => identity.id),
    ["vendor"],
  );
  assert.deepEqual(
    filterTopologyNodes(graph, { networkKinds: ["CHUTE"] }).map(
      ({ identity }) => identity.id,
    ),
    ["chute"],
  );
});

test("supports wrapping roving focus for keyboard-only list navigation", () => {
  const keys = ["a", "b", "c"];
  assert.equal(moveRovingFocus(keys, undefined, "next"), "a");
  assert.equal(moveRovingFocus(keys, "c", "next"), "a");
  assert.equal(moveRovingFocus(keys, "a", "previous"), "c");
  assert.equal(moveRovingFocus(keys, "b", "home"), "a");
  assert.equal(moveRovingFocus(keys, "b", "end"), "c");
  assert.equal(moveRovingFocus([], undefined, "next"), undefined);
});

test("chooses deterministic directional neighbours for spatial keyboard navigation", () => {
  const positions = {
    centre: { x: 100, y: 100 },
    left: { x: 20, y: 100 },
    right: { x: 180, y: 100 },
    up: { x: 100, y: 20 },
    down: { x: 100, y: 180 },
    "right-diagonal": { x: 150, y: 170 },
  };
  assert.equal(spatialFocusNeighbour(positions, "centre", "left"), "left");
  assert.equal(spatialFocusNeighbour(positions, "centre", "right"), "right");
  assert.equal(
    spatialFocusNeighbour(positions, "centre", "right", ["centre", "right"]),
    "right",
  );
  assert.equal(spatialFocusNeighbour(positions, "centre", "up"), "up");
  assert.equal(spatialFocusNeighbour(positions, "centre", "down"), "down");
  assert.equal(spatialFocusNeighbour(positions, "missing", "down"), undefined);
});

test("treats magic object property names as ordinary IDs in every remap namespace", () => {
  const input: EnvironmentScenario = {
    schemaVersion: 1,
    networks: [
      { id: "__proto__", kind: "cable" },
      { id: "constructor", kind: "chute" },
    ],
    devices: [
      {
        id: "toString",
        prefab: "Housing",
        connections: { "0": "__proto__", "1": "constructor" },
      },
    ],
  };
  const duplicate = duplicateSubnetwork(input, {
    networkIds: ["__proto__", "constructor"],
    deviceIds: ["toString"],
    includeConnectedDevices: false,
  });
  assert.equal(duplicate.networkIdRemap["__proto__"], "__proto__-copy");
  assert.equal(duplicate.networkIdRemap.constructor, "constructor-copy");
  assert.equal(duplicate.deviceIdRemap.toString, "toString-copy");
  assert.deepEqual(
    duplicate.scenario.devices.at(-1)?.connections,
    { "0": "__proto__-copy", "1": "constructor-copy" },
  );

  const sameNamespace: EnvironmentScenario = {
    schemaVersion: 1,
    networks: [{ id: "same", kind: "cable" }],
    devices: [{ id: "same", prefab: "Housing", connections: { "0": "same" } }],
  };
  const sameDuplicate = duplicateSubnetwork(sameNamespace, {
    networkIds: ["same"],
    deviceIds: ["same"],
    includeConnectedDevices: false,
  });
  assert.equal(sameDuplicate.networkIdRemap.same, "same-copy");
  assert.equal(sameDuplicate.deviceIdRemap.same, "same-copy");
});

test("preserves distinct raw invalid connection keys for diagnostics and focus", () => {
  const input = scenario();
  input.devices[1]!.connections = { foo: "data", bar: "data" };
  const graph = buildTopologyGraph(input, catalog);
  const connections = graph.edges.filter(
    (edge) => edge.kind === "connection" && edge.deviceId === "vendor",
  );
  assert.deepEqual(
    connections
      .map((edge) => (edge.kind === "connection" ? edge.connectionKey : ""))
      .sort(),
    ["bar", "foo"],
  );
  assert.equal(new Set(connections.map(({ key }) => key)).size, 2);
  assert(connections.every(({ validationState }) => validationState === "error"));
  const target = mapValidationTarget(graph, {
    deviceId: "vendor",
    property: "connections.foo",
  });
  assert.equal(target?.kind, "edge");
});

test("persists layouts only for unique identities and enforces schema bounds", () => {
  const input = scenario();
  input.devices.push({ id: "vendor", prefab: "Duplicate" });
  const graph = buildTopologyGraph(input);
  const duplicateKey = topologyNodeKey({
    kind: "device",
    id: "vendor",
    occurrence: 0,
  });
  const uniqueKey = topologyNodeKey({
    kind: "device",
    id: "controller",
    occurrence: 0,
  });
  const parsed = parseEnvironmentLayoutSidecar(
    JSON.stringify({
      schemaVersion: 1,
      nodes: {
        [duplicateKey]: { x: 4, y: 5 },
        [uniqueKey]: { x: 1_000_000, y: -1_000_000 },
      },
      viewport: { x: 0, y: 0, zoom: 8 },
    }),
    graph,
  );
  assert.deepEqual(parsed.errors, []);
  assert.equal(parsed.layout?.nodes[duplicateKey], undefined);
  assert.deepEqual(parsed.layout?.nodes[uniqueKey], {
    x: 1_000_000,
    y: -1_000_000,
  });
  assert(
    parseEnvironmentLayoutSidecar(
      '{"schemaVersion":1,"nodes":{"x":{"x":1000001,"y":0}}}',
    ).errors.some((error) => error.includes("bounded")),
  );
  assert(
    parseEnvironmentLayoutSidecar(
      '{"schemaVersion":1,"nodes":{},"surprise":true}',
    ).errors.some((error) => error.includes("unknown property")),
  );
  assert(
    parseEnvironmentLayoutSidecar(
      '{"schemaVersion":1,"nodes":{},"viewport":{"x":0,"y":0,"zoom":8.1}}',
    ).errors.some((error) => error.includes("zoom from")),
  );
});

test("rejects empty, unknown, and structurally invalid fragments", () => {
  const empty = parseTopologyFragment(
    '{"schemaVersion":1,"networks":[],"devices":[]}',
  );
  assert(empty.errors.some((error) => error.includes("at least one")));
  const invalid = parseTopologyFragment(
    JSON.stringify({
      schemaVersion: 1,
      networks: [{ id: "data", kind: "telepathy", extra: true }],
      devices: [
        {
          id: "ic",
          prefab: "Housing",
          fields: { Setting: "not-a-scalar" },
          ic: { program: "", pins: { d9: "x" }, registers: { r18: 0 } },
        },
      ],
    }),
  );
  assert(invalid.errors.some((error) => error.includes("unknown property")));
  assert(invalid.errors.some((error) => error.includes("supported kind")));
  assert(invalid.errors.some((error) => error.includes("special scalar")));
  assert(invalid.errors.some((error) => error.includes("d0 through d5")));
  assert.throws(
    () =>
      exportTopologyFragment(scenario(), {
        deviceIds: ["missing"],
        networkIds: ["missing"],
      }),
    /no known objects/,
  );
});

test("revalidates stale import previews and resolves program paths explicitly", () => {
  const source = scenario();
  const exported = exportTopologyFragment(source, {
    deviceIds: ["controller"],
  });
  const destination: EnvironmentScenario = {
    schemaVersion: 1,
    networks: [],
    devices: [],
  };
  const preview = previewFragmentImport(destination, exported.fragment, {
    origin: "file:///source",
    destination: "file:///destination",
    resolveProgramPath: ({ program, origin, destination }) =>
      `${destination}/${origin}/${program}`,
  });
  assert.equal(
    preview.fragment.devices[0]?.ic?.program,
    "file:///destination/file:///source/main.ic10",
  );
  assert.equal(
    preview.destinationFingerprint,
    topologyDestinationFingerprint(destination),
  );
  destination.devices.push({ id: "late", prefab: "Housing" });
  assert.throws(
    () => applyFragmentImport(destination, preview),
    /destination changed/,
  );
  const unresolved = previewFragmentImport(
    { schemaVersion: 1, networks: [], devices: [] },
    exported.fragment,
    { resolveProgramPath: () => undefined },
  );
  assert(unresolved.warnings.some((warning) => warning.includes("could not")));
});

test("includes pinned devices by default and reports deliberately dropped pins", () => {
  const included = exportTopologyFragment(scenario(), {
    deviceIds: ["controller"],
  });
  assert.deepEqual(
    included.fragment.devices.map(({ id }) => id),
    ["controller", "vendor"],
  );
  const dropped = exportTopologyFragment(scenario(), {
    deviceIds: ["controller"],
    includePinnedDevices: false,
  });
  assert.deepEqual(dropped.fragment.devices[0]?.ic?.pins, {});
  assert(dropped.warnings.some((warning) => warning.includes("pin d0")));
});

test("keeps default search bounded and invokes deep value search only on demand", () => {
  const input = scenario();
  input.devices[1]!.fields = {
    Secret: 123456789,
    Huge: Number("1"),
  };
  input.devices[1]!.name = "x".repeat(10_000);
  const graph = buildTopologyGraph(input, catalog);
  const vendor = graph.nodes.find(({ identity }) => identity.id === "vendor")!;
  assert(vendor.searchText.length <= 4096);
  assert.deepEqual(filterTopologyNodes(graph, { query: "123456789" }), []);
  let calls = 0;
  const deep = (node: (typeof graph.nodes)[number]): string => {
    calls += 1;
    return node.identity.id === "vendor" ? "123456789" : "";
  };
  assert.deepEqual(
    filterTopologyNodes(graph, { query: "123456789" }, deep),
    [],
  );
  assert.equal(calls, 0);
  assert.deepEqual(
    filterTopologyNodes(
      graph,
      { query: "123456789", deepValues: true },
      deep,
    ).map(({ identity }) => identity.id),
    ["vendor"],
  );
  assert.equal(calls, graph.nodes.length);
});

test("provides canonical reading order and accessible node, port, edge, and pin focus", () => {
  const graph = buildTopologyGraph(scenario(), catalog);
  const positions = Object.fromEntries(
    graph.nodes.map((node, index) => [
      node.key,
      { x: index % 2, y: Math.floor(index / 2) },
    ]),
  );
  positions.invalid = { x: Number.NaN, y: 0 };
  const order = topologyReadingOrder(positions);
  assert.equal(order.at(-1) === "invalid", false);
  assert.deepEqual(order.slice(0, 2), graph.nodes.slice(0, 2).map(({ key }) => key));
  const focus = buildTopologyFocusItems(graph, positions);
  assert(focus.some(({ kind, label }) => kind === "node" && label.length > 0));
  assert(
    focus.some(
      ({ kind, description }) =>
        kind === "port" && description.includes("port 0"),
    ),
  );
  assert(
    focus.some(
      ({ kind, label }) =>
        kind === "edge" && label.includes("controller to data"),
    ),
  );
  assert(
    focus.some(
      ({ kind, label }) =>
        kind === "pin" && label.includes("controller to vendor"),
    ),
  );
});

test("falls back deterministically when selections disappear or are filtered", () => {
  const graph = buildTopologyGraph(scenario());
  const vendorKey = graph.nodes.find(({ identity }) => identity.id === "vendor")!
    .key;
  const portFallback = reconcileTopologySelectionWithFallback(graph, {
    kind: "port",
    nodeKey: vendorKey,
    connectionKey: "",
  });
  assert.deepEqual(portFallback.selection, {
    kind: "node",
    nodeKey: vendorKey,
  });
  assert(portFallback.announcement?.includes("moved to"));
  const filtered = reconcileTopologySelectionWithFallback(
    graph,
    { kind: "node", nodeKey: vendorKey },
    [graph.nodes[0]!.key],
  );
  assert.deepEqual(filtered.selection, {
    kind: "node",
    nodeKey: graph.nodes[0]!.key,
  });
  assert(filtered.announcement?.includes(graph.nodes[0]!.label));
});
