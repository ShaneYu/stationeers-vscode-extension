import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const expected = new Set([
  "solar-tracking",
  "one-door-airlock",
  "two-door-airlock",
  "temperature-pressure-control",
  "filtration",
  "batch-production",
  "vending-chute-handshake",
  "multi-ic-shared-network",
]);
const generated = JSON.parse(
  fs.readFileSync(path.resolve(root, "../data/generated/instructions.json"), "utf8"),
);
const targetVersion = generated.gameVersion;

test("template manifests reference complete self-contained fixtures", () => {
  const directories = fs
    .readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name);
  assert.deepEqual(new Set(directories), expected);

  for (const directory of directories) {
    const base = path.join(root, directory);
    const manifest = JSON.parse(
      fs.readFileSync(path.join(base, "manifest.json"), "utf8"),
    );
    assert.equal(manifest.schemaVersion, 1);
    assert.equal(manifest.id, directory);
    assert.equal(manifest.targetGameVersion, targetVersion);
    assert.ok(manifest.knownDeviations.length > 0);
    assert.ok(fs.existsSync(path.join(base, "README.md")));

    const entries = manifest.entryFiles;
    for (const relative of [
      entries.scenario,
      entries.tests,
      ...entries.programs,
    ]) {
      const absolute = path.resolve(base, relative);
      const contained = path.relative(base, absolute);
      assert.equal(
        contained !== "" &&
          !contained.startsWith(`..${path.sep}`) &&
          contained !== ".." &&
          !path.isAbsolute(contained),
        true,
        `${directory}: path escapes template: ${relative}`,
      );
      assert.ok(fs.existsSync(absolute), `${directory}: missing ${relative}`);
    }
    assert.match(entries.scenario, /\.icsim$/);
    assert.match(entries.tests, /\.ictest$/);
    for (const program of entries.programs) assert.match(program, /\.ic10$/);
    const fixture = JSON.parse(
      fs.readFileSync(path.join(base, entries.tests), "utf8"),
    );
    const scenarioPath = path.resolve(base, fixture.scenario);
    assert.equal(scenarioPath, path.resolve(base, entries.scenario));
    const scenario = JSON.parse(fs.readFileSync(scenarioPath, "utf8"));
    assert.equal(scenario.gameVersion, targetVersion);
    const names = fixture.cases.map((item) => item.name);
    assert.equal(new Set(names).size, names.length, `${directory}: duplicate tests`);
    assert.deepEqual(
      names,
      manifest.tests,
    );
  }
});
