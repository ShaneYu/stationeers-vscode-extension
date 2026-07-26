export function validateTemplateRelativePaths(
  relativeFiles: readonly string[],
): readonly string[] {
  return relativeFiles.map((relativePath) => {
    const normalized = relativePath.replaceAll("\\", "/");
    if (
      normalized.startsWith("/") ||
      /^[a-z]:\//i.test(normalized) ||
      normalized.split("/").some((segment) => segment === "..")
    ) {
      throw new Error(`Template path escapes its root: ${relativePath}`);
    }
    return normalized;
  });
}
