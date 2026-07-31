import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const manifest = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "package.json"), "utf8"),
) as {
  contributes: {
    breakpoints: { language: string }[];
    commands: { command: string }[];
    configuration: {
      properties: Record<string, { readonly default?: unknown }>;
    };
    problemMatchers: { name: string }[];
    debuggers: { type: string; languages?: string[] }[];
  };
};

test("enables simulation breakpoints in IC10 and Lua editors", () => {
  assert.deepEqual(
    manifest.contributes.breakpoints.map((breakpoint) => breakpoint.language),
    ["ic10", "lua"],
  );
  assert.deepEqual(
    manifest.contributes.debuggers.find((debuggerContribution) => debuggerContribution.type === "ic10")?.languages,
    ["ic10", "lua"],
  );
});

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
