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

test("lets environment forms and section actions use the inspector width", () => {
  assert(
    source.includes(
      ".form { display: grid; grid-template-columns: minmax(140px, 220px) minmax(220px, 1fr); gap: 7px 12px; align-items: center; width: 100%;",
    ),
  );
  assert(source.includes(".section-actions { display: flex;"));
  assert(!source.includes("max-width: 920px"));
  assert(!source.includes("max-width: 720px"));
});

test("collapses empty slots and exposes the simulation JSON source", () => {
  assert(source.includes('id="openJson"'));
  assert(source.includes('message.type === "openJson"'));
  assert(source.includes('class="slot-section"'));
  assert(source.includes("configured ? ' open' : ''"));
  assert(source.includes("Configured slots open automatically"));
  assert(source.includes("const values = device.slots[slot] || {};"));
});

test("keeps the embedded environment-editor script syntactically valid", () => {
  const marker = '<script nonce="${nonce}">';
  const start = source.indexOf(marker);
  const end = source.indexOf("</script>", start);
  assert(start >= 0 && end > start);
  const raw = source.slice(start + marker.length, end);
  const cooked = Function(`return \`${raw}\`;`)() as string;
  assert.doesNotThrow(() => new Function(cooked));
});
