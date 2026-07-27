export type Scalar = number | "NaN" | "Infinity" | "-Infinity" | "-0";

export interface EnvironmentNetwork {
  id: string;
  kind: string;
  cableRole?: string;
  channels?: Record<string, Scalar>;
}

export interface EnvironmentIc {
  program: string;
  enabled?: boolean;
  pins?: Record<string, string>;
  registers?: Record<string, Scalar>;
  stack?: Record<string, Scalar>;
}

export interface EnvironmentDevice {
  id: string;
  prefab: string;
  name?: string;
  referenceId?: number;
  connections?: Record<string, string>;
  fields?: Record<string, Scalar>;
  slots?: Record<string, Record<string, Scalar>>;
  memory?: Record<string, Scalar>;
  ic?: EnvironmentIc;
}

export interface EnvironmentScenario {
  schemaVersion: number;
  gameVersion?: string;
  networks: EnvironmentNetwork[];
  devices: EnvironmentDevice[];
}

export type TopologyNodeKind = "network" | "device";
export type ValidationState = "valid" | "warning" | "error";

export interface TopologyNodeIdentity {
  kind: TopologyNodeKind;
  id: string;
  occurrence: number;
}

export interface TopologyNode {
  key: string;
  identity: TopologyNodeIdentity;
  label: string;
  secondaryLabel: string;
  networkKind?: string;
  cableRole?: string;
  prefab?: string;
  isIc: boolean;
  ports: readonly TopologyPort[];
  searchText: string;
  validationState: ValidationState;
  sourceIndex: number;
}

export interface TopologyPort {
  key: string;
  connectionKey: string;
  connection?: number;
  label: string;
  connectionType?: string;
  connectionRole?: string;
}

export interface TopologyConnectionEdge {
  kind: "connection";
  key: string;
  deviceKey: string;
  networkKey?: string;
  deviceId: string;
  networkId: string;
  connectionKey: string;
  connection?: number;
  connectionType?: string;
  connectionRole?: string;
  validationState: ValidationState;
}

export interface TopologyPinEdge {
  kind: "pin";
  key: string;
  icKey: string;
  targetKey?: string;
  icId: string;
  targetId: string;
  pin: string;
  validationState: ValidationState;
}

export type TopologyEdge = TopologyConnectionEdge | TopologyPinEdge;

export interface DeviceConnectionMetadata {
  type?: unknown;
  role?: unknown;
}

export interface DeviceTopologyMetadata {
  displayName?: string;
  connections?: readonly DeviceConnectionMetadata[];
}

export type TopologyCatalog = Readonly<
  Record<string, DeviceTopologyMetadata | undefined>
>;

export interface TopologyDiagnostic {
  severity: "warning" | "error";
  message: string;
  target?: ValidationTarget;
}

export interface TopologyGraph {
  nodes: readonly TopologyNode[];
  edges: readonly TopologyEdge[];
  nodeByKey: ReadonlyMap<string, TopologyNode>;
  edgeByKey: ReadonlyMap<string, TopologyEdge>;
}

export type TopologySelection =
  | { kind: "node"; nodeKey: string }
  | { kind: "port"; nodeKey: string; connectionKey: string }
  | { kind: "edge"; edgeKey: string }
  | { kind: "pin"; edgeKey: string };

export interface ValidationTarget {
  entity?: "scenario" | "network" | "device" | "connection" | "pin";
  networkId?: string;
  networkIndex?: number;
  deviceId?: string;
  deviceIndex?: number;
  icId?: string;
  connection?: number;
  connectionIndex?: number;
  connectionKey?: string;
  pin?: string;
  property?: string;
}

export interface Point {
  x: number;
  y: number;
}

export interface TopologyViewport {
  x: number;
  y: number;
  zoom: number;
}

export interface EnvironmentLayoutSidecar {
  schemaVersion: 1;
  nodes: Record<string, Point>;
  viewport?: TopologyViewport;
}

export interface LayoutParseResult {
  layout?: EnvironmentLayoutSidecar;
  errors: readonly string[];
}

export interface DuplicateResult {
  scenario: EnvironmentScenario;
  selected: TopologyNodeIdentity;
  networkIdRemap: Readonly<Record<string, string>>;
  deviceIdRemap: Readonly<Record<string, string>>;
  warnings: readonly string[];
}

export interface DuplicateSubnetworkOptions {
  networkIds: readonly string[];
  deviceIds?: readonly string[];
  includeConnectedDevices?: boolean;
  preserveExternalConnections?: boolean;
}

export interface TopologyFragment {
  schemaVersion: 1;
  networks: EnvironmentNetwork[];
  devices: EnvironmentDevice[];
  layout?: EnvironmentLayoutSidecar;
}

export interface FragmentExportOptions {
  networkIds?: readonly string[];
  deviceIds?: readonly string[];
  includeReferencedNetworks?: boolean;
  includePinnedDevices?: boolean;
  layout?: EnvironmentLayoutSidecar;
}

export interface FragmentExportResult {
  fragment: TopologyFragment;
  warnings: readonly string[];
}

export interface FragmentProgramPathContext {
  program: string;
  sourceDeviceId: string;
  importedDeviceId: string;
  origin?: string;
  destination?: string;
}

export interface FragmentImportOptions {
  origin?: string;
  destination?: string;
  resolveProgramPath?: (
    context: FragmentProgramPathContext,
  ) => string | undefined;
}

export interface FragmentImportPreview {
  fragment: TopologyFragment;
  networkIdRemap: Readonly<Record<string, string>>;
  deviceIdRemap: Readonly<Record<string, string>>;
  destinationFingerprint: string;
  warnings: readonly string[];
}

export interface FragmentParseResult {
  fragment?: TopologyFragment;
  errors: readonly string[];
}

export interface TopologyFilter {
  query?: string;
  networkKinds?: readonly string[];
  prefabs?: readonly string[];
  icOnly?: boolean;
  validationStates?: readonly ValidationState[];
  deepValues?: boolean;
}

export interface TopologyFocusItem {
  key: string;
  kind: "node" | "port" | "edge" | "pin";
  selection: TopologySelection;
  nodeKey?: string;
  point: Point;
  label: string;
  description: string;
  validationState: ValidationState;
}

export interface SelectionReconciliation {
  selection?: TopologySelection;
  announcement?: string;
}

export type RovingDirection = "next" | "previous" | "home" | "end";
export type SpatialDirection = "left" | "right" | "up" | "down";

const KEY_SEPARATOR = ":";
export const TOPOLOGY_COORDINATE_LIMIT = 1_000_000;
export const TOPOLOGY_ZOOM_MIN = 0.1;
export const TOPOLOGY_ZOOM_MAX = 8;
export const TOPOLOGY_SEARCH_TEXT_LIMIT = 4096;

export function topologyNodeKey(identity: TopologyNodeIdentity): string {
  return [
    identity.kind,
    encodeURIComponent(identity.id),
    String(identity.occurrence),
  ].join(KEY_SEPARATOR);
}

export function topologySelectionKey(selection: TopologySelection): string {
  switch (selection.kind) {
    case "node":
      return `node:${selection.nodeKey}`;
    case "port":
      return `port:${selection.nodeKey}:${encodeURIComponent(selection.connectionKey)}`;
    case "edge":
      return `edge:${selection.edgeKey}`;
    case "pin":
      return `pin:${selection.edgeKey}`;
  }
}

