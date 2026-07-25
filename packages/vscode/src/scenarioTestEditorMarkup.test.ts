const assert: typeof import("node:assert/strict") = require("node:assert/strict");
const fs: typeof import("node:fs") = require("node:fs");
const path: typeof import("node:path") = require("node:path");
const { test }: typeof import("node:test") = require("node:test");

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
    { filenamePattern: "*.ic10test.json" },
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
    "validateScenarioTestFixture(fixture)",
    "workspace.applyEdit(edit)",
    "document.save()",
    'case "openJson"',
    'id="addCase"',
    'id="addAssertion"',
    'id="addTimeline"',
    'id="addParameter"',
    'id="expectErrorEnabled"',
    'data-add-pair="snapshot"',
  ]) {
    assert(source.includes(marker), `missing ${marker}`);
  }
});
