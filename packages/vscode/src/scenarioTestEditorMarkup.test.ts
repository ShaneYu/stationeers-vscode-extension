import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const source = fs.readFileSync(
  path.resolve(process.cwd(), "src", "scenarioTestEditor.ts"),
  "utf8",
);
const manifest = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "package.json"), "utf8"),
) as {
  activationEvents: string[];
  contributes: {
    commands: { command: string }[];
    customEditors: {
      viewType: string;
      priority: string;
      selector: { filenamePattern: string }[];
    }[];
  };
};

test("registers the scenario-test visual editor as the default", () => {
  const editor = manifest.contributes.customEditors.find(
    (candidate) => candidate.viewType === "ic10.scenarioTest",
  );

  assert(editor);
  assert.equal(editor.priority, "default");
  assert.deepEqual(editor.selector, [
    { filenamePattern: "*.ictest" },
  ]);
  assert(manifest.activationEvents.includes("onCustomEditor:ic10.scenarioTest"));
  assert(
    manifest.contributes.commands.some(
      (command) => command.command === "ic10.createScenarioTest",
    ),
  );
});

test("keeps guarded visual authoring and JSON escape hatches", () => {
  for (const marker of [
    "validateScenarioTestFixture(fixture, document.uri.fsPath)",
    "workspace.applyEdit(edit)",
    "document.save()",
    'case "validate"',
    'case "runCase"',
    'case "runAll"',
    'case "openJson"',
    'id="validateNow"',
    'id="runAll"',
    "data-run-case",
    "targetSuggestions",
    'data-suggestions="valueSuggestions"',
    'id="suggestionPopup"',
    "scenarioIntelligence(",
    'id="addCase"',
    'id="addAssertion"',
    'id="addTimeline"',
    'id="addParameter"',
    'id="addDriver"',
    "data-rule-target",
    "data-rule-actions",
    'id="expectErrorEnabled"',
    'data-add-pair="snapshot"',
  ]) {
    assert(source.includes(marker), `missing ${marker}`);
  }
});

test("uses the full editor width and wraps long case names", () => {
  assert(source.includes("grid-template-columns: 310px minmax(480px, 1fr)"));
  assert(source.includes("#app { display: flex; flex: 1 1 auto; flex-direction: column; width: 100%;"));
  assert(source.includes(".layout { display: grid; flex: 1 1 auto; width: 100%;"));
  assert(source.includes(".sidebar { min-height: 0;") && source.includes("overflow-y: auto;"));
  assert(!source.includes(".sidebar { scrollbar-gutter: stable;"));
  assert(source.includes(".case-select { grid-row: 1 / 3; min-width: 0;"));
  assert(source.includes(".case-ticks { justify-self: end;"));
  assert(source.includes(".section-head { display: flex;"));
  assert(!source.includes("max-width: 980px"));
});

test("runs cases from Test Explorer-style sidebar controls", () => {
  assert(source.includes("runButtonContent"));
  assert(source.includes('class="spinner"'));
  assert(source.includes("status: 'queued'"));
  assert(
    source.includes(
      'class="case-tools"><span class="case-result" data-case-result=',
    ),
  );
  assert(!source.includes('id="runState"'));
});

test("anchors guarded suggestions to their input instead of using Chromium datalists", () => {
  assert(source.includes(".suggestion-popup { position: fixed;"));
  assert(source.includes("input.getBoundingClientRect()"));
  assert(source.includes("bounds.bottom + 2"));
  assert(!source.includes("<datalist"));
});

test("keeps the embedded test-editor script syntactically valid", () => {
  const marker = '<script nonce="${nonce}">';
  const start = source.indexOf(marker);
  const end = source.indexOf("</script>", start);
  assert(start >= 0 && end > start);
  const raw = source.slice(start + marker.length, end);
  const cooked = Function(`return \`${raw}\`;`)() as string;
  assert.doesNotThrow(() => new Function(cooked));
});