export function buildTopologyGraph(
  scenario: EnvironmentScenario,
  catalog: TopologyCatalog = {},
  diagnostics: readonly TopologyDiagnostic[] = [],
): TopologyGraph {
  const networkOccurrences = occurrences(scenario.networks.map(({ id }) => id));
  const deviceOccurrences = occurrences(scenario.devices.map(({ id }) => id));
  const nodes: TopologyNode[] = [];

  scenario.networks.forEach((network, sourceIndex) => {
    const identity: TopologyNodeIdentity = {
      kind: "network",
      id: network.id,
      occurrence: networkOccurrences[sourceIndex] ?? 0,
    };
    nodes.push({
      key: topologyNodeKey(identity),
      identity,
      label: network.id,
      secondaryLabel:
        network.kind === "cable" && network.cableRole
          ? `${network.kind} · ${network.cableRole}`
          : network.kind,
      networkKind: network.kind,
      cableRole: network.cableRole,
      isIc: false,
      ports: [],
      searchText: searchableNetwork(network),
      validationState: "valid",
      sourceIndex,
    });
  });

  scenario.devices.forEach((device, sourceIndex) => {
    const identity: TopologyNodeIdentity = {
      kind: "device",
      id: device.id,
      occurrence: deviceOccurrences[sourceIndex] ?? 0,
    };
    const metadata = catalog[device.prefab];
    nodes.push({
      key: topologyNodeKey(identity),
      identity,
      label: device.name?.trim() || device.id,
      secondaryLabel: metadata?.displayName ?? device.prefab,
      prefab: device.prefab,
      isIc: device.ic !== undefined,
      ports: devicePorts(device, metadata),
      searchText: searchableDevice(device, metadata),
      validationState: "valid",
      sourceIndex,
    });
  });

  const nodeByKey = new Map(nodes.map((node) => [node.key, node]));
  const uniqueNetwork = uniqueNodeKeys(nodes, "network");
  const uniqueDevice = uniqueNodeKeys(nodes, "device");
  const edges: TopologyEdge[] = [];

  for (const deviceNode of nodes.filter(
    (node): node is TopologyNode => node.identity.kind === "device",
  )) {
    const device = scenario.devices[deviceNode.sourceIndex];
    if (!device) {
      continue;
    }
    const metadata = catalog[device.prefab];
    for (const [rawConnection, networkId] of sortedEntries(
      device.connections ?? {},
    )) {
      const connection = /^\d+$/.test(rawConnection)
        ? Number(rawConnection)
        : undefined;
      const networkKey = uniqueNetwork.get(networkId);
      const edge: TopologyConnectionEdge = {
        kind: "connection",
        key: connectionEdgeKey(deviceNode.key, rawConnection),
        deviceKey: deviceNode.key,
        networkKey,
        deviceId: device.id,
        networkId,
        connectionKey: rawConnection,
        ...(connection !== undefined ? { connection } : {}),
        connectionType: displayMetadata(
          connection === undefined
            ? undefined
            : metadata?.connections?.[connection]?.type,
        ),
        connectionRole: displayMetadata(
          connection === undefined
            ? undefined
            : metadata?.connections?.[connection]?.role,
        ),
        validationState:
          connection !== undefined && networkKey ? "valid" : "error",
      };
      edges.push(edge);
    }
    for (const [pin, targetId] of sortedEntries(device.ic?.pins ?? {})) {
      const targetKey = uniqueDevice.get(targetId);
      edges.push({
        kind: "pin",
        key: pinEdgeKey(deviceNode.key, pin),
        icKey: deviceNode.key,
        targetKey,
        icId: device.id,
        targetId,
        pin,
        validationState: targetKey ? "valid" : "error",
      });
    }
  }

  const edgeByKey = new Map(edges.map((edge) => [edge.key, edge]));
  const graph: TopologyGraph = { nodes, edges, nodeByKey, edgeByKey };
  for (const diagnostic of diagnostics) {
    const selection = diagnostic.target
      ? mapValidationTarget(graph, diagnostic.target)
      : undefined;
    if (!selection) {
      continue;
    }
    const state = diagnostic.severity;
    if (selection.kind === "node" || selection.kind === "port") {
      promoteValidation(nodeByKey.get(selection.nodeKey), state);
    } else {
      const edge = edgeByKey.get(selection.edgeKey);
      promoteValidation(edge, state);
      if (edge?.kind === "connection") {
        promoteValidation(nodeByKey.get(edge.deviceKey), state);
        promoteValidation(
          edge.networkKey ? nodeByKey.get(edge.networkKey) : undefined,
          state,
        );
      } else if (edge?.kind === "pin") {
        promoteValidation(nodeByKey.get(edge.icKey), state);
        promoteValidation(
          edge.targetKey ? nodeByKey.get(edge.targetKey) : undefined,
          state,
        );
      }
    }
  }
  return graph;
}

export function automaticTopologyLayout(
  graph: TopologyGraph,
  saved?: EnvironmentLayoutSidecar,
): Record<string, Point> {
  const persistable = persistableNodeKeys(graph);
  const adjacency = new Map<string, Set<string>>();
  for (const node of graph.nodes) {
    adjacency.set(node.key, new Set());
  }
  for (const edge of graph.edges) {
    const pair =
      edge.kind === "connection"
        ? [edge.deviceKey, edge.networkKey]
        : [edge.icKey, edge.targetKey];
    if (!pair[1]) {
      continue;
    }
    adjacency.get(pair[0]!)?.add(pair[1]);
    adjacency.get(pair[1])?.add(pair[0]!);
  }

  const remaining = new Set(graph.nodes.map(({ key }) => key));
  const components: string[][] = [];
  while (remaining.size > 0) {
    const start = [...remaining].sort()[0]!;
    const queue = [start];
    const component: string[] = [];
    remaining.delete(start);
    while (queue.length > 0) {
      const key = queue.shift()!;
      component.push(key);
      for (const neighbour of [...(adjacency.get(key) ?? [])].sort()) {
        if (remaining.delete(neighbour)) {
          queue.push(neighbour);
        }
      }
    }
    components.push(component.sort());
  }
  components.sort((left, right) => left[0]!.localeCompare(right[0]!));

  const result: Record<string, Point> = {};
  let componentX = 0;
  for (const component of components) {
    const root =
      component.find(
        (key) => graph.nodeByKey.get(key)?.identity.kind === "network",
      ) ?? component[0]!;
    const distances = breadthFirstDistances(root, component, adjacency);
    const layers = new Map<number, string[]>();
    for (const key of component) {
      const distance = distances.get(key) ?? 0;
      const values = layers.get(distance) ?? [];
      values.push(key);
      layers.set(distance, values);
    }
    let componentWidth = 0;
    const sortedLayers = [...layers.entries()].sort(([left], [right]) => left - right);
    const nodeRows = new Map<string, number>();
    for (const [layer, keys] of sortedLayers) {
      if (layer === 0) {
        keys.sort((left, right) => nodeSort(graph, left, right));
      } else {
        keys.sort((left, right) => {
          const leftNeighbours = [...(adjacency.get(left) ?? [])].filter((n) => nodeRows.has(n));
          const rightNeighbours = [...(adjacency.get(right) ?? [])].filter((n) => nodeRows.has(n));
          const leftBary = leftNeighbours.length > 0
            ? leftNeighbours.reduce((sum, n) => sum + nodeRows.get(n)!, 0) / leftNeighbours.length
            : Infinity;
          const rightBary = rightNeighbours.length > 0
            ? rightNeighbours.reduce((sum, n) => sum + nodeRows.get(n)!, 0) / rightNeighbours.length
            : Infinity;
          if (leftBary !== rightBary) return leftBary - rightBary;
          return nodeSort(graph, left, right);
        });
      }
      componentWidth = Math.max(componentWidth, layer * 460 + 240);
      keys.forEach((key, row) => {
        nodeRows.set(key, row);
        result[key] = {
          x: componentX + layer * 460,
          y: row * 270,
        };
      });
    }
    componentX += componentWidth + 180;
  }

  for (const [key, point] of Object.entries(saved?.nodes ?? {})) {
    if (persistable.has(key) && boundedPoint(point)) {
      result[key] = { x: point.x, y: point.y };
    }
  }
  return result;
}

