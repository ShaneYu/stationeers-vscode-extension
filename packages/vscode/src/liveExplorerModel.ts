import type { BridgeChip, BridgeHello, BridgeScope, BridgeSnapshot, BridgeState } from "./bridge";

export interface LiveExplorerRow { key: string; kind: "scope" | "chip"; label: string; description: string; chip?: BridgeChip }

export function formatChipDescription(chip: BridgeChip): string {
  const status = `${chip.language.toUpperCase()} · ${chip.powered ? "powered" : "unpowered"}`;
  const length = chip.source.length ?? chip.source.bytes;
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
