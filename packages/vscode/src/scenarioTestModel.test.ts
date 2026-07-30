import assert from "node:assert/strict";
import test from "node:test";
import {
  expandScenarioTestCases,
  scenarioLanguageLabel,
  scenarioLanguageSummary,
  stringOffset,
} from "./scenarioTestModel.ts";

test("classifies pure Lua, full Lua, and mixed scenario runtimes", () => {
  assert.equal(scenarioLanguageSummary({ programs: [{ language: "lua" }] }), "lua");
  assert.equal(scenarioLanguageSummary({ programs: [{ language: "ic10" }] }), "ic10");
  assert.equal(
    scenarioLanguageSummary({ programs: [{ language: "ic10" }, { language: "lua" }] }),
    "mixed",
  );
  assert.equal(scenarioLanguageLabel("lua", "luaModule"), "Lua module");
  assert.equal(scenarioLanguageLabel("mixed"), "IC10 + Lua");
});

test("expands parameter rows into stable case names", () => {
  assert.deepEqual(
    expandScenarioTestCases({
      schemaVersion: 1,
      scenario: "solar.icsim",
      cases: [
        {
          name: "tracks",
          focusIc: "controller",
          parameters: [
            { name: "sunrise", angle: -90 },
            { angle: 0, speed: 1 },
          ],
        },
      ],
    }),
    [
      {
        caseIndex: 0,
        parameterIndex: 0,
        caseName: "tracks",
        displayName: "sunrise",
        expandedName: "tracks [sunrise]",
        focusIc: "controller",
      },
      {
        caseIndex: 0,
        parameterIndex: 1,
        caseName: "tracks",
        displayName: "angle=0, speed=1",
        expandedName: "tracks [angle=0, speed=1]",
        focusIc: "controller",
      },
    ],
  );
});

test("rejects unknown schema versions and locates source names", () => {
  assert.deepEqual(expandScenarioTestCases({ schemaVersion: 2, cases: [] }), []);
  assert.equal(stringOffset('{"name":"works"}', "works"), 8);
  assert.equal(stringOffset("{}", "missing"), undefined);
});

test("preserves Lua module execution kind for Test Explorer", () => {
  assert.deepEqual(
    expandScenarioTestCases({
      schemaVersion: 1,
      scenario: "pure.icsim",
      cases: [
        {
          name: "pure module",
          focusProgram: "module-tests",
          execution: { kind: "luaModule" },
        },
      ],
    }),
    [
      {
        caseIndex: 0,
        caseName: "pure module",
        displayName: "pure module",
        expandedName: "pure module",
        focusIc: undefined,
        focusProgram: "module-tests",
        executionKind: "luaModule",
      },
    ],
  );
});
