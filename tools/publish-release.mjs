import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import * as path from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const releaseFiles = [
  "package.json",
  "package-lock.json",
  "Cargo.toml",
  "Cargo.lock",
  "packages/vscode/package.json",
  "packages/vscode/CHANGELOG.md",
  "mods/StationeersToolkit/src/StationeersToolkit.csproj",
  "mods/StationeersToolkit/About/About.xml",
];
const releaseFileSet = new Set(releaseFiles);
const extensionManifest = path.join(
  repositoryRoot,
  "packages",
  "vscode",
  "package.json",
);

function run(command, args, options = {}) {
  const capture = options.capture ?? false;
  const acceptedStatuses = options.acceptedStatuses ?? [0];
  const result = spawnSync(command, args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    stdio: capture ? "pipe" : "inherit",
  });

  if (result.error) {
    throw result.error;
  }
  if (!acceptedStatuses.includes(result.status)) {
    const details = [result.stderr, result.stdout]
      .filter(Boolean)
      .join("\n")
      .trim();
    throw new Error(
      `${command} ${args.join(" ")} failed with exit code ${result.status}${
        details ? `:\n${details}` : ""
      }`,
    );
  }
  return result;
}

function git(args, options = {}) {
  return run("git", args, options);
}

function gitOutput(args) {
  return git(args, { capture: true }).stdout.trim();
}

function nullSeparatedGitPaths(args) {
  const output = git(args, { capture: true }).stdout;
  return output.split("\0").filter(Boolean);
}

function changedPaths() {
  return new Set([
    ...nullSeparatedGitPaths(["diff", "--name-only", "-z"]),
    ...nullSeparatedGitPaths(["diff", "--cached", "--name-only", "-z"]),
    ...nullSeparatedGitPaths([
      "ls-files",
      "--others",
      "--exclude-standard",
      "-z",
    ]),
  ]);
}

function assertOnlyReleaseFiles(paths, context) {
  const unexpected = [...paths].filter((file) => !releaseFileSet.has(file));
  if (unexpected.length > 0) {
    throw new Error(
      `${context} contains changes outside the release metadata:\n- ${unexpected.join(
        "\n- ",
      )}`,
    );
  }
}

function assertPublishableCommitRange(branch) {
  const aheadCount = Number(
    gitOutput(["rev-list", "--count", `origin/${branch}..HEAD`]),
  );
  if (aheadCount > 1) {
    throw new Error(
      `Local ${branch} is ${aheadCount} commits ahead of origin/${branch}. ` +
        "Publish those changes normally before preparing the release.",
    );
  }
  if (aheadCount === 1) {
    const committedPaths = new Set(
      nullSeparatedGitPaths([
        "diff",
        "--name-only",
        "-z",
        `origin/${branch}..HEAD`,
      ]),
    );
    assertOnlyReleaseFiles(committedPaths, "The unpublished commit");
  }
}

function currentBranch() {
  return gitOutput(["branch", "--show-current"]);
}

function assertReleaseBranch(branch) {
  if (branch !== "main" && branch !== "experimental") {
    throw new Error("release:publish must run from main or experimental");
  }
}

function releaseTag(version, branch) {
  return branch === "experimental" ? `v${version}-prerelease` : `v${version}`;
}

function localTagExists(tag) {
  const result = git(
    ["show-ref", "--verify", "--quiet", `refs/tags/${tag}`],
    {
      capture: true,
      acceptedStatuses: [0, 1],
    },
  );
  return result.status === 0;
}

function remoteTagExists(tag) {
  const result = git(
    ["ls-remote", "--exit-code", "--tags", "origin", `refs/tags/${tag}`],
    {
      capture: true,
      acceptedStatuses: [0, 2],
    },
  );
  return result.status === 0;
}

function isAncestor(ancestor, descendant) {
  const result = git(
    ["merge-base", "--is-ancestor", ancestor, descendant],
    {
      capture: true,
      acceptedStatuses: [0, 1],
    },
  );
  return result.status === 0;
}

async function currentVersion() {
  const manifest = JSON.parse(await readFile(extensionManifest, "utf8"));
  return manifest.version;
}

function verifyRelease(version, tag = `v${version}`) {
  run(process.execPath, [
    path.join(repositoryRoot, "tools", "verify-release.mjs"),
    tag,
  ]);
}

async function confirmPublication(version, tag) {
  if (!process.stdin.isTTY || !process.stdout.isTTY) {
    throw new Error(
      "release:publish requires an interactive terminal for final confirmation",
    );
  }

  console.log(`
Release ${tag} is prepared but has not been published.

Before continuing, use another terminal to:

  1. Review the release metadata with git diff and git diff --cached.
  2. Run npm ci.
  3. Run npm run release:check -- ${tag}.
  4. Run npm run check and npm test.
  5. Run npm run package:extension.
  6. Validate the VSIX with tools/verify_vsix.py.
  7. Sideload the VSIX and manually exercise the extension.
  8. Confirm the marketplace environment and third-party-content gate.

Answering no leaves the prepared version in place. A later
\`npm run release:publish\` will resume it without another bump.
`);
  git(["status", "--short"]);
  git(["diff", "--stat"]);
  git(["diff", "--cached", "--stat"]);

  const prompt = createInterface({
    input: process.stdin,
    output: process.stdout,
  });
  try {
    const answer = (
      await prompt.question(`Publish Stationeers IC10 ${tag}? [y/N] `)
    )
      .trim()
      .toLowerCase();
    return answer === "y" || answer === "yes";
  } finally {
    prompt.close();
  }
}

