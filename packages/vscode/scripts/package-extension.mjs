import { createVSIX } from "@vscode/vsce";
import { readFile } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const packageDirectory = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const manifest = JSON.parse(
  await readFile(path.join(packageDirectory, "package.json"), "utf8"),
);
const target = `${process.platform}-${process.arch}`;
const supportedTargets = new Set([
  "darwin-arm64",
  "darwin-x64",
  "linux-arm64",
  "linux-x64",
  "win32-arm64",
  "win32-x64",
]);

if (!supportedTargets.has(target)) {
  throw new Error(
    `Unsupported packaging host ${target}. Use one of: ${[...supportedTargets].join(", ")}`,
  );
}

const packagePath = path.join(
  packageDirectory,
  `${manifest.name}-${manifest.version}@${target}.vsix`,
);

await createVSIX({
  cwd: packageDirectory,
  dependencies: false,
  githubBranch: "main",
  packagePath,
  target,
});

console.log(`Created ${packagePath}`);
