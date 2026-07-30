import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import path from "node:path";

import {
  CANONICAL_LAYOUT_SUFFIX,
  CANONICAL_SIM_SUFFIX,
  CANONICAL_TEST_SUFFIX,
  isLayoutPath,
  isSimulationPath,
  isStationeersProgramPath,
  isTestPath,
  scenarioLayoutFilename,
  shouldWarnForLegacyLuaExtension,
} from "./workspaceFormats.ts";

test("recognizes workspace suffixes", () => {
  assert.equal(isSimulationPath(`world${CANONICAL_SIM_SUFFIX}`), true);
  assert.equal(isTestPath(`world${CANONICAL_TEST_SUFFIX}`), true);
  assert.equal(isLayoutPath(`world${CANONICAL_LAYOUT_SUFFIX}`), true);
  assert.equal(isStationeersProgramPath("monitor.lua"), true);
  assert.equal(isStationeersProgramPath("monitor.txt"), false);
});

test("uses the new workspace suffixes", () => {
  assert.equal(
    scenarioLayoutFilename("world.icsim"),
    "world.icsimlayout",
  );
  assert.equal(
    scenarioLayoutFilename("world.icsim"),
    "world.icsimlayout",
  );
});

test("warns only when the legacy StationeersLua extension is present", () => {
  assert.equal(shouldWarnForLegacyLuaExtension([]), false);
  assert.equal(shouldWarnForLegacyLuaExtension(["sumneko.lua"]), false);
  assert.equal(
    shouldWarnForLegacyLuaExtension(["sumneko.lua", "OrbitalFoundryModdingCrew.stationeers-lua"]),
    true,
  );
});

test("packaged templates generate canonical scenario and test files", () => {
  const templatesRoot = path.resolve("templates");
  for (const name of fs.readdirSync(templatesRoot)) {
    const root = path.join(templatesRoot, name);
    if (!fs.statSync(root).isDirectory()) continue;
    const manifest = JSON.parse(
      fs.readFileSync(path.join(root, "manifest.json"), "utf8"),
    ) as { entryFiles: { scenario: string; tests: string } };
    assert.match(manifest.entryFiles.scenario, /\.icsim$/);
    assert.match(manifest.entryFiles.tests, /\.ictest$/);
    assert.equal(
      fs.readdirSync(root).some((file) => /\.(ic10sim|ic10test|stationeerssim|stationeerstest)\.json$/.test(file)),
      false,
      `${name}: packaged templates must not emit obsolete workspace filenames`,
    );
  }
});
