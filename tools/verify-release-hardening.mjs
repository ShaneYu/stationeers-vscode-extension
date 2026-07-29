import { mkdtemp, readFile, rm, writeFile, readdir } from "node:fs/promises";
import * as os from "node:os";
import * as path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

export const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const sensitiveText = [
  /\bBearer\s+[A-Za-z0-9._~-]{12,}/i,
  /\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b/,
  /\b[A-Za-z]:[\\/]/,
  /(?:^|[\s"'])(?:\\\\|\/)(?:Users|home|private|var|tmp|workspace)[\\/]/i,
  /\b(?:player|user|steam|save)(?:Name|Id|Identifier)\s*[:=]\s*["'][^"']+/i,
];
const forbiddenSourceKeys = /(?:raw|source)(?:Text|Code|Log)|player(?:Name|Id)|steam(?:Id|Name)/i;

function collectStrings(value, key = "", output = []) {
  if (typeof value === "string") output.push({ key, value });
  else if (Array.isArray(value)) value.forEach((item) => collectStrings(item, key, output));
  else if (value && typeof value === "object") {
    for (const [childKey, childValue] of Object.entries(value)) collectStrings(childValue, childKey, output);
  }
  return output;
}

export function assertSanitizedEvidence(fileName, evidence, { template = false } = {}) {
  const failures = [];
  for (const { key, value } of collectStrings(evidence)) {
    if (sensitiveText.some((pattern) => pattern.test(value))) failures.push(`${key || "value"} contains a sensitive-looking value`);
    if (!template && forbiddenSourceKeys.test(key) && value && !/Removed|REPLACE-ME|absent/i.test(value)) failures.push(`${key} contains retained identity/source/log content`);
  }
  if (!template && evidence.sanitization) {
    for (const [key, value] of Object.entries(evidence.sanitization)) {
      if (typeof value !== "boolean" || value !== true) failures.push(`sanitization.${key} must be true`);
    }
  }
  if (evidence.realGame?.acceptance?.status && !["not-run", "blocked"].includes(evidence.realGame.acceptance.status)) {
    failures.push("real-game acceptance must remain not-run or blocked until release evidence is captured");
  }
  if (failures.length) throw new Error(`${fileName}:\n- ${failures.join("\n- ")}`);
}

export async function verifyEvidenceDirectory(root = repositoryRoot) {
  const directory = path.join(root, "docs", "live-integration", "evidence");
  const names = (await readdir(directory)).filter((name) => name.endsWith(".json")).sort();
  for (const name of names) {
    const evidence = JSON.parse(await readFile(path.join(directory, name), "utf8"));
    assertSanitizedEvidence(name, evidence, { template: name.endsWith(".template.json") });
  }
  return names;
}

export async function verifyLockfile(root = repositoryRoot) {
  const manifest = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
  const lock = JSON.parse(await readFile(path.join(root, "package-lock.json"), "utf8"));
  const extension = JSON.parse(await readFile(path.join(root, "packages", "vscode", "package.json"), "utf8"));
  const failures = [];
  if (lock.lockfileVersion !== 3) failures.push(`package-lock lockfileVersion must be 3, got ${lock.lockfileVersion}`);
  if (lock.name !== manifest.name || lock.version !== manifest.version) failures.push("package-lock root metadata differs from package.json");
  if (lock.packages?.[""]?.version !== manifest.version) failures.push("package-lock root workspace version differs from package.json");
  if (lock.packages?.["packages/vscode"]?.version !== extension.version) failures.push("package-lock extension workspace version differs from its manifest");
  if (failures.length) throw new Error(failures.join("\n"));
}

export function verifyExtensionManifest(manifest) {
  if (!manifest.files?.length) throw new Error("extension manifest must declare an explicit files allowlist");
  if (!manifest.extensionDependencies?.includes("sumneko.lua")) throw new Error("extension must declare sumneko.lua explicitly");
  if (manifest.extensionDependencies.includes("OrbitalFoundryModdingCrew.stationeers-lua")) throw new Error("extension must not depend on StationeersLua");
  if (!manifest.main || manifest.main.includes("src/")) throw new Error("extension main must point at built output");
}

export async function verifyCleanInstall(root = repositoryRoot) {
  const tempRoot = await mkdtemp(path.join(os.tmpdir(), "stationeers-release-"));
  try {
    await writeFile(path.join(tempRoot, "package.json"), await readFile(path.join(root, "package.json")));
    await writeFile(path.join(tempRoot, "package-lock.json"), await readFile(path.join(root, "package-lock.json")));
    await (await import("node:fs/promises")).mkdir(path.join(tempRoot, "packages", "vscode"), { recursive: true });
    await writeFile(path.join(tempRoot, "packages", "vscode", "package.json"), await readFile(path.join(root, "packages", "vscode", "package.json")));
    const result = spawnSync("npm", ["ci", "--ignore-scripts", "--no-audit", "--no-fund", "--cache", path.join(tempRoot, "npm-cache")], { cwd: tempRoot, stdio: "inherit", shell: process.platform === "win32" });
    if (result.status !== 0) throw new Error(`clean npm ci failed with exit ${result.status}`);
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }
}

async function main() {
  await verifyLockfile();
  verifyExtensionManifest(JSON.parse(await readFile(path.join(repositoryRoot, "packages", "vscode", "package.json"), "utf8")));
  await verifyEvidenceDirectory();
  if (process.argv.includes("--clean-install")) await verifyCleanInstall();
  console.log("Release hardening checks passed; real-game acceptance remains explicit and non-falsifiable.");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
