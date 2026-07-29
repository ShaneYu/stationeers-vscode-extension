import assert from "node:assert/strict";
import path from "node:path";
import test from "node:test";
import {
  cloneTestCase,
  formatTestScalar,
  newScenarioTestFixture,
  parseTestScalar,
  scenarioPathForTest,
  validateScenarioTestFixture,
} from "./scenarioTestEditorModel.ts";

test("creates and clones guarded scenario tests", () => {
  const fixture = newScenarioTestFixture("./simulation.ic10sim.json");
  fixture.cases[0]!.initial = { r2: "${angle}" };
  fixture.cases[0]!.parameters = [{ name: "sunrise", angle: -90 }];
  fixture.cases[0]!.expect = [
    { expression: "r2", expected: "${angle}", atTick: 0 },
  ];
  assert.deepEqual(validateScenarioTestFixture(fixture), []);

  const clone = cloneTestCase(fixture.cases[0]!, "second");
  clone.initial = { r0: 42 };
  assert.equal(fixture.cases[0]!.initial?.r0, undefined);
  assert.equal(clone.name, "second");
});

test("parses editable scalar values without losing special values", () => {
  assert.equal(parseTestScalar("42"), 42);
  assert.equal(parseTestScalar("-0"), "-0");
  assert.equal(parseTestScalar("NaN"), "NaN");
  assert.equal(parseTestScalar("${angle}"), "${angle}");
  assert.equal(formatTestScalar("Infinity"), "Infinity");
});

test("reports actionable cross-field validation errors", () => {
  const fixture = newScenarioTestFixture("./simulation.ic10sim.json");
  fixture.cases[0] = {
    name: "",
    maxTicks: 2,
    maxOperations: 0,
    timeline: [{ tick: 3 }],
    expect: [{ expression: "r0", eventually: "r0 == 1", withinTicks: 4 }],
    parameters: [{}],
  };
  (fixture.cases[0] as unknown as Record<string, unknown>).unsupported = true;
  const errors = validateScenarioTestFixture(fixture);

  assert(errors.some((error) => error.includes("needs a name")));
  assert(errors.some((error) => error.includes("maxOperations")));
  assert(errors.some((error) => error.includes("beyond maxTicks")));
  assert(errors.some((error) => error.includes("exactly one")));
  assert(errors.some((error) => error.includes("parameter 1")));
  assert(errors.some((error) => error.includes("unsupported")));
});

test("rejects malformed state targets and unexplained scalar strings", () => {
  const fixture = newScenarioTestFixture("./simulation.ic10sim.json");
  fixture.cases[0]!.initial = {
    "device(no-quotes).On": 1,
    r0: "mystery",
  };

  const errors = validateScenarioTestFixture(fixture);
  assert(errors.some((error) => error.includes("initial state")));
});

test("requires every referenced placeholder in every parameter set", () => {
  const fixture = newScenarioTestFixture("./simulation.ic10sim.json");
  fixture.cases[0]!.initial = { r0: "${angle}" };
  fixture.cases[0]!.parameters = [{ name: "missing angle", speed: 1 }];

  const errors = validateScenarioTestFixture(fixture);
  assert(errors.some((error) => error.includes("missing ${angle}")));
});

test("writes portable scenario paths relative to the test file", () => {
  const root = path.resolve("/workspace");
  assert.equal(
    scenarioPathForTest(
      path.join(root, "tests", "controller.ic10test.json"),
      path.join(root, "simulations", "controller.ic10sim.json"),
    ),
    "../simulations/controller.ic10sim.json",
  );
});

test("validates bounded declarative scripted drivers", () => {
  const fixture = newScenarioTestFixture("./simulation.ic10sim.json");
  fixture.cases[0]!.drivers = [{
    id: "mock-vendor",
    model: "example.vendor",
    version: 1,
    rules: [{
      name: "vend later",
      when: { target: 'device("vendor").Activate', equals: 1 },
      actions: [{
        action: "schedule",
        afterTicks: 1,
        actions: [{
          action: "moveSlot",
          from: 'device("vendor").slot[0]',
          to: 'device("outlet").slot[0]',
        }],
      }],
    }],
  }];
  assert.deepEqual(validateScenarioTestFixture(fixture), []);

  fixture.cases[0]!.drivers[0]!.rules[0]!.actions = [{
    action: "publish",
    network: "data",
    channel: 8,
    value: 1,
  }];
  assert(validateScenarioTestFixture(fixture).some((error) =>
    error.includes("Channel0–7")));
});

test("validates explicit pure Lua module execution", () => {
  const fixture = newScenarioTestFixture("./pure.stationeerssim.json");
  fixture.cases[0] = {
    name: "pure module",
    focusProgram: "module-tests",
    execution: {
      kind: "luaModule",
      profile: "stationeerslua-0.9.5.0-lua5.2-pure-module-v1",
      moduleRoots: ["lib"],
      memoryLimitBytes: 1024 * 1024,
    },
  };
  assert.deepEqual(validateScenarioTestFixture(fixture), []);

  fixture.cases[0]!.execution!.moduleRoots = ["C:\\lua"];
  assert(
    validateScenarioTestFixture(fixture).some((error) =>
      error.includes("test-relative"),
    ),
  );
  fixture.cases[0]!.execution!.moduleRoots = ["../lua"];
  assert(
    validateScenarioTestFixture(fixture).some((error) =>
      error.includes("test-relative"),
    ),
  );
  fixture.cases[0]!.execution!.moduleRoots = ["C:lua"];
  assert(
    validateScenarioTestFixture(fixture).some((error) =>
      error.includes("test-relative"),
    ),
  );
  fixture.cases[0]!.execution!.moduleRoots = ["lib"];

  fixture.cases[0]!.execution!.memoryLimitBytes = 64 * 1024 * 1024 + 1;
  assert(
    validateScenarioTestFixture(fixture).some((error) =>
      error.includes("hard Lua sandbox limit"),
    ),
  );
  fixture.cases[0]!.execution!.memoryLimitBytes = 1024 * 1024;

  fixture.cases[0]!.initial = { r0: 1 };
  assert(
    validateScenarioTestFixture(fixture).some((error) =>
      error.includes("cannot use world state"),
    ),
  );
});
