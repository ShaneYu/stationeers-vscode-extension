import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const source = fs.readFileSync(
  path.resolve(process.cwd(), "src", "stateView.ts"),
  "utf8",
);
const manifest = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "package.json"), "utf8"),
) as {
  contributes: {
    configuration: {
      properties: Record<string, { default: unknown }>;
    };
  };
};
const readme = fs.readFileSync(
  path.resolve(process.cwd(), "README.md"),
  "utf8",
);

test("exposes reversible history through the IC10 State view", () => {
  for (const marker of [
    '"ic10/getTrace"',
    '"ic10/navigateHistory"',
    '"ic10/stateDiff"',
    "historyEvent",
    "historyTarget",
    "valueChart",
    "retainedTicks",
    "droppedEvents",
    "detailsRegisters",
    "detailsStack",
    "worldViewType",
    "mode === 'world'",
    "detailsLua",
    "detailsHistory",
    "refreshEpoch",
    "minmax(90px",
    "state.runtimes",
    "invocations",
    "onDidStartDebugSession",
    "knownIc10Session",
  ]) {
    assert.match(source, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});

test("filters the timeline with the current target selector", () => {
  assert.match(source, /const target = targetFilter\.value;/);
  assert.match(
    source,
    /record\.writes\.some\(\(write\) => write\.target === target\)/,
  );
  assert.match(source, /type: 'selectTraceFilter'/);
  assert.match(source, /this\.traceFilter = message\.target \|\| undefined/);
  assert.doesNotMatch(source, /const requestedTarget = message\.traceFilter/);
});

test("renders an accessible SVG chart for retained numeric writes", () => {
  for (const marker of [
    "<svg viewBox=",
    'role="img"',
    "aria-label=",
    "Number.isFinite(point.value)",
    "prefers-reduced-motion: reduce",
    "forced-colors: active",
    "chart-line",
    "chart-point",
  ]) {
    assert.match(source, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});

test("keeps the embedded State view script syntactically valid", () => {
  const marker = '<script nonce="${nonce}">';
  const start = source.indexOf(marker);
  const end = source.indexOf("</script>", start);
  assert(start >= 0 && end > start);
  const raw = source.slice(start + marker.length, end);
  const cooked = Function(`return \`${raw}\`;`)() as string;
  assert.doesNotThrow(() => new Function(cooked));
});

test("documents the contributed history defaults", () => {
  const properties = manifest.contributes.configuration.properties;
  assert.equal(properties["ic10.debug.history.enabled"]?.default, false);
  assert.equal(properties["ic10.debug.history.events"]?.default, 20_000);
  assert.equal(
    properties["ic10.debug.history.checkpointInterval"]?.default,
    10_000,
  );
  assert.equal(properties["ic10.debug.history.memoryMiB"]?.default, 64);
  for (const row of [
    "| `ic10.debug.history.enabled` | `false` |",
    "| `ic10.debug.history.events` | `20000` |",
    "| `ic10.debug.history.checkpointInterval` | `10000` |",
    "| `ic10.debug.history.memoryMiB` | `64` |",
  ]) {
    assert(readme.includes(row), `README is missing ${row}`);
  }
});
