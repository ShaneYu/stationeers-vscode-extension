import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";
import { resolveBuildDirectory } from "./buildPath.ts";

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
