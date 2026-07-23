import { spawnSync } from "node:child_process";
import { readFile, writeFile } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const files = {
  rootPackage: path.join(repositoryRoot, "package.json"),
  extensionPackage: path.join(
    repositoryRoot,
    "packages",
    "vscode",
    "package.json",
  ),
  packageLock: path.join(repositoryRoot, "package-lock.json"),
  cargoManifest: path.join(repositoryRoot, "Cargo.toml"),
  cargoLock: path.join(repositoryRoot, "Cargo.lock"),
  changelog: path.join(
    repositoryRoot,
    "packages",
    "vscode",
    "CHANGELOG.md",
  ),
};
const versionPattern = /^(\d+)\.(\d+)\.(\d+)$/;

function parseVersion(value, label) {
  const match = versionPattern.exec(value);
  if (!match) {
    throw new Error(`${label} must use major.minor.patch format; received ${value}`);
  }

  return match.slice(1).map(Number);
}

function resolveTargetVersion(currentVersion, requestedVersion) {
  const [major, minor, patch] = parseVersion(currentVersion, "Current version");

  switch (requestedVersion) {
    case "major":
      return `${major + 1}.0.0`;
    case "minor":
      return `${major}.${minor + 1}.0`;
    case "patch":
      return `${major}.${minor}.${patch + 1}`;
    default:
      parseVersion(requestedVersion, "Requested version");
      return requestedVersion;
  }
}

function compareVersions(left, right) {
  const leftParts = parseVersion(left, "Version");
  const rightParts = parseVersion(right, "Version");

  for (let index = 0; index < leftParts.length; index += 1) {
    if (leftParts[index] !== rightParts[index]) {
      return leftParts[index] - rightParts[index];
    }
  }

  return 0;
}

function replaceCargoWorkspaceVersion(manifest, currentVersion, targetVersion) {
  const pattern =
    /(\[workspace\.package\][\s\S]*?\nversion = ")([^"]+)(")/;
  const match = pattern.exec(manifest);

  if (!match) {
    throw new Error("Cargo.toml has no [workspace.package] version");
  }
  if (match[2] !== currentVersion) {
    throw new Error(
      `Cargo workspace version ${match[2]} != extension ${currentVersion}`,
    );
  }

  return manifest.replace(pattern, `$1${targetVersion}$3`);
}

function updateCargoLock(
  cargoLock,
  workspacePackageNames,
  currentVersion,
  targetVersion,
) {
  const updatedPackages = new Set();
  const blocks = cargoLock.split(/(?=^\[\[package\]\]\r?$)/m);
  const updatedBlocks = blocks.map((block) => {
    const name = block.match(/^name = "([^"]+)"\r?$/m)?.[1];

    if (!name || !workspacePackageNames.has(name)) {
      return block;
    }

    const version = block.match(/^version = "([^"]+)"\r?$/m)?.[1];
    if (version !== currentVersion) {
      throw new Error(
        `Cargo.lock package ${name} version ${version ?? "missing"} != ${currentVersion}`,
      );
    }
    if (updatedPackages.has(name)) {
      throw new Error(`Cargo.lock contains multiple workspace packages named ${name}`);
    }

    updatedPackages.add(name);
    return block.replace(
      /^version = "([^"]+)"\r?$/m,
      `version = "${targetVersion}"`,
    );
  });

  const missingPackages = [...workspacePackageNames].filter(
    (name) => !updatedPackages.has(name),
  );
  if (missingPackages.length > 0) {
    throw new Error(
      `Cargo.lock is missing workspace packages: ${missingPackages.join(", ")}`,
    );
  }

  return updatedBlocks.join("");
}

function updateChangelog(changelog, currentVersion, targetVersion) {
  if (changelog.includes(`## [${targetVersion}]`)) {
    throw new Error(`CHANGELOG.md already has a ${targetVersion} section`);
  }

  const endOfLine = changelog.includes("\r\n") ? "\r\n" : "\n";
  const unreleasedSection =
    /## \[Unreleased\]\r?\n(?:\r?\n)?([\s\S]*?)(?=^## \[)/m.exec(changelog);
  if (!unreleasedSection) {
    throw new Error("CHANGELOG.md has no [Unreleased] section");
  }

  const releaseDate = new Date().toISOString().slice(0, 10);
  let updated = changelog.replace(
    /## \[Unreleased\]\r?\n(?:\r?\n)?/,
    `## [Unreleased]${endOfLine}${endOfLine}## [${targetVersion}] - ${releaseDate}${endOfLine}${endOfLine}`,
  );

  const unreleasedLinkPattern =
    /^\[Unreleased\]: (.+\/compare\/)v(\d+\.\d+\.\d+)\.\.\.HEAD\r?$/m;
  const unreleasedLink = unreleasedLinkPattern.exec(updated);
  if (!unreleasedLink) {
    throw new Error("CHANGELOG.md has no supported [Unreleased] comparison link");
  }
  if (unreleasedLink[2] !== currentVersion) {
    throw new Error(
      `CHANGELOG.md compares from ${unreleasedLink[2]} instead of ${currentVersion}`,
    );
  }

  updated = updated.replace(
    unreleasedLinkPattern,
    [
      `[Unreleased]: ${unreleasedLink[1]}v${targetVersion}...HEAD`,
      `[${targetVersion}]: ${unreleasedLink[1]}v${currentVersion}...v${targetVersion}`,
    ].join(endOfLine),
  );

  return {
    content: updated,
    hadUnreleasedNotes: unreleasedSection[1].trim().length > 0,
  };
}

