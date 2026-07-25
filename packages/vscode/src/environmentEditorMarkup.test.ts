const assert: typeof import("node:assert/strict") = require("node:assert/strict");
const fs: typeof import("node:fs") = require("node:fs");
const path: typeof import("node:path") = require("node:path");
const { test }: typeof import("node:test") = require("node:test");

const source = fs.readFileSync(
  path.resolve(process.cwd(), "src", "environmentEditor.ts"),
  "utf8",
);

test("uses an anchored native select for IC program paths", () => {
  assert(source.includes('<select id="program"'));
  assert(!source.includes('list="programFiles"'));
  assert(!source.includes('<datalist id="programFiles">'));
  assert(
    source.includes(
      ".input-action { display: grid; grid-template-columns: minmax(0, 1fr) auto auto;",
    ),
  );
});
