import * as path from "node:path";

export function resolveScenarioProgramPath(
  scenario: {
    readonly scheme: string;
    readonly authority: string;
    readonly path: string;
  },
  program: string,
): { scheme: string; authority: string; path: string } {
  return {
    scheme: scenario.scheme,
    authority: scenario.authority,
    path: path.posix.resolve(
      path.posix.dirname(scenario.path),
      program.replaceAll("\\", "/"),
    ),
  };
}
