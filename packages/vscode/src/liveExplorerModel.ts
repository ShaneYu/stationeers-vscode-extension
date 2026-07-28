import type { BridgeChip, BridgeScope, BridgeSnapshot } from "./bridge";

export interface LiveExplorerRow { key: string; kind: "scope" | "chip"; label: string; description: string; chip?: BridgeChip }

/** Pure snapshot projection used by the native tree and fixture tests. */
export function buildLiveExplorerRows(snapshot: BridgeSnapshot): LiveExplorerRow[] {
  return snapshot.scopes.slice().sort(scopeSort).flatMap((scope) => [
    { key: `scope:${scope.scopeId}`, kind: "scope" as const, label: scope.disambiguator ? `${scope.name} · ${scope.disambiguator}` : scope.name, description: `${scope.anchorCount} anchor${scope.anchorCount === 1 ? "" : "s"}` },
    ...scope.chipIds.map((id) => snapshot.chips.find((chip) => chip.chipId === id)).filter((chip): chip is BridgeChip => Boolean(chip)).sort(chipSort).map((chip) => ({ key: `chip:${scope.scopeId}:${chip.chipId}`, kind: "chip" as const, label: chip.housingName, description: `${chip.language.toUpperCase()} · ${chip.powered ? "powered" : "unpowered"}`, chip })),
  ]);
}
function scopeSort(a: BridgeScope, b: BridgeScope): number { return a.name.localeCompare(b.name) || (a.disambiguator ?? "").localeCompare(b.disambiguator ?? "") || a.scopeId.localeCompare(b.scopeId); }
function chipSort(a: BridgeChip, b: BridgeChip): number { return a.housingName.localeCompare(b.housingName) || a.housingReferenceId.localeCompare(b.housingReferenceId); }
