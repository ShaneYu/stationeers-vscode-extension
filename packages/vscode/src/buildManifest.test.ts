const assert: typeof import("node:assert/strict") = require("node:assert/strict");
const fs: typeof import("node:fs") = require("node:fs");
const path: typeof import("node:path") = require("node:path");
const { test }: typeof import("node:test") = require("node:test");

const manifest = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "..", "package.json"), "utf8"),
) as {
  contributes: {
    commands: { command: string }[];
    configuration: {
      properties: Record<string, { readonly default?: unknown }>;
    };
    problemMatchers: { name: string }[];
  };
};

test("contributes every deployment build surface", () => {
  const commands = new Set(
    manifest.contributes.commands.map((command) => command.command),
  );
  assert(commands.has("ic10.buildForGame"));
  assert(commands.has("ic10.copyDeployableCode"));
  assert(commands.has("ic10.openBuiltCode"));
  assert(
    "ic10.build.optimization" in
      manifest.contributes.configuration.properties,
  );
  assert(
    "ic10.build.outputDirectory" in
      manifest.contributes.configuration.properties,
  );
  assert.equal(
    manifest.contributes.configuration.properties[
      "ic10.build.outputDirectory"
    ].default,
    "build",
  );
  assert(
    manifest.contributes.problemMatchers.some(
      (matcher) => matcher.name === "ic10-build",
    ),
  );
});
