import {
  automaticTopologyLayout,
  buildTopologyFocusItems,
  buildTopologyGraph,
  duplicateDevice,
  duplicateSubnetwork,
  filterTopologyNodes,
  normalizeEnvironmentLayout,
  type DeviceTopologyMetadata,
  type EnvironmentLayoutSidecar,
  type EnvironmentScenario,
  type Point,
  type TopologyCatalog,
  type TopologyDiagnostic,
  type TopologyFilter,
  type TopologySelection,
} from "./environmentTopologyModel.ts";

export interface EnvironmentTopologyViewModel {
  readonly nodes: readonly {
    key: string;
    kind: "network" | "device";
    id: string;
    label: string;
    secondaryLabel: string;
    prefab?: string;
    isIc: boolean;
    validationState: "valid" | "warning" | "error";
    x: number;
    y: number;
    ports: readonly {
      connectionKey: string;
      label: string;
    }[];
  }[];
  readonly edges: readonly {
    key: string;
    kind: "connection" | "pin";
    sourceKey: string;
    targetKey?: string;
    label: string;
    networkKind?: string;
    cableRole?: string;
    direction?: "toNetwork" | "toDevice";
    validationState: "valid" | "warning" | "error";
  }[];
  readonly focusItems: ReturnType<typeof buildTopologyFocusItems>;
  readonly visibleNodeKeys: readonly string[];
  readonly positions: Readonly<Record<string, Point>>;
  readonly viewport?: { x: number; y: number; zoom: number };
}

export function topologyLayoutFilename(scenarioFilename: string): string {
  return scenarioFilename.endsWith(".ic10sim.json")
    ? `${scenarioFilename.slice(0, -".ic10sim.json".length)}.ic10sim.layout.json`
    : `${scenarioFilename}.layout.json`;
}

export function buildEnvironmentTopologyView(
  scenario: EnvironmentScenario,
  catalog: Readonly<Record<string, DeviceTopologyMetadata | undefined>>,
  layout: EnvironmentLayoutSidecar | undefined = undefined,
  diagnostics: readonly TopologyDiagnostic[] = [],
  filter: TopologyFilter = {},
): EnvironmentTopologyViewModel {
  const graph = buildTopologyGraph(
    scenario,
    catalog as TopologyCatalog,
    [...environmentTopologyDiagnostics(scenario, catalog), ...diagnostics],
  );
  const positions = automaticTopologyLayout(graph, layout);
  const visibleNodeKeys = filterTopologyNodes(graph, filter).map(
    ({ key }) => key,
  );
  const networks = new Map(
    graph.nodes
      .filter(({ identity }) => identity.kind === "network")
      .map((node) => [node.key, node]),
  );
  return {
    nodes: graph.nodes.map((node) => ({
      key: node.key,
      kind: node.identity.kind,
      id: node.identity.id,
      label: node.label,
      secondaryLabel: node.secondaryLabel,
      ...(node.prefab ? { prefab: node.prefab } : {}),
      isIc: node.isIc,
      validationState: node.validationState,
      x: positions[node.key]?.x ?? 0,
      y: positions[node.key]?.y ?? 0,
      ports: node.ports.map(({ connectionKey, label }) => ({
        connectionKey,
        label,
      })),
    })),
    edges: graph.edges.map((edge) => {
      if (edge.kind === "pin") {
        return {
          key: edge.key,
          kind: edge.kind,
          sourceKey: edge.icKey,
          targetKey: edge.targetKey,
          label: `pin ${edge.pin}`,
          validationState: edge.validationState,
        };
      }
      const network = edge.networkKey
        ? networks.get(edge.networkKey)
        : undefined;
      return {
        key: edge.key,
        kind: edge.kind,
        sourceKey: edge.deviceKey,
        targetKey: edge.networkKey,
        label: [
          `connection ${edge.connectionKey}`,
          edge.connectionType,
          edge.connectionRole,
        ]
          .filter(Boolean)
          .join(" · "),
        networkKind: network?.networkKind,
        cableRole: network?.cableRole,
        ...(edge.connectionRole?.toLocaleLowerCase().includes("output")
          ? { direction: "toNetwork" as const }
          : edge.connectionRole?.toLocaleLowerCase().includes("input")
            ? { direction: "toDevice" as const }
            : {}),
        validationState: edge.validationState,
      };
    }),
    focusItems: buildTopologyFocusItems(graph, positions),
    visibleNodeKeys,
    positions,
    ...(layout?.viewport ? { viewport: { ...layout.viewport } } : {}),
  };
}