function verifyLocalTag(tag) {
  const type = gitOutput(["cat-file", "-t", tag]);
  if (type !== "tag") {
    throw new Error(`${tag} is a ${type}, not an annotated tag`);
  }
  const contents = gitOutput(["cat-file", "tag", tag]);
  if (
    !/-----BEGIN (?:PGP SIGNATURE|SSH SIGNATURE|SIGNED MESSAGE)-----/.test(
      contents,
    )
  ) {
    throw new Error(`${tag} is annotated but does not contain a signature`);
  }
  const taggedCommit = gitOutput(["rev-list", "-n", "1", tag]);
  const head = gitOutput(["rev-parse", "HEAD"]);
  if (taggedCommit !== head) {
    throw new Error(
      `${tag} points to ${taggedCommit}, but the current branch is at ${head}`,
    );
  }
}

function usage() {
  return [
    "Usage:",
    "  npm run release:publish -- <patch|minor|major|major.minor.patch>",
    "  npm run release:publish",
    "",
    "The version argument is required for a new release and optional when",
    "resuming an already-bumped version that has no remote tag.",
  ].join("\n");
}

async function main() {
  const rawRequestedVersion = process.argv[2];
  if (rawRequestedVersion === "--help" || rawRequestedVersion === "-h") {
    console.log(usage());
    return;
  }
  if (process.argv.length > 3) {
    throw new Error(usage());
  }
  const requestedVersion = rawRequestedVersion?.replace(/^v(?=\d)/, "");
  const branch = currentBranch();
  assertReleaseBranch(branch);

  const initialChanges = changedPaths();
  if (initialChanges.size === 0) {
    git(["pull", "--ff-only", "origin", branch]);
  } else {
    assertOnlyReleaseFiles(initialChanges, "The working tree");
    git(["fetch", "origin", branch]);
    if (
      gitOutput(["rev-parse", "HEAD"]) !==
      gitOutput(["rev-parse", `origin/${branch}`])
    ) {
      throw new Error(
        `Local ${branch} differs from origin/${branch}. Reconcile it before resuming the release.`,
      );
    }
  }

  let version = await currentVersion();
  const currentTag = releaseTag(version, branch);
  const currentVersionIsPublished =
    remoteTagExists(currentTag) ||
    (branch === "experimental" && remoteTagExists(`v${version}`));

  if (currentVersionIsPublished) {
    if (changedPaths().size > 0) {
      throw new Error(
        `${currentTag} is already published, but the working tree is not clean`,
      );
    }
    if (
      gitOutput(["rev-parse", "HEAD"]) !==
      gitOutput(["rev-parse", `origin/${branch}`])
    ) {
      throw new Error(
        `Local ${branch} must exactly match origin/${branch} before preparing a new release.`,
      );
    }
    if (!requestedVersion) {
      throw new Error(
        `The current version ${currentTag} is already published.\n${usage()}`,
      );
    }

    run(process.execPath, [
      path.join(repositoryRoot, "tools", "bump-version.mjs"),
      requestedVersion,
    ]);
    version = await currentVersion();
    console.log(`Prepared a new release at ${releaseTag(version, branch)}.`);
  } else {
    console.log(
      `No remote ${currentTag} tag exists; resuming the prepared ${currentTag} release without another version bump.`,
    );
  }

  const tag = releaseTag(version, branch);
  if (remoteTagExists(tag)) {
    throw new Error(`${tag} already exists on origin`);
  }
  const preparedChanges = changedPaths();
  assertOnlyReleaseFiles(preparedChanges, "The prepared release");
  assertPublishableCommitRange(branch);
  verifyRelease(version, tag);

  if (!(await confirmPublication(version, tag))) {
    console.log(
      `Release ${tag} was not published. Its prepared metadata remains available for a later resume.`,
    );
    return;
  }

  if (currentBranch() !== branch) {
    throw new Error("The release branch changed while awaiting confirmation");
  }
  git(["fetch", "origin", branch]);
  if (!isAncestor(`origin/${branch}`, "HEAD")) {
    throw new Error(
      `origin/${branch} changed or diverged while checks were running. Reconcile it before publishing.`,
    );
  }
  assertPublishableCommitRange(branch);
  if (remoteTagExists(tag)) {
    throw new Error(`${tag} appeared on origin while checks were running`);
  }

  verifyRelease(version, tag);
  const finalChanges = changedPaths();
  assertOnlyReleaseFiles(finalChanges, "The final release state");
  if (finalChanges.size > 0) {
    git(["add", "--", ...releaseFiles]);
    git(["commit", "-m", `chore(release): ${tag}`]);
  }
  if (changedPaths().size > 0) {
    throw new Error("The working tree is not clean after the release commit");
  }
  assertPublishableCommitRange(branch);

  if (localTagExists(tag)) {
    console.log(`Reusing the existing local ${tag} tag.`);
  } else {
    git(["tag", "-s", tag, "-m", `Stationeers IC10 ${tag}`]);
  }
  verifyLocalTag(tag);

  git(["push", "--atomic", "origin", branch, `refs/tags/${tag}`]);
  console.log(
    `Published ${tag}. The protected release workflow will build, verify, and distribute the extension.`,
  );
}

main().catch((error) => {
  console.error(`Release publication failed: ${error.message}`);
  process.exitCode = 1;
});