export function parseEnvironmentLayoutSidecar(
  source: string,
  graph?: TopologyGraph,
): LayoutParseResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch (error) {
    return { errors: [`Invalid layout JSON: ${String(error)}`] };
  }
  if (!isRecord(parsed)) {
    return { errors: ["Layout must be a JSON object."] };
  }
  const errors: string[] = [];
  rejectUnknownProperties(parsed, ["schemaVersion", "nodes", "viewport"], "layout", errors);
  if (parsed.schemaVersion !== 1) {
    errors.push("layout schemaVersion must be 1.");
  }
  if (!isRecord(parsed.nodes)) {
    errors.push("layout nodes must be an object.");
  }
  const nodes: Record<string, Point> = {};
  const persistable = graph ? persistableNodeKeys(graph) : undefined;
  if (isRecord(parsed.nodes)) {
    for (const [key, value] of Object.entries(parsed.nodes)) {
      if (!isRecord(value)) {
        errors.push(`layout node ${JSON.stringify(key)} needs bounded x and y.`);
      } else {
        rejectUnknownProperties(value, ["x", "y"], `layout node ${JSON.stringify(key)}`, errors);
      }
      if (!isRecord(value) || !boundedNumber(value.x) || !boundedNumber(value.y)) {
        errors.push(`layout node ${JSON.stringify(key)} needs bounded x and y.`);
      } else if (!persistable || persistable.has(key)) {
        nodes[key] = { x: value.x, y: value.y };
      }
    }
  }
  let viewport: TopologyViewport | undefined;
  if (parsed.viewport !== undefined) {
    if (
      !isRecord(parsed.viewport) ||
      !boundedNumber(parsed.viewport.x) ||
      !boundedNumber(parsed.viewport.y) ||
      !finiteNumber(parsed.viewport.zoom) ||
      parsed.viewport.zoom < TOPOLOGY_ZOOM_MIN ||
      parsed.viewport.zoom > TOPOLOGY_ZOOM_MAX
    ) {
      errors.push(
        `layout viewport needs bounded x/y and zoom from ${TOPOLOGY_ZOOM_MIN} to ${TOPOLOGY_ZOOM_MAX}.`,
      );
    } else {
      rejectUnknownProperties(
        parsed.viewport,
        ["x", "y", "zoom"],
        "layout viewport",
        errors,
      );
      viewport = {
        x: parsed.viewport.x,
        y: parsed.viewport.y,
        zoom: parsed.viewport.zoom,
      };
    }
  }
  if (errors.length > 0) {
    return { errors };
  }
  return {
    errors: [],
    layout: {
      schemaVersion: 1,
      nodes,
      ...(viewport ? { viewport } : {}),
    },
  };
}

export function normalizeEnvironmentLayout(
  value: EnvironmentLayoutSidecar,
  graph: TopologyGraph,
): EnvironmentLayoutSidecar {
  const persistable = persistableNodeKeys(graph);
  const nodes = Object.fromEntries(
    Object.entries(value.nodes)
      .filter(([key, point]) => persistable.has(key) && boundedPoint(point))
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, point]) => [key, { x: point.x, y: point.y }]),
  );
  return {
    schemaVersion: 1,
    nodes,
    ...(value.viewport &&
    boundedPoint(value.viewport) &&
    finiteNumber(value.viewport.zoom) &&
    value.viewport.zoom >= TOPOLOGY_ZOOM_MIN &&
    value.viewport.zoom <= TOPOLOGY_ZOOM_MAX
      ? { viewport: { ...value.viewport } }
      : {}),
  };
}

export function reconcileTopologySelection(
  graph: TopologyGraph,
  selection: TopologySelection | undefined,
): TopologySelection | undefined {
  if (!selection) {
    return undefined;
  }
  if (selection.kind === "node") {
    return graph.nodeByKey.has(selection.nodeKey) ? selection : undefined;
  }
  if (selection.kind === "port") {
    return graph.nodeByKey.has(selection.nodeKey) &&
      selection.connectionKey.length > 0
      ? selection
      : undefined;
  }
  return graph.edgeByKey.has(selection.edgeKey) ? selection : undefined;
}

export function mapValidationTarget(
  graph: TopologyGraph,
  target: ValidationTarget,
): TopologySelection | undefined {
  const deviceNode = findTargetNode(
    graph,
    "device",
    target.deviceId ?? target.icId,
    target.deviceIndex,
  );
  const networkNode = findTargetNode(
    graph,
    "network",
    target.networkId,
    target.networkIndex,
  );
  const connectionKey =
    target.connectionKey ??
    numericConnectionKey(target.connection) ??
    numericConnectionKey(target.connectionIndex) ??
    propertyName(target.property, "connections");
  if (deviceNode && connectionKey !== undefined) {
    const edge = graph.edges.find(
      (candidate) =>
        candidate.kind === "connection" &&
        candidate.deviceKey === deviceNode.key &&
        candidate.connectionKey === connectionKey,
    );
    return edge
      ? { kind: "edge", edgeKey: edge.key }
      : { kind: "port", nodeKey: deviceNode.key, connectionKey };
  }
  const pin = target.pin ?? propertyName(target.property, "pins");
  if (deviceNode && pin) {
    const edge = graph.edges.find(
      (candidate) =>
        candidate.kind === "pin" &&
        candidate.icKey === deviceNode.key &&
        candidate.pin === pin,
    );
    return edge ? { kind: "pin", edgeKey: edge.key } : undefined;
  }
  const node =
    target.entity === "network"
      ? networkNode
      : target.entity === "device" || target.entity === "pin"
        ? deviceNode
        : deviceNode ?? networkNode;
  return node ? { kind: "node", nodeKey: node.key } : undefined;
}

export function duplicateDevice(
  scenario: EnvironmentScenario,
  deviceId: string,
): DuplicateResult {
  const source = scenario.devices.find((device) => device.id === deviceId);
  if (!source) {
    throw new Error(`Cannot duplicate unknown device ${JSON.stringify(deviceId)}.`);
  }
  const result = clone(scenario);
  const duplicate = clone(source);
  const nextId = uniqueCopyId(
    source.id,
    new Set(result.devices.map(({ id }) => id)),
  );
  duplicate.id = nextId;
  duplicate.name = source.name?.trim()
    ? uniqueCopyName(
        source.name,
        new Set(result.devices.map(({ name }) => name ?? "")),
      )
    : nextId;
  delete duplicate.referenceId;
  result.devices.push(duplicate);
  return {
    scenario: result,
    selected: { kind: "device", id: nextId, occurrence: 0 },
    networkIdRemap: Object.fromEntries([]),
    deviceIdRemap: Object.fromEntries([[source.id, nextId]]),
    warnings: source.referenceId === undefined
      ? []
      : ["The duplicate receives an automatically assigned ReferenceId."],
  };
}