export function environmentTopologyDiagnostics(
  scenario: EnvironmentScenario,
  catalog: Readonly<Record<string, DeviceTopologyMetadata | undefined>>,
): readonly TopologyDiagnostic[] {
  const result: TopologyDiagnostic[] = [];
  const networkCounts = counts(scenario.networks.map(({ id }) => id));
  const deviceCounts = counts(scenario.devices.map(({ id }) => id));
  scenario.networks.forEach((network, networkIndex) => {
    if ((networkCounts.get(network.id) ?? 0) > 1) {
      result.push({
        severity: "error",
        message: `Duplicate network ID ${network.id}.`,
        target: { entity: "network", networkIndex },
      });
    }
    const attached = scenario.devices.some((device) =>
      Object.values(device.connections ?? {}).includes(network.id),
    );
    if (!attached) {
      result.push({
        severity: "warning",
        message: `Network ${network.id} is disconnected.`,
        target: { entity: "network", networkIndex },
      });
    }
  });
  scenario.devices.forEach((device, deviceIndex) => {
    if ((deviceCounts.get(device.id) ?? 0) > 1) {
      result.push({
        severity: "error",
        message: `Duplicate device ID ${device.id}.`,
        target: { entity: "device", deviceIndex },
      });
    }
    const connections = Object.entries(device.connections ?? {});
    if (connections.length === 0) {
      result.push({
        severity: "warning",
        message: `Device ${device.id} has no network connections.`,
        target: { entity: "device", deviceIndex },
      });
    }
    const metadata = catalog[device.prefab];
    for (const [connectionKey, networkId] of connections) {
      const network = scenario.networks.find(({ id }) => id === networkId);
      if (!network) {
        continue;
      }
      const connection = /^\d+$/.test(connectionKey)
        ? Number(connectionKey)
        : undefined;
      const type =
        connection === undefined
          ? undefined
          : metadata?.connections?.[connection]?.type;
      if (
        typeof type === "string" &&
        !connectionMatchesNetwork(type, network.kind, network.cableRole)
      ) {
        result.push({
          severity: "error",
          message: `${device.id} connection ${connectionKey} is incompatible with ${network.id}.`,
          target: { deviceIndex, connectionKey },
        });
      }
    }
  });
  return result;
}

export function savedTopologyLayout(
  scenario: EnvironmentScenario,
  catalog: TopologyCatalog,
  positions: Readonly<Record<string, Point>>,
  viewport?: { x: number; y: number; zoom: number },
): EnvironmentLayoutSidecar {
  return normalizeEnvironmentLayout(
    {
      schemaVersion: 1,
      nodes: Object.fromEntries(
        Object.entries(positions).map(([key, point]) => [key, { ...point }]),
      ),
      ...(viewport ? { viewport: { ...viewport } } : {}),
    },
    buildTopologyGraph(scenario, catalog),
  );
}

export function inspectorSelectionForTopology(
  scenario: EnvironmentScenario,
  selection: TopologySelection,
): { type: "network" | "device"; index: number } | undefined {
  const graph = buildTopologyGraph(scenario);
  const nodeKey =
    selection.kind === "node" || selection.kind === "port"
      ? selection.nodeKey
      : (() => {
          const edge = graph.edgeByKey.get(selection.edgeKey);
          return edge?.kind === "connection"
            ? edge.deviceKey
            : edge?.kind === "pin"
              ? edge.icKey
              : undefined;
        })();
  const node = nodeKey ? graph.nodeByKey.get(nodeKey) : undefined;
  return node
    ? { type: node.identity.kind, index: node.sourceIndex }
    : undefined;
}

export function duplicateTopologySelection(
  scenario: EnvironmentScenario,
  selection: { kind: "device"; id: string } | { kind: "network"; id: string },
): EnvironmentScenario {
  return selection.kind === "device"
    ? duplicateDevice(scenario, selection.id).scenario
    : duplicateSubnetwork(scenario, {
        networkIds: [selection.id],
        includeConnectedDevices: true,
      }).scenario;
}

function counts(values: readonly string[]): ReadonlyMap<string, number> {
  const result = new Map<string, number>();
  for (const value of values) {
    result.set(value, (result.get(value) ?? 0) + 1);
  }
  return result;
}

function connectionMatchesNetwork(
  connectionType: string,
  networkKind: string,
  cableRole: string | undefined,
): boolean {
  const type = connectionType.toLocaleLowerCase();
  if (networkKind === "cable") {
    if (type.includes("power") && type.includes("data")) {
      return cableRole === "powerAndData";
    }
    if (type.includes("power")) {
      return cableRole === "power" || cableRole === "powerAndData";
    }
    if (type.includes("data")) {
      return cableRole === "data" || cableRole === "powerAndData";
    }
    return false;
  }
  if (networkKind === "chute") {
    return type.includes("chute");
  }
  if (networkKind === "liquid") {
    return type.includes("liquid");
  }
  return networkKind === "gas"
    ? type.includes("gas") || type === "pipe"
    : false;
}
