import * as path from "node:path";

export type TestScalar = number | string;

export interface TestTolerance {
  absolute?: number;
  relative?: number;
}

export interface TestAssertion {
  expression?: string;
  eventually?: string;
  always?: string;
  atTick?: number;
  withinTicks?: number;
  expected?: TestScalar;
  tolerance?: TestTolerance;
}

export interface TestTimelineEvent {
  target: string;
  value: TestScalar;
}

export interface TestTimelineEntry {
  tick: number;
  set?: Record<string, TestScalar>;
  events?: TestTimelineEvent[];
}

export interface TestCaseFixture {
  name: string;
  maxTicks?: number;
  maxOperations?: number;
  focusIc?: string;
  initial?: Record<string, TestScalar>;
  timeline?: TestTimelineEntry[];
  expect?: TestAssertion[];
  expectError?: {
    kind: "compile" | "runtime";
    messageContains?: string;
  };
  parameters?: Record<string, string | number | boolean>[];
  snapshot?: { values: Record<string, TestScalar> };
}

export interface ScenarioTestEditorFixture {
  schemaVersion: number;
  scenario: string;
  seed?: number;
  cases: TestCaseFixture[];
}

export function newScenarioTestFixture(
  scenario = "",
): ScenarioTestEditorFixture {
  return {
    schemaVersion: 1,
    scenario,
    seed: 0,
    cases: [newTestCase()],
  };
}

export function newTestCase(name = "new test"): TestCaseFixture {
  return {
    name,
    maxTicks: 100,
    maxOperations: 100_000,
    initial: {},
    timeline: [],
    expect: [],
    parameters: [],
  };
}

export function cloneTestCase(
  testCase: TestCaseFixture,
  name = `${testCase.name} copy`,
): TestCaseFixture {
  const clone = JSON.parse(JSON.stringify(testCase)) as TestCaseFixture;
  clone.name = name;
  return clone;
}

export function parseTestScalar(text: string): TestScalar {
  const trimmed = text.trim();
  if (["NaN", "Infinity", "-Infinity", "-0"].includes(trimmed)) {
    return trimmed;
  }
  if (trimmed !== "") {
    const numeric = Number(trimmed);
    if (Number.isFinite(numeric)) {
      return numeric;
    }
  }
  return trimmed;
}

export function formatTestScalar(value: unknown): string {
  return typeof value === "string" ? value : String(value ?? "");
}

export function scenarioPathForTest(
  testPath: string,
  scenarioPath: string,
): string {
  const relative = path.relative(path.dirname(testPath), scenarioPath);
  return (path.isAbsolute(relative) ? scenarioPath : relative).replaceAll(
    "\\",
    "/",
  );
}