export function duplicateSubnetwork(
  scenario: EnvironmentScenario,
  options: DuplicateSubnetworkOptions,
): DuplicateResult {
  const selectedNetworks = new Set(options.networkIds);
  const selectedDevices = new Set(options.deviceIds ?? []);
  if (options.includeConnectedDevices !== false) {
    for (const device of scenario.devices) {
      if (
        Object.values(device.connections ?? {}).some((network) =>
          selectedNetworks.has(network),
        )
      ) {
        selectedDevices.add(device.id);
      }
    }
  }
  const result = clone(scenario);
  const warnings: string[] = [];
  const networkIdRemap = new Map<string, string>();
  const deviceIdRemap = new Map<string, string>();
  const usedNetworkIds = new Set(result.networks.map(({ id }) => id));
  const usedDeviceIds = new Set(result.devices.map(({ id }) => id));

  for (const network of scenario.networks.filter(({ id }) =>
    selectedNetworks.has(id),
  )) {
    const id = uniqueCopyId(network.id, usedNetworkIds);
    usedNetworkIds.add(id);
    networkIdRemap.set(network.id, id);
    result.networks.push({ ...clone(network), id });
  }
  for (const device of scenario.devices.filter(({ id }) =>
    selectedDevices.has(id),
  )) {
    const id = uniqueCopyId(device.id, usedDeviceIds);
    usedDeviceIds.add(id);
    deviceIdRemap.set(device.id, id);
  }
  for (const device of scenario.devices.filter(({ id }) =>
    selectedDevices.has(id),
  )) {
    const copy = clone(device);
    copy.id = deviceIdRemap.get(device.id)!;
    copy.name = device.name?.trim()
      ? uniqueCopyName(
          device.name,
          new Set(result.devices.map(({ name }) => name ?? "")),
        )
      : copy.id;
    delete copy.referenceId;
    copy.connections = remapConnections(
      device.connections,
      networkIdRemap,
      new Set(scenario.networks.map(({ id }) => id)),
      options.preserveExternalConnections === true,
      warnings,
      device.id,
    );
    if (copy.ic?.pins) {
      copy.ic.pins = remapPins(
        copy.ic.pins,
        deviceIdRemap,
        new Set(scenario.devices.map(({ id }) => id)),
        options.preserveExternalConnections === true,
        warnings,
        device.id,
      );
    }
    result.devices.push(copy);
  }
  const firstDevice = deviceIdRemap.values().next().value as string | undefined;
  const firstNetwork = networkIdRemap.values().next().value as string | undefined;
  if (!firstDevice && !firstNetwork) {
    throw new Error("The selected subnetwork contains no known objects.");
  }
  return {
    scenario: result,
    selected: firstDevice
      ? { kind: "device", id: firstDevice, occurrence: 0 }
      : { kind: "network", id: firstNetwork!, occurrence: 0 },
    networkIdRemap: Object.fromEntries(networkIdRemap),
    deviceIdRemap: Object.fromEntries(deviceIdRemap),
    warnings,
  };
}

export function exportTopologyFragment(
  scenario: EnvironmentScenario,
  options: FragmentExportOptions,
): FragmentExportResult {
  const deviceIds = new Set(options.deviceIds ?? []);
  const networkIds = new Set(options.networkIds ?? []);
  const warnings: string[] = [];
  if (options.includePinnedDevices !== false) {
    let changed = true;
    while (changed) {
      changed = false;
      for (const device of scenario.devices) {
        if (!deviceIds.has(device.id)) {
          continue;
        }
        for (const target of Object.values(device.ic?.pins ?? {})) {
          if (
            scenario.devices.some(({ id }) => id === target) &&
            !deviceIds.has(target)
          ) {
            deviceIds.add(target);
            changed = true;
          }
        }
      }
    }
  }
  const devices = scenario.devices
    .filter(({ id }) => deviceIds.has(id))
    .map(clone);
  if (options.includeReferencedNetworks !== false) {
    for (const device of devices) {
      for (const network of Object.values(device.connections ?? {})) {
        networkIds.add(network);
      }
    }
  }
  const networks = scenario.networks
    .filter(({ id }) => networkIds.has(id))
    .map(clone);
  const includedDeviceIds = new Set(devices.map(({ id }) => id));
  for (const device of devices) {
    if (device.ic?.pins) {
      device.ic.pins = Object.fromEntries(
        sortedEntries(device.ic.pins).filter(([pin, target]) => {
          const included = includedDeviceIds.has(target);
          if (!included) {
            warnings.push(
              `IC “${device.id}” pin ${pin} was omitted because target “${target}” was not included.`,
            );
          }
          return included;
        }),
      );
    }
  }
  const graph = buildTopologyGraph({
    schemaVersion: 1,
    networks,
    devices,
  });
  const layout = options.layout
    ? normalizeEnvironmentLayout(options.layout, graph)
    : undefined;
  if (networks.length === 0 && devices.length === 0) {
    throw new Error("The selected topology fragment contains no known objects.");
  }
  return {
    warnings,
    fragment: {
      schemaVersion: 1,
      networks,
      devices,
      ...(layout ? { layout } : {}),
    },
  };
}

export function previewFragmentImport(
  scenario: EnvironmentScenario,
  fragment: TopologyFragment,
  options: FragmentImportOptions = {},
): FragmentImportPreview {
  if (fragment.schemaVersion !== 1) {
    throw new Error("Unsupported topology fragment schema version.");
  }
  if (fragment.networks.length === 0 && fragment.devices.length === 0) {
    throw new Error("Cannot import an empty topology fragment.");
  }
  const duplicateNetworkIds = duplicateStrings(
    fragment.networks.map(({ id }) => id),
  );
  const duplicateDeviceIds = duplicateStrings(
    fragment.devices.map(({ id }) => id),
  );
  if (duplicateNetworkIds.length > 0 || duplicateDeviceIds.length > 0) {
    throw new Error(
      [
        ...duplicateNetworkIds.map((id) => `duplicate network ID “${id}”`),
        ...duplicateDeviceIds.map((id) => `duplicate device ID “${id}”`),
      ].join("; "),
    );
  }
  const warnings: string[] = [];
  const usedNetworkIds = new Set(scenario.networks.map(({ id }) => id));
  const usedDeviceIds = new Set(scenario.devices.map(({ id }) => id));
  const networkIdRemap = new Map<string, string>();
  const deviceIdRemap = new Map<string, string>();

  for (const network of fragment.networks) {
    const id = uniqueImportId(network.id, usedNetworkIds);
    usedNetworkIds.add(id);
    networkIdRemap.set(network.id, id);
    if (id !== network.id) {
      warnings.push(`Network “${network.id}” will be imported as “${id}”.`);
    }
  }
  for (const device of fragment.devices) {
    const id = uniqueImportId(device.id, usedDeviceIds);
    usedDeviceIds.add(id);
    deviceIdRemap.set(device.id, id);
    if (id !== device.id) {
      warnings.push(`Device “${device.id}” will be imported as “${id}”.`);
    }
  }

  const knownDestinationNetworks = new Set(
    scenario.networks.map(({ id }) => id),
  );
  const knownDestinationDevices = new Set(scenario.devices.map(({ id }) => id));
  const imported = clone(fragment);
  imported.networks = imported.networks.map((network) => ({
    ...network,
    id: networkIdRemap.get(network.id)!,
  }));
  const usedReferences = new Set(
    scenario.devices
      .map(({ referenceId }) => referenceId)
      .filter((value): value is number => value !== undefined),
  );
  imported.devices = imported.devices.map((device) => {
    const originalId = device.id;
    device.id = deviceIdRemap.get(originalId)!;
    device.connections = remapConnections(
      device.connections,
      networkIdRemap,
      knownDestinationNetworks,
      true,
      warnings,
      originalId,
    );
    if (device.ic?.pins) {
      device.ic.pins = remapPins(
        device.ic.pins,
        deviceIdRemap,
        knownDestinationDevices,
        true,
        warnings,
        originalId,
      );
    }
    if (
      device.referenceId !== undefined &&
      usedReferences.has(device.referenceId)
    ) {
      warnings.push(
        `Device “${originalId}” will receive a new ReferenceId because ${device.referenceId} is already used.`,
      );
      delete device.referenceId;
    } else if (device.referenceId !== undefined) {
      usedReferences.add(device.referenceId);
    }
    if (device.ic?.program) {
      const resolved = options.resolveProgramPath?.({
        program: device.ic.program,
        sourceDeviceId: originalId,
        importedDeviceId: device.id,
        origin: options.origin,
        destination: options.destination,
      });
      if (resolved?.trim()) {
        device.ic.program = resolved;
      } else {
        warnings.push(
          options.resolveProgramPath
            ? `IC “${originalId}” program path could not be resolved for the import destination.`
            : `IC “${originalId}” program path was retained; review it for the import destination.`,
        );
      }
    }
    return device;
  });
  if (fragment.layout) {
    const nodeKeyRemap = new Map<string, string>();
    for (const network of fragment.networks) {
      nodeKeyRemap.set(
        topologyNodeKey({ kind: "network", id: network.id, occurrence: 0 }),
        topologyNodeKey({
          kind: "network",
          id: networkIdRemap.get(network.id)!,
          occurrence: 0,
        }),
      );
    }
    for (const device of fragment.devices) {
      nodeKeyRemap.set(
        topologyNodeKey({ kind: "device", id: device.id, occurrence: 0 }),
        topologyNodeKey({
          kind: "device",
          id: deviceIdRemap.get(device.id)!,
          occurrence: 0,
        }),
      );
    }
    imported.layout = {
      schemaVersion: 1,
      nodes: Object.fromEntries(
        Object.entries(fragment.layout.nodes)
          .filter(([key]) => nodeKeyRemap.has(key))
          .map(([key, point]) => [nodeKeyRemap.get(key)!, { ...point }]),
      ),
      ...(fragment.layout.viewport
        ? { viewport: { ...fragment.layout.viewport } }
        : {}),
    };
  }
  return {
    fragment: imported,
    networkIdRemap: Object.fromEntries(networkIdRemap),
    deviceIdRemap: Object.fromEntries(deviceIdRemap),
    destinationFingerprint: topologyDestinationFingerprint(scenario),
    warnings,
  };
}

