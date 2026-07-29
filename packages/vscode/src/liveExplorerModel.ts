import type { BridgeChip, BridgeHello, BridgeScope, BridgeSnapshot, BridgeState } from "./bridge";

export interface LiveExplorerRow { key: string; kind: "scope" | "chip"; label: string; description: string; chip?: BridgeChip }

export function formatChipDescription(chip: BridgeChip, sourceLength?: number): string {
  const status = `${chip.language.toUpperCase()} · ${chip.powered ? "powered" : "unpowered"}`;
  const length = sourceLength ?? chip.source.length ?? chip.source.bytes;
  return typeof length === "number" && Number.isSafeInteger(length) && length >= 0 ? `${status} · ${length} bytes` : status;
}

export interface LiveChipContext {
  language: string;
  stale: boolean;
  available: boolean;
  canRead: boolean;
  canCompare: boolean;
  luaDebugEligible: boolean;
}

export interface StationeersLuaChipMetadata {
  ref_id: number | string;
  housing_ref_id: number | string;
  is_lua: boolean;
  source_length?: number;
  source_version?: number | string;
}

export interface StationeersLuaScope {
  serviceAvailable: boolean;
  scopeAvailable: boolean;
  chips: readonly StationeersLuaChipMetadata[];
  editorOpen?: boolean;
  selectedChipRefId?: number | string | null;
  selectedHousingRefId?: number | string | null;
}

export type LuaAccessibilityReason =
  | "accessible"
  | "service-unavailable"
  | "no-scope"
  | "missing-chip"
  | "identity-mismatch"
  | "duplicate-chip";

export interface LuaAccessibilityProjection {
  accessible: boolean;
  reason: LuaAccessibilityReason;
  accessStatus: "selected_editor" | "network_scope" | null;
  correlation: "chip_and_housing" | "unique_housing" | null;
  target: {
    refId: string;
    housingRefId: string;
    sourceLength?: number;
    sourceVersion?: string;
  } | null;
  status: string;
  tooltip: string;
  iconState: "accessible" | "inaccessible" | "unavailable";
}

/**
 * Correlates a globally discovered bridge Lua chip with the current
 * StationeersLua editor/wireless scope. Non-Lua chips have no projection so
 * their existing explorer behaviour remains unchanged.
 */
export function projectLuaAccessibility(
  chip: BridgeChip,
  stationeersLua: StationeersLuaScope,
): LuaAccessibilityProjection | undefined {
  if (chip.language !== "lua") return undefined;
  if (!stationeersLua.serviceAvailable) {
    return luaProjection("service-unavailable", "Lua service unavailable", "StationeersLua is unavailable. Start the service and reconnect.", "unavailable");
  }
  if (!stationeersLua.scopeAvailable) {
    return luaProjection("no-scope", "Lua scope not connected", "Connect an IC editor or Wireless Development Board to access this Lua chip.", "inaccessible");
  }

  const chipReferenceId = referenceId(chip.chipId);
  const housingReferenceId = referenceId(chip.housingReferenceId);
  const exactCandidates = chipReferenceId === undefined
    ? []
    : stationeersLua.chips.filter((candidate) => referenceId(candidate.ref_id) === chipReferenceId);
  const housingCandidates = housingReferenceId === undefined || chip.identitySource !== "housing"
    ? []
    : stationeersLua.chips.filter((candidate) => referenceId(candidate.housing_ref_id) === housingReferenceId);
  const candidates = exactCandidates.length > 0 ? exactCandidates : housingCandidates;
  const correlation = exactCandidates.length > 0 ? "chip_and_housing" : "unique_housing";

  if (candidates.length === 0) {
    return luaProjection("missing-chip", "Lua chip out of scope", "This Lua chip is not in the current StationeersLua editor or wireless scope.", "inaccessible");
  }
  if (candidates.length > 1) {
    return luaProjection("duplicate-chip", "Lua identity ambiguous", "StationeersLua reported duplicate entries for this chip ReferenceId.", "unavailable");
  }

  const candidate = candidates[0]!;
  const candidateRefId = referenceId(candidate.ref_id);
  const candidateHousingRefId = referenceId(candidate.housing_ref_id);
  if (
    candidateRefId === undefined
    || candidateHousingRefId === undefined
    || housingReferenceId === undefined
    || candidateHousingRefId !== housingReferenceId
    || candidate.is_lua !== true
  ) {
    return luaProjection("identity-mismatch", "Lua identity mismatch", "The StationeersLua chip or housing identity does not match bridge discovery.", "unavailable");
  }

  const selected = stationeersLua.editorOpen === true
    && referenceId(stationeersLua.selectedChipRefId ?? "") === candidateRefId
    && referenceId(stationeersLua.selectedHousingRefId ?? "") === candidateHousingRefId;
  const sourceLength = typeof candidate.source_length === "number"
    && Number.isSafeInteger(candidate.source_length)
    && candidate.source_length >= 0
    ? candidate.source_length
    : undefined;
  const sourceVersion = candidate.source_version === undefined
    ? undefined
    : versionId(candidate.source_version);
  return luaProjection(
    "accessible",
    "Lua chip accessible",
    selected
      ? "This Lua chip is the exact chip selected in the Stationeers editor."
      : "This Lua chip is accessible through the current StationeersLua network scope.",
    "accessible",
    true,
    selected ? "selected_editor" : "network_scope",
    correlation,
    {
      refId: candidateRefId,
      housingRefId: candidateHousingRefId,
      sourceLength,
      sourceVersion,
    },
  );
}