function readCargoWorkspacePackageNames(currentVersion) {
  const result = spawnSync(
    "cargo",
    ["metadata", "--format-version", "1", "--no-deps", "--locked"],
    {
      cwd: repositoryRoot,
      encoding: "utf8",
    },
  );

  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(
      `cargo metadata failed:\n${result.stderr || result.stdout}`.trim(),
    );
  }

  const metadata = JSON.parse(result.stdout);
  const workspaceMembers = new Set(metadata.workspace_members);
  const workspacePackages = metadata.packages.filter((entry) =>
    workspaceMembers.has(entry.id),
  );

  if (workspacePackages.length === 0) {
    throw new Error("Cargo workspace has no packages");
  }
  for (const entry of workspacePackages) {
    if (entry.version !== currentVersion) {
      throw new Error(
        `Cargo package ${entry.name} version ${entry.version} != ${currentVersion}`,
      );
    }
  }

  return new Set(workspacePackages.map((entry) => entry.name));
}

async function main() {
  const requestedVersion = process.argv[2];
  if (!requestedVersion || process.argv.length > 3) {
    throw new Error(
      "Usage: npm run release:bump -- <patch|minor|major|major.minor.patch>",
    );
  }

  const originals = new Map(
    await Promise.all(
      Object.values(files).map(async (file) => [file, await readFile(file, "utf8")]),
    ),
  );
  const rootPackage = JSON.parse(originals.get(files.rootPackage));
  const extensionPackage = JSON.parse(originals.get(files.extensionPackage));
  const packageLock = JSON.parse(originals.get(files.packageLock));
  const currentVersion = extensionPackage.version;
  const targetVersion = resolveTargetVersion(currentVersion, requestedVersion);

  const manifestVersions = [
    ["root package", rootPackage.version],
    ["package-lock root", packageLock.version],
    ["package-lock root workspace", packageLock.packages?.[""]?.version],
    [
      "package-lock extension workspace",
      packageLock.packages?.["packages/vscode"]?.version,
    ],
  ];
  const inconsistentVersion = manifestVersions.find(
    ([, version]) => version !== currentVersion,
  );
  if (inconsistentVersion) {
    throw new Error(
      `${inconsistentVersion[0]} version ${inconsistentVersion[1] ?? "missing"} != extension ${currentVersion}`,
    );
  }
  if (compareVersions(targetVersion, currentVersion) <= 0) {
    throw new Error(
      `Target version ${targetVersion} must be greater than ${currentVersion}`,
    );
  }

  const workspacePackageNames =
    readCargoWorkspacePackageNames(currentVersion);
  const updatedCargoManifest = replaceCargoWorkspaceVersion(
    originals.get(files.cargoManifest),
    currentVersion,
    targetVersion,
  );
  const updatedCargoLock = updateCargoLock(
    originals.get(files.cargoLock),
    workspacePackageNames,
    currentVersion,
    targetVersion,
  );
  const updatedChangelog = updateChangelog(
    originals.get(files.changelog),
    currentVersion,
    targetVersion,
  );

  rootPackage.version = targetVersion;
  extensionPackage.version = targetVersion;
  packageLock.version = targetVersion;
  packageLock.packages[""].version = targetVersion;
  packageLock.packages["packages/vscode"].version = targetVersion;

  const updates = new Map([
    [files.rootPackage, `${JSON.stringify(rootPackage, null, 2)}\n`],
    [
      files.extensionPackage,
      `${JSON.stringify(extensionPackage, null, 2)}\n`,
    ],
    [files.packageLock, `${JSON.stringify(packageLock, null, 2)}\n`],
    [files.cargoManifest, updatedCargoManifest],
    [files.cargoLock, updatedCargoLock],
    [files.changelog, updatedChangelog.content],
  ]);

  try {
    await Promise.all(
      [...updates].map(([file, content]) => writeFile(file, content, "utf8")),
    );

    const verification = spawnSync(
      process.execPath,
      [
        path.join(repositoryRoot, "tools", "verify-release.mjs"),
        `v${targetVersion}`,
      ],
      {
        cwd: repositoryRoot,
        stdio: "inherit",
      },
    );
    if (verification.error) {
      throw verification.error;
    }
    if (verification.status !== 0) {
      throw new Error("Release metadata verification failed");
    }
  } catch (error) {
    await Promise.all(
      [...originals].map(([file, content]) => writeFile(file, content, "utf8")),
    );
    throw error;
  }

  console.log(`Bumped release metadata from ${currentVersion} to ${targetVersion}.`);
  if (!updatedChangelog.hadUnreleasedNotes) {
    console.warn(
      `The ${targetVersion} changelog section is empty; add release notes before tagging.`,
    );
  }
  console.log(`Review the diff, then run npm run release:check -- v${targetVersion}.`);
}

main().catch((error) => {
  console.error(`Version bump failed: ${error.message}`);
  process.exitCode = 1;
});