export function parseTopologyFragment(source: string): FragmentParseResult {
  let parsed: unknown;
  try {
    parsed = JSON.parse(source);
  } catch (error) {
    return { errors: [`Invalid topology fragment JSON: ${String(error)}`] };
  }
  if (!isRecord(parsed)) {
    return { errors: ["Topology fragment must be a JSON object."] };
  }
  const errors: string[] = [];
  rejectUnknownProperties(
    parsed,
    ["schemaVersion", "networks", "devices", "layout"],
    "fragment",
    errors,
  );
  if (parsed.schemaVersion !== 1) {
    errors.push("fragment schemaVersion must be 1.");
  }
  if (!Array.isArray(parsed.networks)) {
    errors.push("fragment networks must be an array.");
  }
  if (!Array.isArray(parsed.devices)) {
    errors.push("fragment devices must be an array.");
  }
  const networks = Array.isArray(parsed.networks)
    ? parsed.networks.flatMap((value, index): EnvironmentNetwork[] =>
        validateFragmentNetwork(value, index, errors)
          ? [clone(value as unknown as EnvironmentNetwork)]
          : [],
      )
    : [];
  const devices = Array.isArray(parsed.devices)
    ? parsed.devices.flatMap((value, index): EnvironmentDevice[] =>
        validateFragmentDevice(value, index, errors)
          ? [clone(value as unknown as EnvironmentDevice)]
          : [],
      )
    : [];
  if (
    Array.isArray(parsed.networks) &&
    Array.isArray(parsed.devices) &&
    parsed.networks.length === 0 &&
    parsed.devices.length === 0
  ) {
    errors.push("fragment must contain at least one network or device.");
  }
  for (const id of duplicateStrings(networks.map(({ id }) => id))) {
    errors.push(`fragment contains duplicate network ID “${id}”.`);
  }
  for (const id of duplicateStrings(devices.map(({ id }) => id))) {
    errors.push(`fragment contains duplicate device ID “${id}”.`);
  }
  let layout: EnvironmentLayoutSidecar | undefined;
  if (parsed.layout !== undefined) {
    const layoutGraph = buildTopologyGraph({
      schemaVersion: 1,
      networks,
      devices,
    });
    const result = parseEnvironmentLayoutSidecar(
      JSON.stringify(parsed.layout),
      layoutGraph,
    );
    errors.push(...result.errors.map((error) => `fragment ${error}`));
    layout = result.layout;
  }
  if (errors.length > 0) {
    return { errors };
  }
  return {
    errors: [],
    fragment: {
      schemaVersion: 1,
      networks,
      devices,
      ...(layout ? { layout } : {}),
    },
  };
}

export function applyFragmentImport(
  scenario: EnvironmentScenario,
  preview: FragmentImportPreview,
): EnvironmentScenario {
  if (
    preview.destinationFingerprint !== topologyDestinationFingerprint(scenario)
  ) {
    throw new Error(
      "The import destination changed after this preview was created; generate a new preview.",
    );
  }
  assertImportStillUnique(scenario, preview.fragment);
  const result = clone(scenario);
  result.networks.push(...clone(preview.fragment.networks));
  result.devices.push(...clone(preview.fragment.devices));
  return result;
}

export function topologyDestinationFingerprint(
  scenario: EnvironmentScenario,
): string {
  return JSON.stringify({
    networks: scenario.networks.map(({ id }) => id).sort(),
    devices: scenario.devices
      .map(({ id, referenceId }) => [id, referenceId ?? null] as const)
      .sort(([left], [right]) => left.localeCompare(right)),
  });
}

export function filterTopologyNodes(
  graph: TopologyGraph,
  filter: TopologyFilter,
  deepSearchProvider?: (node: TopologyNode) => string,
): readonly TopologyNode[] {
  const terms = (filter.query ?? "")
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  const networkKinds = normalizedSet(filter.networkKinds);
  const prefabs = normalizedSet(filter.prefabs);
  const states = new Set(filter.validationStates ?? []);
  return graph.nodes.filter((node) => {
    if (filter.icOnly && !node.isIc) {
      return false;
    }
    if (
      networkKinds.size > 0 &&
      (node.identity.kind !== "network" ||
        !networkKinds.has(node.networkKind?.toLocaleLowerCase() ?? ""))
    ) {
      return false;
    }
    if (
      prefabs.size > 0 &&
      (node.identity.kind !== "device" ||
        !prefabs.has(node.prefab?.toLocaleLowerCase() ?? ""))
    ) {
      return false;
    }
    if (states.size > 0 && !states.has(node.validationState)) {
      return false;
    }
    if (terms.every((term) => node.searchText.includes(term))) {
      return true;
    }
    if (!filter.deepValues || !deepSearchProvider) {
      return false;
    }
    const deepText = boundedSearchText([deepSearchProvider(node)]);
    return terms.every((term) => deepText.includes(term));
  });
}

export function topologyReadingOrder(
  positions: Readonly<Record<string, Point>>,
  eligibleKeys: readonly string[] = Object.keys(positions),
): readonly string[] {
  return [...eligibleKeys]
    .filter((key) => boundedPoint(positions[key]))
    .sort((left, right) => {
      const a = positions[left]!;
      const b = positions[right]!;
      return a.y - b.y || a.x - b.x || left.localeCompare(right);
    });
}

