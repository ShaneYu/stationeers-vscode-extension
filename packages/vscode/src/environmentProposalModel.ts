export interface SourceEvidence {
  readonly line: number;
  readonly startCharacter: number;
  readonly endCharacter: number;
  readonly text: string;
}

export interface PrefabCandidate {
  readonly prefabName: string;
  readonly prefabHash: number;
  readonly displayName: string;
  readonly confidence: number;
  readonly reason: string;
}

export interface FieldRequirement {
  readonly name: string;
  readonly read: boolean;
  readonly write: boolean;
}

export interface DeviceProposal {
  readonly reference: string;
  readonly aliases: readonly string[];
  readonly suggestedId: string;
  readonly pin?: number;
  readonly candidates: readonly PrefabCandidate[];
  readonly requiredFields: readonly FieldRequirement[];
  readonly requiredSlotFields: readonly string[];
  readonly requiresMemory: boolean;
  readonly confidence: number;
  readonly reasons: readonly string[];
  readonly evidence: readonly SourceEvidence[];
}

export interface BatchGroupProposal {
  readonly prefabHashExpression: string;
  readonly nameHashExpression?: string;
  readonly suggestedName?: string;
  readonly candidates: readonly PrefabCandidate[];
  readonly requiredFields: readonly FieldRequirement[];
  readonly confidence: number;
  readonly reasons: readonly string[];
  readonly evidence: readonly SourceEvidence[];
}

export interface NetworkProposal {
  readonly suggestedId: string;
  readonly kind: "cable" | "chute" | "pipe";
  readonly cableRole?: "data" | "power" | "powerAndData";
  readonly participants: readonly string[];
  readonly channels: readonly number[];
  readonly reason: string;
}

export interface UnresolvedAssumption {
  readonly code: string;
  readonly message: string;
  readonly reference?: string;
  readonly evidence?: SourceEvidence;
}

export interface EnvironmentProposal {
  readonly schemaVersion: 1;
  readonly sourceUri: string;
  readonly previewOnly: true;
  readonly housing: {
    readonly suggestedId: string;
    readonly suggestedName: string;
    readonly programUri: string;
    readonly prefab: PrefabCandidate;
    readonly requiredFields: readonly FieldRequirement[];
    readonly channels: readonly number[];
  };
  readonly devices: readonly DeviceProposal[];
  readonly batchGroups: readonly BatchGroupProposal[];
  readonly networks: readonly NetworkProposal[];
  readonly unresolved: readonly UnresolvedAssumption[];
}

export interface EnvironmentProposalPreview {
  readonly type: "environmentProposalPreview";
  readonly proposal: EnvironmentProposal;
  readonly selectedPrefabs: Readonly<Record<string, string>>;
  readonly blockers: readonly string[];
  readonly canApply: boolean;
}

export function buildEnvironmentProposalPreview(
  proposal: EnvironmentProposal,
): EnvironmentProposalPreview {
  const selectedPrefabs: Record<string, string> = {};
  const blockers = proposal.unresolved.map((item) => item.message);
  for (const device of proposal.devices) {
    const candidate = device.candidates[0];
    if (candidate) {
      selectedPrefabs[device.reference] = candidate.prefabName;
    } else {
      blockers.push(`Choose a prefab for ${device.reference}.`);
    }
  }
  for (const [index, group] of proposal.batchGroups.entries()) {
    const candidate = group.candidates[0];
    if (candidate) {
      selectedPrefabs[`batch:${index}`] = candidate.prefabName;
    } else {
      blockers.push(
        `Choose a prefab for batch expression ${group.prefabHashExpression}.`,
      );
    }
  }
  return {
    type: "environmentProposalPreview",
    proposal,
    selectedPrefabs,
    blockers,
    canApply: blockers.length === 0,
  };
}

export function validateEnvironmentProposal(
  value: unknown,
  expectedSourceUri: string,
): EnvironmentProposal {
  if (!value || typeof value !== "object") {
    throw new Error("The language server returned an invalid environment proposal.");
  }
  const proposal = value as Partial<EnvironmentProposal>;
  if (
    proposal.schemaVersion !== 1 ||
    proposal.previewOnly !== true ||
    proposal.sourceUri !== expectedSourceUri ||
    proposal.housing?.programUri !== expectedSourceUri ||
    !Array.isArray(proposal.devices) ||
    !Array.isArray(proposal.batchGroups) ||
    !Array.isArray(proposal.networks) ||
    !Array.isArray(proposal.unresolved)
  ) {
    throw new Error(
      "The language server returned an incompatible or cross-document proposal.",
    );
  }
  return proposal as EnvironmentProposal;
}