export function validateScenarioTestFixture(value: unknown): string[] {
  if (!isRecord(value)) {
    return ["The test file must contain a JSON object."];
  }
  const errors: string[] = [];
  validateKeys(
    value,
    ["schemaVersion", "scenario", "seed", "cases"],
    "Test file",
    errors,
  );
  if (value.schemaVersion !== 1) {
    errors.push("schemaVersion must be 1.");
  }
  if (typeof value.scenario !== "string" || value.scenario.trim() === "") {
    errors.push("Choose a simulation environment.");
  } else if (!value.scenario.endsWith(".ic10sim.json")) {
    errors.push("The scenario path must end in .ic10sim.json.");
  }
  if (
    value.seed !== undefined &&
    (!Number.isInteger(value.seed) || Number(value.seed) < 0)
  ) {
    errors.push("The fixture seed must be a non-negative integer.");
  }
  if (!Array.isArray(value.cases) || value.cases.length === 0) {
    errors.push("Add at least one test case.");
    return errors;
  }

  const names = new Set<string>();
  value.cases.forEach((candidate, index) => {
    const location = `Case ${index + 1}`;
    if (!isRecord(candidate)) {
      errors.push(`${location} must be an object.`);
      return;
    }
    validateKeys(
      candidate,
      [
        "name",
        "maxTicks",
        "maxOperations",
        "focusIc",
        "initial",
        "timeline",
        "expect",
        "expectError",
        "parameters",
        "snapshot",
      ],
      location,
      errors,
    );
    const name =
      typeof candidate.name === "string" ? candidate.name.trim() : "";
    if (!name) {
      errors.push(`${location} needs a name.`);
    } else if (names.has(name)) {
      errors.push(`Case names must be unique; “${name}” is duplicated.`);
    } else {
      names.add(name);
    }
    const maxTicks = positiveInteger(candidate.maxTicks, 100);
    const maxOperations = positiveInteger(candidate.maxOperations, 100_000);
    if (maxTicks === undefined) {
      errors.push(`${location} maxTicks must be a positive integer.`);
    }
    if (maxOperations === undefined) {
      errors.push(`${location} maxOperations must be a positive integer.`);
    }
    if (
      candidate.focusIc !== undefined &&
      (typeof candidate.focusIc !== "string" ||
        candidate.focusIc.trim() === "")
    ) {
      errors.push(`${location} focusIc must be a non-empty housing ID.`);
    }

    if (candidate.initial !== undefined && !isScalarMap(candidate.initial)) {
      errors.push(`${location} initial state contains an invalid target or value.`);
    }
    if (candidate.snapshot !== undefined) {
      if (
        !isRecord(candidate.snapshot) ||
        !isScalarMap(candidate.snapshot.values)
      ) {
        errors.push(`${location} snapshot values are invalid.`);
      } else {
        validateKeys(
          candidate.snapshot,
          ["values"],
          `${location} snapshot`,
          errors,
        );
      }
    }
    if (candidate.parameters !== undefined) {
      if (!Array.isArray(candidate.parameters)) {
        errors.push(`${location} parameters must be a list.`);
      } else {
        candidate.parameters.forEach((parameter, parameterIndex) => {
          const entries = isRecord(parameter)
            ? Object.entries(parameter).filter(([key]) => key !== "name")
            : [];
          if (!isRecord(parameter) || entries.length === 0) {
            errors.push(
              `${location} parameter ${parameterIndex + 1} needs at least one value.`,
            );
          } else if (
            (parameter.name !== undefined &&
              typeof parameter.name !== "string") ||
            entries.some(
              ([key, entry]) =>
                key.trim() === "" ||
                !(
                  typeof entry === "string" ||
                  typeof entry === "boolean" ||
                  (typeof entry === "number" && Number.isFinite(entry))
                ),
            )
          ) {
            errors.push(
              `${location} parameter ${parameterIndex + 1} contains an invalid name or value.`,
            );
          }
        });
      }
    }
    if (candidate.timeline !== undefined) {
      if (!Array.isArray(candidate.timeline)) {
        errors.push(`${location} timeline must be a list.`);
      } else {
        candidate.timeline.forEach((entry, timelineIndex) => {
          const entryLocation = `${location} timeline ${timelineIndex + 1}`;
          if (
            !isRecord(entry) ||
            !Number.isInteger(entry.tick) ||
            Number(entry.tick) < 0
          ) {
            errors.push(`${entryLocation} needs a non-negative integer tick.`);
            return;
          }
          validateKeys(entry, ["tick", "set", "events"], entryLocation, errors);
          if (maxTicks !== undefined && Number(entry.tick) > maxTicks) {
            errors.push(`${entryLocation} is beyond maxTicks ${maxTicks}.`);
          }
          if (entry.set !== undefined && !isScalarMap(entry.set)) {
            errors.push(`${entryLocation} set values are invalid.`);
          }
          if (
            entry.events !== undefined &&
            (!Array.isArray(entry.events) ||
              entry.events.some(
                (event) =>
                  !isRecord(event) ||
                  typeof event.target !== "string" ||
                  event.target.trim() === "" ||
                  !isScalar(event.value),
              ))
          ) {
            errors.push(
              `${entryLocation} events contain an invalid target or value.`,
            );
          } else if (Array.isArray(entry.events)) {
            entry.events.forEach((event, eventIndex) => {
              if (isRecord(event)) {
                validateKeys(
                  event,
                  ["target", "value"],
                  `${entryLocation} event ${eventIndex + 1}`,
                  errors,
                );
              }
            });
          }
        });
      }
    }
    if (candidate.expect !== undefined) {
      if (!Array.isArray(candidate.expect)) {
        errors.push(`${location} assertions must be a list.`);
      } else {
        candidate.expect.forEach((assertion, assertionIndex) => {
          validateAssertion(
            assertion,
            `${location} assertion ${assertionIndex + 1}`,
            maxTicks,
            errors,
          );
        });
      }
    }
    if (candidate.expectError !== undefined) {
      if (
        !isRecord(candidate.expectError) ||
        !["compile", "runtime"].includes(String(candidate.expectError.kind)) ||
        (candidate.expectError.messageContains !== undefined &&
          typeof candidate.expectError.messageContains !== "string")
      ) {
        errors.push(
          `${location} expected error must have a compile/runtime kind and an optional text message.`,
        );
      } else {
        validateKeys(
          candidate.expectError,
          ["kind", "messageContains"],
          `${location} expected error`,
          errors,
        );
      }
    }
  });
  return errors;
}

