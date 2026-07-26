const assert: typeof import("node:assert/strict") = require("node:assert/strict");
const { test }: typeof import("node:test") = require("node:test");
const fs: typeof import("node:fs") = require("node:fs");
const path: typeof import("node:path") = require("node:path");

test("topology overlays use one snapshot and event-driven trace batches without polling", () => {
  const source = fs.readFileSync(
    path.resolve(__dirname, "environmentDebugOverlay.ts"),
    "utf8",
  );
  assert(source.includes('"ic10/getTopologyState"'));
  assert(source.includes('event.event === "ic10/traceBatch"'));
  assert.equal((source.match(/customRequest\(/g) ?? []).length, 1);
  assert(!source.includes("setInterval"));
  assert(!source.includes("setTimeout"));
});
