import type { BridgeSource } from "./bridge";

export interface LiveSourceIdentity {
  worldEpoch: string;
  chipId: string;
  housingReferenceId: string;
  language: string;
}

export interface LiveSourceSession {
  identity: LiveSourceIdentity;
  version: string;
  length: number;
  sha256: string;
  source: string;
  networkName: string;
  chipName: string;
  uri: string;
}

export function liveSourceIdentity(source: BridgeSource): LiveSourceIdentity {
  return { worldEpoch: source.worldEpoch, chipId: source.chipId, housingReferenceId: source.housingReferenceId, language: source.language };
}

export function liveSourceKey(identity: LiveSourceIdentity): string {
  return [identity.worldEpoch, identity.chipId, identity.housingReferenceId, identity.language].join("\u001f");
}

export function liveSourceLabel(networkName: string, chipName: string): string {
  return `${networkName} — ${chipName}`;
}