export function buildTopologyFocusItems(
  graph: TopologyGraph,
  positions: Readonly<Record<string, Point>>,
): readonly TopologyFocusItem[] {
  const result: TopologyFocusItem[] = [];
  for (const node of graph.nodes) {
    const point = positions[node.key];
    if (!boundedPoint(point)) {
      continue;
    }
    result.push({
      key: `focus:node:${node.key}`,
      kind: "node",
      selection: { kind: "node", nodeKey: node.key },
      nodeKey: node.key,
      point: { ...point },
      label: node.label,
      description: `${node.identity.kind}, ${node.secondaryLabel}, ${node.validationState}`,
      validationState: node.validationState,
    });
    node.ports.forEach((port, index) => {
      result.push({
        key: `focus:port:${node.key}:${encodeURIComponent(port.connectionKey)}`,
        kind: "port",
        selection: {
          kind: "port",
          nodeKey: node.key,
          connectionKey: port.connectionKey,
        },
        nodeKey: node.key,
        point: { x: point.x + 240, y: point.y + 32 + index * 24 },
        label: port.label,
        description: [
          `port ${port.connectionKey} on ${node.label}`,
          port.connectionType,
          port.connectionRole,
        ]
          .filter(Boolean)
          .join(", "),
        validationState: node.validationState,
      });
    });
  }
  for (const edge of graph.edges) {
    const sourceKey = edge.kind === "connection" ? edge.deviceKey : edge.icKey;
    const targetKey =
      edge.kind === "connection" ? edge.networkKey : edge.targetKey;
    const source = positions[sourceKey];
    if (!boundedPoint(source)) {
      continue;
    }
    const target = targetKey ? positions[targetKey] : undefined;
    const point = boundedPoint(target)
      ? { x: (source.x + target.x) / 2, y: (source.y + target.y) / 2 }
      : { x: source.x + 260, y: source.y + 24 };
    if (edge.kind === "connection") {
      result.push({
        key: `focus:edge:${edge.key}`,
        kind: "edge",
        selection: { kind: "edge", edgeKey: edge.key },
        nodeKey: edge.deviceKey,
        point,
        label: `Connection ${edge.connectionKey}: ${edge.deviceId} to ${edge.networkId}`,
        description: [
          edge.connectionType,
          edge.connectionRole,
          edge.validationState,
        ]
          .filter(Boolean)
          .join(", "),
        validationState: edge.validationState,
      });
    } else {
      result.push({
        key: `focus:pin:${edge.key}`,
        kind: "pin",
        selection: { kind: "pin", edgeKey: edge.key },
        nodeKey: edge.icKey,
        point,
        label: `Pin ${edge.pin}: ${edge.icId} to ${edge.targetId}`,
        description: `${edge.validationState} IC pin link`,
        validationState: edge.validationState,
      });
    }
  }
  const order = new Map(
    topologyReadingOrder(
      Object.fromEntries(result.map(({ key, point }) => [key, point])),
    ).map((key, index) => [key, index]),
  );
  return result.sort(
    (left, right) => order.get(left.key)! - order.get(right.key)!,
  );
}

export function reconcileTopologySelectionWithFallback(
  graph: TopologyGraph,
  selection: TopologySelection | undefined,
  eligibleNodeKeys: readonly string[] = graph.nodes.map(({ key }) => key),
): SelectionReconciliation {
  const eligible = new Set(eligibleNodeKeys);
  const reconciled = reconcileTopologySelection(graph, selection);
  if (reconciled) {
    const nodeKey = selectionNodeKey(graph, reconciled);
    if (!nodeKey || eligible.has(nodeKey)) {
      return { selection: reconciled };
    }
  }
  if (selection) {
    const parent = selectionNodeKey(graph, selection);
    if (parent && graph.nodeByKey.has(parent) && eligible.has(parent)) {
      const node = graph.nodeByKey.get(parent)!;
      return {
        selection: { kind: "node", nodeKey: parent },
        announcement: `Selection is no longer available; moved to ${node.label}.`,
      };
    }
  }
  const fallback = eligibleNodeKeys.find((key) => graph.nodeByKey.has(key));
  return fallback
    ? {
        selection: { kind: "node", nodeKey: fallback },
        announcement: `Selection is no longer available; moved to ${graph.nodeByKey.get(fallback)!.label}.`,
      }
    : selection
      ? { announcement: "Selection is no longer available." }
      : {};
}

export function moveRovingFocus(
  keys: readonly string[],
  current: string | undefined,
  direction: RovingDirection,
): string | undefined {
  if (keys.length === 0) {
    return undefined;
  }
  if (direction === "home") {
    return keys[0];
  }
  if (direction === "end") {
    return keys.at(-1);
  }
  const index = current ? keys.indexOf(current) : -1;
  if (direction === "next") {
    return keys[(index + 1 + keys.length) % keys.length];
  }
  return keys[(index <= 0 ? keys.length : index) - 1];
}

export function spatialFocusNeighbour(
  positions: Readonly<Record<string, Point>>,
  current: string,
  direction: SpatialDirection,
  eligibleKeys: readonly string[] = Object.keys(positions),
): string | undefined {
  const origin = positions[current];
  if (!origin) {
    return undefined;
  }
  const candidates = eligibleKeys
    .filter((key) => key !== current && positions[key])
    .map((key) => {
      const point = positions[key]!;
      const dx = point.x - origin.x;
      const dy = point.y - origin.y;
      const primary =
        direction === "left"
          ? -dx
          : direction === "right"
            ? dx
            : direction === "up"
              ? -dy
              : dy;
      const cross =
        direction === "left" || direction === "right"
          ? Math.abs(dy)
          : Math.abs(dx);
      return {
        key,
        primary,
        cross,
        distance: Math.hypot(dx, dy),
        score: primary + cross * 2,
      };
    })
    .filter(({ primary }) => primary > 0)
    .sort(
      (left, right) =>
        left.score - right.score ||
        left.primary - right.primary ||
        left.cross - right.cross ||
        left.distance - right.distance ||
        left.key.localeCompare(right.key),
    );
  return candidates[0]?.key;
}

function connectionEdgeKey(deviceKey: string, connectionKey: string): string {
  return `connection:${deviceKey}:${encodeURIComponent(connectionKey)}`;
}

function pinEdgeKey(deviceKey: string, pin: string): string {
  return `pin:${deviceKey}:${encodeURIComponent(pin)}`;
}

function occurrences(values: readonly string[]): number[] {
  const counts = new Map<string, number>();
  return values.map((value) => {
    const occurrence = counts.get(value) ?? 0;
    counts.set(value, occurrence + 1);
    return occurrence;
  });
}

function uniqueNodeKeys(
  nodes: readonly TopologyNode[],
  kind: TopologyNodeKind,
): Map<string, string> {
  const grouped = new Map<string, string[]>();
  for (const node of nodes.filter(({ identity }) => identity.kind === kind)) {
    const keys = grouped.get(node.identity.id) ?? [];
    keys.push(node.key);
    grouped.set(node.identity.id, keys);
  }
  return new Map(
    [...grouped]
      .filter(([, keys]) => keys.length === 1)
      .map(([id, keys]) => [id, keys[0]!]),
  );
}

function nodeSort(
  graph: TopologyGraph,
  leftKey: string,
  rightKey: string,
): number {
  const left = graph.nodeByKey.get(leftKey)!;
  const right = graph.nodeByKey.get(rightKey)!;
  return (
    Number(right.isIc) - Number(left.isIc) ||
    left.identity.kind.localeCompare(right.identity.kind) ||
    left.label.localeCompare(right.label) ||
    left.key.localeCompare(right.key)
  );
}