function validateAssertion(
  value: unknown,
  location: string,
  maxTicks: number | undefined,
  errors: string[],
): void {
  if (!isRecord(value)) {
    errors.push(`${location} must be an object.`);
    return;
  }
  validateKeys(
    value,
    [
      "expression",
      "eventually",
      "always",
      "atTick",
      "withinTicks",
      "expected",
      "tolerance",
    ],
    location,
    errors,
  );
  const expressions = ["expression", "eventually", "always"].filter(
    (key) => typeof value[key] === "string" && String(value[key]).trim() !== "",
  );
  if (expressions.length !== 1) {
    errors.push(
      `${location} needs exactly one expression, eventually, or always condition.`,
    );
  }
  if (value.withinTicks !== undefined && expressions[0] !== "eventually") {
    errors.push(`${location} can use withinTicks only with eventually.`);
  }
  for (const key of ["atTick", "withinTicks"]) {
    const deadline = value[key];
    if (
      deadline !== undefined &&
      (!Number.isInteger(deadline) || Number(deadline) < 0)
    ) {
      errors.push(`${location} ${key} must be a non-negative integer.`);
    } else if (
      deadline !== undefined &&
      maxTicks !== undefined &&
      Number(deadline) > maxTicks
    ) {
      errors.push(`${location} ${key} is beyond maxTicks ${maxTicks}.`);
    }
  }
  if (value.expected !== undefined && !isScalar(value.expected)) {
    errors.push(`${location} expected value is invalid.`);
  }
  if (value.tolerance !== undefined) {
    if (!isRecord(value.tolerance)) {
      errors.push(`${location} tolerance must be an object.`);
    } else {
      validateKeys(
        value.tolerance,
        ["absolute", "relative"],
        `${location} tolerance`,
        errors,
      );
      for (const key of ["absolute", "relative"]) {
        const tolerance = value.tolerance[key];
        if (
          tolerance !== undefined &&
          (typeof tolerance !== "number" ||
            !Number.isFinite(tolerance) ||
            tolerance < 0)
        ) {
          errors.push(`${location} ${key} tolerance must be non-negative.`);
        }
      }
    }
  }
}

function validateKeys(
  value: Record<string, unknown>,
  allowed: readonly string[],
  location: string,
  errors: string[],
): void {
  const unexpected = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unexpected.length > 0) {
    errors.push(
      `${location} contains unsupported ${
        unexpected.length === 1 ? "field" : "fields"
      }: ${unexpected.join(", ")}.`,
    );
  }
}

function positiveInteger(value: unknown, fallback: number): number | undefined {
  const candidate = value === undefined ? fallback : value;
  return Number.isInteger(candidate) && Number(candidate) > 0
    ? Number(candidate)
    : undefined;
}

function isScalarMap(value: unknown): boolean {
  return (
    isRecord(value) &&
    Object.entries(value).every(
      ([key, scalar]) => key.trim() !== "" && isScalar(scalar),
    )
  );
}

function isScalar(value: unknown): value is TestScalar {
  return (
    (typeof value === "number" && Number.isFinite(value)) ||
    typeof value === "string"
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
