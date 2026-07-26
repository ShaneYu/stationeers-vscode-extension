import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

test("topology overlays use one snapshot and event-driven trace batches without polling", () => {
  const source = fs.readFileSync(
    path.resolve(process.cwd(), "src", "environmentDebugOverlay.ts"),
    "utf8",
  );
  assert(source.includes('"ic10/getTopologyState"'));
  assert(source.includes('event.event === "ic10/traceBatch"'));
  assert.equal((source.match(/customRequest\(/g) ?? []).length, 1);
  assert(!source.includes("setInterval"));
  assert(!source.includes("setTimeout"));
});