function breadthFirstDistances(
  root: string,
  component: readonly string[],
  adjacency: ReadonlyMap<string, ReadonlySet<string>>,
): Map<string, number> {
  const allowed = new Set(component);
  const distances = new Map([[root, 0]]);
  const queue = [root];
  while (queue.length > 0) {
    const key = queue.shift()!;
    const nextDistance = (distances.get(key) ?? 0) + 1;
    for (const neighbour of [...(adjacency.get(key) ?? [])].sort()) {
      if (allowed.has(neighbour) && !distances.has(neighbour)) {
        distances.set(neighbour, nextDistance);
        queue.push(neighbour);
      }
    }
  }
  return distances;
}

function findTargetNode(
  graph: TopologyGraph,
  kind: TopologyNodeKind,
  id: string | undefined,
  index: number | undefined,
): TopologyNode | undefined {
  if (index !== undefined) {
    const indexed = graph.nodes.find(
      (node) => node.identity.kind === kind && node.sourceIndex === index,
    );
    if (indexed) {
      return indexed;
    }
  }
  const matches = graph.nodes.filter(
    (node) => node.identity.kind === kind && node.identity.id === id,
  );
  return matches.length === 1 ? matches[0] : undefined;
}

function propertyName(
  property: string | undefined,
  prefix: string,
): string | undefined {
  const match = property?.match(new RegExp(`^${prefix}\\.([^.]+)(?:\\.|$)`));
  return match?.[1];
}

function promoteValidation(
  value: { validationState: ValidationState } | undefined,
  state: Exclude<ValidationState, "valid">,
): void {
  if (!value || value.validationState === "error") {
    return;
  }
  value.validationState = state;
}

function remapConnections(
  connections: Record<string, string> | undefined,
  remap: ReadonlyMap<string, string>,
  knownExternal: ReadonlySet<string>,
  preserveExternal: boolean,
  warnings: string[],
  owner: string,
): Record<string, string> {
  return Object.fromEntries(
    sortedEntries(connections ?? {}).flatMap(([port, network]) => {
      const mapped = remap.get(network);
      if (mapped !== undefined) {
        return [[port, mapped]];
      }
      if (preserveExternal && knownExternal.has(network)) {
        return [[port, network]];
      }
      warnings.push(
        `Device “${owner}” connection ${port} was detached from external network “${network}”.`,
      );
      return [];
    }),
  );
}

function remapPins(
  pins: Record<string, string>,
  remap: ReadonlyMap<string, string>,
  knownExternal: ReadonlySet<string>,
  preserveExternal: boolean,
  warnings: string[],
  owner: string,
): Record<string, string> {
  return Object.fromEntries(
    sortedEntries(pins).flatMap(([pin, target]) => {
      const mapped = remap.get(target);
      if (mapped !== undefined) {
        return [[pin, mapped]];
      }
      if (preserveExternal && knownExternal.has(target)) {
        return [[pin, target]];
      }
      warnings.push(
        `IC “${owner}” pin ${pin} was cleared because target “${target}” was not included.`,
      );
      return [];
    }),
  );
}

function uniqueCopyId(base: string, used: ReadonlySet<string>): string {
  return uniqueSuffixedId(`${base}-copy`, used);
}

function uniqueImportId(base: string, used: ReadonlySet<string>): string {
  return used.has(base) ? uniqueSuffixedId(`${base}-import`, used) : base;
}

function uniqueSuffixedId(base: string, used: ReadonlySet<string>): string {
  if (!used.has(base)) {
    return base;
  }
  let suffix = 2;
  while (used.has(`${base}-${suffix}`)) {
    suffix += 1;
  }
  return `${base}-${suffix}`;
}

function uniqueCopyName(base: string, used: ReadonlySet<string>): string {
  return uniqueSuffixedId(`${base} Copy`, used);
}

function searchableNetwork(network: EnvironmentNetwork): string {
  return boundedSearchText([
    network.id,
    network.kind,
    network.cableRole,
    ...Object.keys(network.channels ?? {}),
  ]);
}

function searchableDevice(
  device: EnvironmentDevice,
  metadata: DeviceTopologyMetadata | undefined,
): string {
  return boundedSearchText([
    device.id,
    device.name,
    device.prefab,
    metadata?.displayName,
    device.ic?.program,
    ...Object.keys(device.fields ?? {}),
    ...Object.keys(device.slots ?? {}),
    ...Object.values(device.slots ?? {}).flatMap((slot) => Object.keys(slot)),
    ...Object.keys(device.memory ?? {}),
    ...Object.keys(device.ic?.registers ?? {}),
    ...Object.keys(device.ic?.stack ?? {}),
  ]);
}

function boundedSearchText(values: readonly unknown[]): string {
  return values
    .filter((value): value is string | number => {
      return typeof value === "string" || typeof value === "number";
    })
    .join(" ")
    .toLocaleLowerCase()
    .slice(0, TOPOLOGY_SEARCH_TEXT_LIMIT);
}

function normalizedSet(values: readonly string[] | undefined): Set<string> {
  return new Set((values ?? []).map((value) => value.toLocaleLowerCase()));
}

function duplicateStrings(values: readonly string[]): string[] {
  const seen = new Set<string>();
  const duplicates = new Set<string>();
  for (const value of values) {
    if (seen.has(value)) {
      duplicates.add(value);
    }
    seen.add(value);
  }
  return [...duplicates].sort();
}

function sortedEntries(
  value: Record<string, string>,
): [string, string][] {
  return Object.entries(value).sort(([left], [right]) =>
    left.localeCompare(right, undefined, { numeric: true }),
  );
}

