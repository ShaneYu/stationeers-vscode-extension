export const CANONICAL_SIM_SUFFIX = ".icsim";
export const CANONICAL_TEST_SUFFIX = ".ictest";
export const CANONICAL_LAYOUT_SUFFIX = ".icsimlayout";

export const SIM_GLOB = `**/*${CANONICAL_SIM_SUFFIX}`;
export const TEST_GLOB = `**/*${CANONICAL_TEST_SUFFIX}`;
export const PROGRAM_GLOB = "**/*.{ic10,lua}";

export function isSimulationPath(path: string): boolean {
  return path.endsWith(CANONICAL_SIM_SUFFIX);
}

export function isTestPath(path: string): boolean {
  return path.endsWith(CANONICAL_TEST_SUFFIX);
}

export function isLayoutPath(path: string): boolean {
  return path.endsWith(CANONICAL_LAYOUT_SUFFIX);
}

export function isStationeersProgramPath(path: string): boolean {
  return path.endsWith(".ic10") || path.endsWith(".lua");
}

export function shouldWarnForLegacyLuaExtension(extensionIds: readonly string[]): boolean {
  return extensionIds.includes("OrbitalFoundryModdingCrew.stationeers-lua");
}

export function isCanonicalSimulationPath(path: string): boolean {
  return path.endsWith(CANONICAL_SIM_SUFFIX);
}

export function isCanonicalTestPath(path: string): boolean {
  return path.endsWith(CANONICAL_TEST_SUFFIX);
}

export function scenarioLayoutFilename(scenarioFilename: string): string {
  if (scenarioFilename.endsWith(CANONICAL_SIM_SUFFIX)) {
    return `${scenarioFilename.slice(0, -CANONICAL_SIM_SUFFIX.length)}${CANONICAL_LAYOUT_SUFFIX}`;
  }
  return `${scenarioFilename}${CANONICAL_LAYOUT_SUFFIX}`;
}

export function defaultScenarioFilename(base = "simulation"): string {
  return `${base}${CANONICAL_SIM_SUFFIX}`;
}

export function defaultTestFilename(base = "scenario"): string {
  return `${base}${CANONICAL_TEST_SUFFIX}`;
}
