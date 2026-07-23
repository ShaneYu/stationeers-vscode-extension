import { readFile } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const rootPackage = JSON.parse(
  await readFile(path.join(repositoryRoot, "package.json"), "utf8"),
);
const extensionPackage = JSON.parse(
  await readFile(
    path.join(repositoryRoot, "packages", "vscode", "package.json"),
    "utf8",
  ),
);
const cargoManifest = await readFile(
  path.join(repositoryRoot, "Cargo.toml"),
  "utf8",
);
const changelog = await readFile(
  path.join(repositoryRoot, "packages", "vscode", "CHANGELOG.md"),
  "utf8",
);
const rootLicense = await readFile(
  path.join(repositoryRoot, "LICENSE"),
  "utf8",
);
const extensionLicense = await readFile(
  path.join(repositoryRoot, "packages", "vscode", "LICENSE"),
  "utf8",
);
const rootNotices = await readFile(
  path.join(repositoryRoot, "THIRD_PARTY_NOTICES.md"),
  "utf8",
);
const extensionNotices = await readFile(
  path.join(
    repositoryRoot,
    "packages",
    "vscode",
    "THIRD_PARTY_NOTICES.md",
  ),
  "utf8",
);
const cargoVersion = cargoManifest.match(
  /\[workspace\.package\][\s\S]*?\nversion = "([^"]+)"/,
)?.[1];
const expectedVersion = extensionPackage.version;
const suppliedTag =
  process.argv[2] ||
  (process.env.GITHUB_REF_TYPE === "tag"
    ? process.env.GITHUB_REF_NAME || ""
    : "");
const tagVersion = suppliedTag.replace(/^v/, "");
const failures = [];

if (rootPackage.version !== expectedVersion) {
  failures.push(
    `root package version ${rootPackage.version} != extension ${expectedVersion}`,
  );
}
if (cargoVersion !== expectedVersion) {
  failures.push(
    `Cargo workspace version ${cargoVersion ?? "missing"} != extension ${expectedVersion}`,
  );
}
if (!changelog.includes(`## [${expectedVersion}]`)) {
  failures.push(`CHANGELOG.md has no ## [${expectedVersion}] section`);
}
if (rootLicense !== extensionLicense) {
  failures.push("root and extension LICENSE files differ");
}
if (rootNotices !== extensionNotices) {
  failures.push("root and extension THIRD_PARTY_NOTICES.md files differ");
}
if (suppliedTag && tagVersion !== expectedVersion) {
  failures.push(`tag ${suppliedTag} != extension version ${expectedVersion}`);
}

if (failures.length > 0) {
  throw new Error(`Release metadata is inconsistent:\n- ${failures.join("\n- ")}`);
}

console.log(`Release metadata is consistent at ${expectedVersion}.`);
