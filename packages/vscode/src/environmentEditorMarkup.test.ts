import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const source = fs.readFileSync(
  path.resolve(process.cwd(), "src", "environmentEditor.ts"),
  "utf8",
);

test("uses an anchored native select for IC program paths", () => {
  assert(source.includes("hasRunnableIcProgram"));
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

test("offers an accessible synchronized topology workspace", () => {
  assert(source.includes('id="inspectorTab"'));
  assert(source.includes('id="topologyTab"'));
  assert(source.includes('role="tablist"'));
  assert(source.includes('aria-label="Search topology"'));
  assert(source.includes('aria-label="Filter validation status"'));
  assert(source.includes("function renderTopology()"));
  assert(source.includes("function installTopologyKeyboard()"));
  assert(source.includes("ArrowLeft"));
  assert(source.includes("event.key === 'Escape'"));
  assert(source.includes("@media (forced-colors: active)"));
  assert(source.includes("@media (prefers-reduced-motion: reduce)"));
  assert(source.includes("type: 'saveTopologyLayout'"));
  assert(source.includes("type: 'duplicateTopology'"));
  assert(source.includes("type: 'exportTopology'"));
  assert(source.includes("type: 'importTopology'"));
  assert(source.includes("type: 'topologyDebugAction'"));
  assert(source.includes("modelled"));
  assert(source.includes("recent-write"));
  assert(source.includes("runtime.reads.slice(-128)"));
  assert(source.includes("runtime.writes.slice(-128)"));
  assert(source.includes("topologyScroll.addEventListener('wheel'"));
  assert(source.includes("event.ctrlKey && !event.metaKey"));
  assert(source.includes("function computeEdgePath("));
  assert(source.includes("function updateTopologyEdges("));
  assert(source.includes("function calculateFitZoom()"));
});


test("shows guarded source proposals before a coherent non-overwriting apply", () => {
  assert(source.includes('id="proposalDialog"'));
  assert(source.includes("function renderEnvironmentProposal("));
  assert(source.includes("candidate.confidence"));
  assert(source.includes("candidate.reason"));
  assert(source.includes("device.evidence"));
  assert(source.includes("preview.blockers"));
  assert(source.includes("proposalConfirm"));
  assert(source.includes("requestEnvironmentProposal"));
  assert(source.includes("applyEnvironmentProposal"));
  assert(source.includes("Source proposals never overwrite a populated environment"));
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