/** Computes selection context without depending on the VS Code host. */
export function getLiveChipContext(chip: BridgeChip, state: BridgeState, hello?: BridgeHello): LiveChipContext {
  const connected = state === "connected";
  const ic10Readable = chip.language === "ic10" && chip.source.readable && Boolean(hello?.capabilities.ic10SourceRead);
  const luaDebugEligible = connected && chip.language === "lua" && Boolean(hello?.mods?.stationeersLua?.detected);
  return {
    language: chip.language,
    stale: !connected,
    available: connected,
    canRead: connected && ic10Readable,
    canCompare: connected && ic10Readable,
    luaDebugEligible,
  };
}

/** Pure snapshot projection used by the native tree and fixture tests. */
export function buildLiveExplorerRows(snapshot: BridgeSnapshot): LiveExplorerRow[] {
  return snapshot.scopes.slice().sort(scopeSort).flatMap((scope) => [
    { key: `scope:${scope.scopeId}`, kind: "scope" as const, label: scope.disambiguator ? `${scope.name} · ${scope.disambiguator}` : scope.name, description: `${scope.anchorCount} anchor${scope.anchorCount === 1 ? "" : "s"}` },
    ...scope.chipIds.map((id) => snapshot.chips.find((chip) => chip.chipId === id)).filter((chip): chip is BridgeChip => Boolean(chip)).sort(chipSort).map((chip) => ({ key: `chip:${scope.scopeId}:${chip.chipId}`, kind: "chip" as const, label: chip.housingName, description: formatChipDescription(chip), chip })),
  ]);
}
function scopeSort(a: BridgeScope, b: BridgeScope): number { return a.name.localeCompare(b.name) || (a.disambiguator ?? "").localeCompare(b.disambiguator ?? "") || a.scopeId.localeCompare(b.scopeId); }
function chipSort(a: BridgeChip, b: BridgeChip): number { return a.housingName.localeCompare(b.housingName) || a.housingReferenceId.localeCompare(b.housingReferenceId); }
function referenceId(value: number | string): string | undefined {
  if (typeof value === "number") return Number.isSafeInteger(value) && value > 0 ? String(value) : undefined;
  return /^[1-9]\d*$/.test(value) && Number.isSafeInteger(Number(value)) ? value : undefined;
}
function versionId(value: number | string): string | undefined {
  if (typeof value === "number") return Number.isSafeInteger(value) && value >= 0 ? String(value) : undefined;
  return /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) ? value : undefined;
}
function luaProjection(
  reason: LuaAccessibilityReason,
  status: string,
  tooltip: string,
  iconState: LuaAccessibilityProjection["iconState"],
  accessible = false,
  accessStatus: LuaAccessibilityProjection["accessStatus"] = null,
  correlation: LuaAccessibilityProjection["correlation"] = null,
  target: LuaAccessibilityProjection["target"] = null,
): LuaAccessibilityProjection {
  return { accessible, reason, accessStatus, correlation, target, status, tooltip, iconState };
}
