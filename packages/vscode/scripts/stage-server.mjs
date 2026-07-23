import { chmod, copyFile, mkdir } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const packageDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const repositoryRoot = path.resolve(packageDirectory, "..", "..");
const executableName = process.platform === "win32" ? "ic10-lsp.exe" : "ic10-lsp";
const source = path.join(
  repositoryRoot,
  "target",
  "release",
  executableName,
);
const destinationDirectory = path.join(
  packageDirectory,
  "server",
  `${process.platform}-${process.arch}`,
);
const destination = path.join(destinationDirectory, executableName);

await mkdir(destinationDirectory, { recursive: true });
await copyFile(source, destination);
if (process.platform !== "win32") {
  await chmod(destination, 0o755);
}
console.log(`Staged ${destination}`);

