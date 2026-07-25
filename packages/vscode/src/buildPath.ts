import * as path from "node:path";

export function resolveBuildDirectory(
  sourcePath: string,
  configuredDirectory = "build",
): string {
  return path.resolve(path.dirname(sourcePath), configuredDirectory);
}
