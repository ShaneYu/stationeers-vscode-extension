const assert: typeof import("node:assert/strict") = require("node:assert/strict");
const fs: typeof import("node:fs") = require("node:fs");
const path: typeof import("node:path") = require("node:path");
const { test }: typeof import("node:test") = require("node:test");

const {
  validateTemplateRelativePaths,
}: typeof import("./environmentTemplateModel") = require("./environmentTemplateModel.ts");

test("normalizes contained template destinations without mutating files", () => {
  assert.deepEqual(
    validateTemplateRelativePaths([
      "manifest.json",
      "src\\controller.ic10",
    ]),
    ["manifest.json", "src/controller.ic10"],
  );
});

test("refuses absolute and parent-traversing template entries", () => {
  assert.throws(
    () => validateTemplateRelativePaths(["../secret"]),
    /escapes/,
  );
  assert.throws(
    () => validateTemplateRelativePaths(["/absolute"]),
    /escapes/,
  );
  assert.throws(
    () => validateTemplateRelativePaths(["C:\\absolute"]),
    /escapes/,
  );
});

test("contributes and packages the guarded template command and topology schemas", () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.resolve(__dirname, "..", "package.json"), "utf8"),
  ) as {
    files: string[];
    contributes: {
      commands: { command: string }[];
      jsonValidation: { fileMatch: string; url: string }[];
    };
  };
  assert(manifest.files.includes("templates/"));
  assert(
    manifest.contributes.commands.some(
      ({ command }) => command === "ic10.createEnvironmentFromTemplate",
    ),
  );
  assert(
    manifest.contributes.jsonValidation.some(
      ({ fileMatch }) => fileMatch === "*.ic10sim.layout.json",
    ),
  );
  assert(
    manifest.contributes.jsonValidation.some(
      ({ fileMatch }) => fileMatch === "*.ic10topology.json",
    ),
  );
  const staging = fs.readFileSync(
    path.resolve(__dirname, "..", "scripts", "stage-server.mjs"),
    "utf8",
  );
  assert(staging.includes('path.join(repositoryRoot, "templates")'));
});
