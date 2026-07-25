const assert = require("node:assert/strict");
const path = require("node:path") as typeof import("node:path");
const test = require("node:test");
const { resolveBuildDirectory } = require("./buildPath.ts") as typeof import("./buildPath");

test("places default builds beside the source program", () => {
  const source = path.resolve(
    "workspace",
    "programs",
    "multi-ic",
    "item-requester.ic10",
  );
  assert.equal(
    resolveBuildDirectory(source),
    path.join(path.dirname(source), "build"),
  );
});

test("resolves configured build directories from the source folder", () => {
  const source = path.resolve("workspace", "programs", "controller.ic10");
  assert.equal(
    resolveBuildDirectory(source, ".generated/ic10"),
    path.join(path.dirname(source), ".generated", "ic10"),
  );
  const absolute = path.resolve("deploy");
  assert.equal(
    resolveBuildDirectory(source, absolute),
    absolute,
  );
});
