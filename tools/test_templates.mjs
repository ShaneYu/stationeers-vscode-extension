import { readdirSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const root = path.resolve(import.meta.dirname, "..");
const templates = path.join(root, "templates");
const executable = path.join(
  root,
  "target",
  "debug",
  process.platform === "win32" ? "ic10.exe" : "ic10",
);

const fixtures = readdirSync(templates, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .flatMap((entry) => {
    const directory = path.join(templates, entry.name);
    return readdirSync(directory)
      .filter((name) =>
        name.endsWith(".stationeerstest.json") || name.endsWith(".ic10test.json"),
      )
      .map((name) => path.join(directory, name));
  })
  .sort();

const canonicalFixtures = fixtures.filter((fixture) =>
  fixture.endsWith(".stationeerstest.json"),
);
if (canonicalFixtures.length !== 8) {
  throw new Error(`Expected eight canonical template tests, found ${canonicalFixtures.length}.`);
}
for (const fixture of fixtures) {
  const result = spawnSync(executable, ["test", fixture], {
    cwd: root,
    encoding: "utf8",
    stdio: "pipe",
  });
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  if (result.status !== 0) {
    throw new Error(
      `Template test failed: ${path.relative(root, fixture)} (exit ${result.status})`,
    );
  }
}
console.log(`Passed ${canonicalFixtures.length} canonical template fixtures; legacy suffixes are accepted for compatibility.`);