function displayMetadata(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function devicePorts(
  device: EnvironmentDevice,
  metadata: DeviceTopologyMetadata | undefined,
): readonly TopologyPort[] {
  const keys = new Set(Object.keys(device.connections ?? {}));
  metadata?.connections?.forEach((_connection, index) => keys.add(String(index)));
  return [...keys]
    .sort((left, right) =>
      left.localeCompare(right, undefined, { numeric: true }),
    )
    .map((connectionKey) => {
      const connection = /^\d+$/.test(connectionKey)
        ? Number(connectionKey)
        : undefined;
      const type =
        connection === undefined
          ? undefined
          : displayMetadata(metadata?.connections?.[connection]?.type);
      const role =
        connection === undefined
          ? undefined
          : displayMetadata(metadata?.connections?.[connection]?.role);
      return {
        key: connectionEdgeKey(
          topologyNodeKey({ kind: "device", id: device.id, occurrence: 0 }),
          connectionKey,
        ),
        connectionKey,
        ...(connection !== undefined ? { connection } : {}),
        label: [`Connection ${connectionKey}`, type, role]
          .filter(Boolean)
          .join(" · "),
        ...(type ? { connectionType: type } : {}),
        ...(role ? { connectionRole: role } : {}),
      };
    });
}

function persistableNodeKeys(graph: TopologyGraph): ReadonlySet<string> {
  const counts = new Map<string, number>();
  for (const node of graph.nodes) {
    const identity = `${node.identity.kind}\0${node.identity.id}`;
    counts.set(identity, (counts.get(identity) ?? 0) + 1);
  }
  return new Set(
    graph.nodes
      .filter(
        (node) =>
          counts.get(`${node.identity.kind}\0${node.identity.id}`) === 1,
      )
      .map(({ key }) => key),
  );
}

function numericConnectionKey(value: number | undefined): string | undefined {
  return value === undefined || !Number.isInteger(value)
    ? undefined
    : String(value);
}

function selectionNodeKey(
  graph: TopologyGraph,
  selection: TopologySelection,
): string | undefined {
  if (selection.kind === "node" || selection.kind === "port") {
    return selection.nodeKey;
  }
  const edge = graph.edgeByKey.get(selection.edgeKey);
  return edge?.kind === "connection"
    ? edge.deviceKey
    : edge?.kind === "pin"
      ? edge.icKey
      : undefined;
}

function rejectUnknownProperties(
  value: Record<string, unknown>,
  allowed: readonly string[],
  label: string,
  errors: string[],
): void {
  const allowedSet = new Set(allowed);
  for (const key of Object.keys(value)) {
    if (!allowedSet.has(key)) {
      errors.push(`${label} contains unknown property ${JSON.stringify(key)}.`);
    }
  }
}

function scalar(value: unknown): value is Scalar {
  return (
    finiteNumber(value) ||
    value === "NaN" ||
    value === "Infinity" ||
    value === "-Infinity" ||
    value === "-0"
  );
}

function scalarMap(
  value: unknown,
  label: string,
  errors: string[],
): value is Record<string, Scalar> {
  if (!isRecord(value) || Object.values(value).some((item) => !scalar(item))) {
    errors.push(`${label} must map names to numeric or special scalar values.`);
    return false;
  }
  return true;
}

function validateFragmentNetwork(
  value: unknown,
  index: number,
  errors: string[],
): value is EnvironmentNetwork {
  const label = `fragment network ${index + 1}`;
  if (!isRecord(value)) {
    errors.push(`${label} must be an object.`);
    return false;
  }
  rejectUnknownProperties(
    value,
    ["id", "kind", "cableRole", "channels"],
    label,
    errors,
  );
  let valid =
    typeof value.id === "string" &&
    value.id.length > 0 &&
    typeof value.kind === "string" &&
    ["cable", "chute", "gas", "liquid"].includes(value.kind);
  if (!valid) {
    errors.push(`${label} needs a non-empty id and supported kind.`);
  }
  if (
    value.cableRole !== undefined &&
    (typeof value.cableRole !== "string" ||
      !["data", "power", "powerAndData"].includes(value.cableRole))
  ) {
    errors.push(`${label} cableRole must be data, power, or powerAndData.`);
    valid = false;
  }
  if (
    value.channels !== undefined &&
    !scalarMap(value.channels, `${label} channels`, errors)
  ) {
    valid = false;
  }
  return valid;
}

function validateFragmentDevice(
  value: unknown,
  index: number,
  errors: string[],
): value is EnvironmentDevice {
  const label = `fragment device ${index + 1}`;
  if (!isRecord(value)) {
    errors.push(`${label} must be an object.`);
    return false;
  }
  rejectUnknownProperties(
    value,
    [
      "id",
      "prefab",
      "name",
      "referenceId",
      "connections",
      "fields",
      "slots",
      "memory",
      "ic",
    ],
    label,
    errors,
  );
  let valid =
    typeof value.id === "string" &&
    value.id.length > 0 &&
    typeof value.prefab === "string" &&
    value.prefab.length > 0;
  if (!valid) {
    errors.push(`${label} needs non-empty id and prefab.`);
  }
  if (value.name !== undefined && typeof value.name !== "string") {
    errors.push(`${label} name must be a string.`);
    valid = false;
  }
  if (
    value.referenceId !== undefined &&
    (!Number.isInteger(value.referenceId) || !finiteNumber(value.referenceId))
  ) {
    errors.push(`${label} referenceId must be an integer.`);
    valid = false;
  }
  if (
    value.connections !== undefined &&
    (!isRecord(value.connections) ||
      Object.values(value.connections).some(
        (network) => typeof network !== "string",
      ))
  ) {
    errors.push(`${label} connections must map ports to network IDs.`);
    valid = false;
  }
  for (const property of ["fields", "memory"] as const) {
    if (
      value[property] !== undefined &&
      !scalarMap(value[property], `${label} ${property}`, errors)
    ) {
      valid = false;
    }
  }
  if (value.slots !== undefined) {
    if (!isRecord(value.slots)) {
      errors.push(`${label} slots must be an object.`);
      valid = false;
    } else {
      for (const [slot, fields] of Object.entries(value.slots)) {
        if (!scalarMap(fields, `${label} slot ${slot}`, errors)) {
          valid = false;
        }
      }
    }
  }
  if (value.ic !== undefined && !validateFragmentIc(value.ic, label, errors)) {
    valid = false;
  }
  return valid;
}

function validateFragmentIc(
  value: unknown,
  ownerLabel: string,
  errors: string[],
): value is EnvironmentIc {
  const label = `${ownerLabel} ic`;
  if (!isRecord(value)) {
    errors.push(`${label} must be an object.`);
    return false;
  }
  rejectUnknownProperties(
    value,
    ["program", "enabled", "pins", "registers", "stack"],
    label,
    errors,
  );
  let valid = typeof value.program === "string" && value.program.length > 0;
  if (!valid) {
    errors.push(`${label} program must be a non-empty string.`);
  }
  if (value.enabled !== undefined && typeof value.enabled !== "boolean") {
    errors.push(`${label} enabled must be boolean.`);
    valid = false;
  }
  if (value.pins !== undefined) {
    if (
      !isRecord(value.pins) ||
      Object.entries(value.pins).some(
        ([pin, target]) =>
          !/^d[0-5]$/.test(pin) ||
          typeof target !== "string",
      )
    ) {
      errors.push(`${label} pins must map d0 through d5 to device IDs.`);
      valid = false;
    }
  }
  for (const property of ["registers", "stack"] as const) {
    if (
      value[property] !== undefined &&
      !scalarMap(value[property], `${label} ${property}`, errors)
    ) {
      valid = false;
    }
  }
  if (
    isRecord(value.registers) &&
    Object.keys(value.registers).some(
      (key) => !/^(?:r(?:[0-9]|1[0-7])|ra|sp)$/.test(key),
    )
  ) {
    errors.push(`${label} register keys must be r0 through r17.`);
    valid = false;
  }
  if (
    isRecord(value.stack) &&
    Object.keys(value.stack).some(
      (key) => !/^(?:0|[1-9]\d{0,2})$/.test(key) || Number(key) > 511,
    )
  ) {
    errors.push(`${label} stack keys must be integer addresses 0 through 511.`);
    valid = false;
  }
  return valid;
}

function assertImportStillUnique(
  scenario: EnvironmentScenario,
  fragment: TopologyFragment,
): void {
  const networks = new Set(scenario.networks.map(({ id }) => id));
  const devices = new Set(scenario.devices.map(({ id }) => id));
  const references = new Set(
    scenario.devices
      .map(({ referenceId }) => referenceId)
      .filter((value): value is number => value !== undefined),
  );
  if (fragment.networks.some(({ id }) => networks.has(id))) {
    throw new Error("The import preview contains a network ID collision.");
  }
  if (fragment.devices.some(({ id }) => devices.has(id))) {
    throw new Error("The import preview contains a device ID collision.");
  }
  if (
    fragment.devices.some(
      ({ referenceId }) =>
        referenceId !== undefined && references.has(referenceId),
    )
  ) {
    throw new Error("The import preview contains a ReferenceId collision.");
  }
}

function finiteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function finitePoint(value: Point): boolean {
  return finiteNumber(value.x) && finiteNumber(value.y);
}

function boundedNumber(value: unknown): value is number {
  return (
    finiteNumber(value) &&
    value >= -TOPOLOGY_COORDINATE_LIMIT &&
    value <= TOPOLOGY_COORDINATE_LIMIT
  );
}

function boundedPoint(value: Point | undefined): value is Point {
  return (
    value !== undefined && boundedNumber(value.x) && boundedNumber(value.y)
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function clone<T>(value: T): T {
  return structuredClone(value);
}
