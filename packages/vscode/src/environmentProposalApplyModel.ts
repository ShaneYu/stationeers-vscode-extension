import type { EnvironmentProposalPreview } from "./environmentProposalModel.ts";
import type { EnvironmentScenario } from "./environmentTopologyModel.ts";

export interface ProposalPrefabMetadata {
  readonly connections: readonly { readonly type: unknown }[];
  readonly slots: Readonly<
    Record<
      string,
      {
        readonly logicTypes: Readonly<Record<string, unknown>>;
      }
    >
  >;
}

export function scenarioFromEnvironmentProposal(
  preview: EnvironmentProposalPreview,
  selectedPrefabs: Readonly<Record<string, string>>,
  programPath: string,
  catalog: Readonly<Record<string, ProposalPrefabMetadata | undefined>>,
): EnvironmentScenario {
  const proposal = preview.proposal;
  const requireUnique = (kind: string, values: readonly string[]): void => {
    const seen = new Set<string>();
    for (const value of values) {
      if (!value.trim() || seen.has(value)) {
        throw new Error(
          value.trim()
            ? `The proposal contains a duplicate ${kind} ID: ${value}.`
            : `The proposal contains an empty ${kind} ID.`,
        );
      }
      seen.add(value);
    }
  };
  requireUnique(
    "network",
    proposal.networks.map(({ suggestedId }) => suggestedId),
  );
  requireUnique("device", [
    proposal.housing.suggestedId,
    ...proposal.devices.map(({ suggestedId }) => suggestedId),
    ...proposal.batchGroups.map(
      (group, index) =>
        group.suggestedName?.trim() || `batch-device-${index + 1}`,
    ),
  ]);
  const choose = (
    key: string,
    candidates: readonly { prefabName: string }[],
  ): string => {
    const selected = selectedPrefabs[key];
    if (
      !selected ||
      !candidates.some(({ prefabName }) => prefabName === selected)
    ) {
      throw new Error(`Choose a valid prefab for ${key}.`);
    }
    return selected;
  };
  const networks: EnvironmentScenario["networks"] = proposal.networks.map(
    (network) => ({
      id: network.suggestedId,
      kind: network.kind === "pipe" ? "gas" : network.kind,
      ...(network.kind === "cable"
        ? { cableRole: network.cableRole ?? "powerAndData" }
        : {}),
    }),
  );
  if (networks.length === 0) {
    networks.push({
      id: "data",
      kind: "cable",
      cableRole: "powerAndData",
    });
  }
  const dataNetwork =
    networks.find(
      (network) =>
        network.kind === "cable" &&
        (network.cableRole === "data" ||
          network.cableRole === "powerAndData"),
    ) ?? networks[0]!;
  const metadataFor = (prefab: string): ProposalPrefabMetadata => {
    const metadata = catalog[prefab];
    if (!metadata) {
      throw new Error(
        `Cannot apply the proposal because metadata for ${prefab} is unavailable.`,
      );
    }
    return metadata;
  };
  const dataConnection = (prefab: string): string => {
    const index = metadataFor(prefab).connections.findIndex(({ type }) =>
      String(type).toLowerCase().includes("data"),
    );
    if (index < 0) {
      throw new Error(
        `${prefab} has no data connection for network ${dataNetwork.id}.`,
      );
    }
    return String(index);
  };
  const compatibleSlot = (
    prefab: string,
    requiredFields: readonly string[],
  ): string => {
    const slot = Object.entries(metadataFor(prefab).slots)
      .filter(([, metadata]) =>
        requiredFields.every((field) => field in metadata.logicTypes),
      )
      .sort(([left], [right]) => Number(left) - Number(right))[0];
    if (!slot) {
      throw new Error(
        `${prefab} has no slot supporting ${requiredFields.join(", ")}.`,
      );
    }
    return slot[0];
  };
  const devices: EnvironmentScenario["devices"] = proposal.devices.map(
    (device) => {
      const prefab = choose(device.reference, device.candidates);
      return {
        id: device.suggestedId,
        prefab,
        connections: { [dataConnection(prefab)]: dataNetwork.id },
        fields: Object.fromEntries(
          device.requiredFields.map(({ name }) => [name, 0]),
        ),
        ...(device.requiredSlotFields.length > 0
          ? {
              slots: {
                [compatibleSlot(prefab, device.requiredSlotFields)]:
                  Object.fromEntries(
                    device.requiredSlotFields.map((name) => [name, 0]),
                  ),
              },
            }
          : {}),
        ...(device.requiresMemory ? { memory: {} } : {}),
      };
    },
  );
  const batchDevices: EnvironmentScenario["devices"] =
    proposal.batchGroups.map((group, index) => {
      const prefab = choose(`batch:${index}`, group.candidates);
      return {
        id: group.suggestedName?.trim() || `batch-device-${index + 1}`,
        prefab,
        ...(group.suggestedName ? { name: group.suggestedName } : {}),
        connections: { [dataConnection(prefab)]: dataNetwork.id },
        fields: Object.fromEntries(
          group.requiredFields.map(({ name }) => [name, 0]),
        ),
      };
    });
  const pins = Object.fromEntries(
    proposal.devices.flatMap((device) =>
      device.pin === undefined
        ? []
        : [[`d${device.pin}`, device.suggestedId]],
    ),
  );
  return {
    schemaVersion: 1,
    networks,
    devices: [
      {
        id: proposal.housing.suggestedId,
        prefab: proposal.housing.prefab.prefabName,
        name: proposal.housing.suggestedName,
        connections: {
          [dataConnection(proposal.housing.prefab.prefabName)]: dataNetwork.id,
        },
        fields: Object.fromEntries(
          proposal.housing.requiredFields.map(({ name }) => [name, 0]),
        ),
        ic: {
          program: programPath,
          enabled: true,
          pins,
          registers: {},
          stack: {},
        },
      },
      ...devices,
      ...batchDevices,
    ],
  };
}
