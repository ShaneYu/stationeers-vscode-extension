import { chmod, copyFile, mkdir, rm } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const packageDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const repositoryRoot = path.resolve(packageDirectory, "..", "..");
const destinationDirectory = path.join(
  packageDirectory,
  "server",
  `${process.platform}-${process.arch}`,
);

await rm(path.join(packageDirectory, "server"), {
  force: true,
  recursive: true,
});
await rm(path.join(packageDirectory, "reference"), {
  force: true,
  recursive: true,
});
await mkdir(destinationDirectory, { recursive: true });
for (const binary of ["ic10-lsp", "ic10-dap", "ic10"]) {
  const executableName =
    process.platform === "win32" ? `${binary}.exe` : binary;
  const source = path.join(
    repositoryRoot,
    "target",
    "release",
    executableName,
  );
  const destination = path.join(destinationDirectory, executableName);
  await copyFile(source, destination);
  if (process.platform !== "win32") {
    await chmod(destination, 0o755);
  }
  console.log(`Staged ${destination}`);
}

const referenceDirectory = path.join(packageDirectory, "reference");
await mkdir(referenceDirectory, { recursive: true });
await copyFile(
  path.join(repositoryRoot, "data", "generated", "devices.json"),
  path.join(referenceDirectory, "devices.json"),
);
console.log(`Staged ${path.join(referenceDirectory, "devices.json")}`);
await copyFile(
  path.join(repositoryRoot, "data", "generated", "instructions.json"),
  path.join(referenceDirectory, "instructions.json"),
);
console.log(`Staged ${path.join(referenceDirectory, "instructions.json")}`);
await copyFile(
  path.join(repositoryRoot, "data", "generated", "resources.json"),
  path.join(referenceDirectory, "resources.json"),
);
console.log(`Staged ${path.join(referenceDirectory, "resources.json")}`);
