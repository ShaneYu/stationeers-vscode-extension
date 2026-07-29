import * as path from "node:path";
import { isSimulationPath } from "./workspaceFormats.ts";

const LUA_MAX_INSTRUCTIONS = 10_000_000;
const LUA_LIMIT_MAXIMA = {
  memoryLimitBytes: 64 * 1024 * 1024,
  maxOutputBytes: 1024 * 1024,
  maxModules: 256,
  maxSourceBytes: 4 * 1024 * 1024,
  maxRecursionDepth: 512,
} as const;

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

export type ScriptAction =
  | { action: "set"; target: string; value: TestScalar }
  | { action: "moveSlot"; from: string; to: string }
  | { action: "publish"; network: string; channel: number; value: TestScalar }
  | { action: "schedule"; afterTicks: number; actions: ScriptAction[] };

export interface ScriptedDriver {
  id: string;
  model?: string;
  version?: number;
  rules: {
    name?: string;
    when: { target: string; equals?: TestScalar };
    actions: ScriptAction[];
  }[];
}

export interface TestCaseFixture {
  name: string;
  maxTicks?: number;
  maxOperations?: number;
  /** Canonical neutral selector; focusIc remains a legacy input alias. */
  focusProgram?: string;
  focusIc?: string;
  execution?: {
    kind: "luaModule";
    profile?: "stationeerslua-0.9.5.0-lua5.2-pure-module-v1";
    moduleRoots?: string[];
    memoryLimitBytes?: number;
    maxOutputBytes?: number;
    maxModules?: number;
    maxSourceBytes?: number;
    maxRecursionDepth?: number;
  };
  initial?: Record<string, TestScalar>;
  timeline?: TestTimelineEntry[];
  drivers?: ScriptedDriver[];
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
  } else if (!isSimulationPath(value.scenario)) {
    errors.push("The scenario path must end in .stationeerssim.json or .ic10sim.json.");
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
  if (
    typeof value.scenario === "string" &&
    value.cases.some(
      (candidate) => isRecord(candidate) && candidate.execution !== undefined,
    ) &&
    !isPortableRelativePath(value.scenario)
  ) {
    errors.push(
      "Lua module tests require a test-relative scenario path without parent traversal.",
    );
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
        "focusProgram",
        "execution",
        "initial",
        "timeline",
        "drivers",
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
    if (
      candidate.focusProgram !== undefined &&
      (typeof candidate.focusProgram !== "string" ||
        candidate.focusProgram.trim() === "")
    ) {
      errors.push(`${location} focusProgram must be a non-empty program ID.`);
    }
    if (candidate.execution !== undefined) {
      if (!isRecord(candidate.execution)) {
        errors.push(`${location} execution must be an object.`);
      } else {
        validateKeys(
          candidate.execution,
          [
            "kind",
            "profile",
            "moduleRoots",
            "memoryLimitBytes",
            "maxOutputBytes",
            "maxModules",
            "maxSourceBytes",
            "maxRecursionDepth",
          ],
          `${location} execution`,
          errors,
        );
        if (candidate.execution.kind !== "luaModule") {
          errors.push(`${location} execution kind must be luaModule.`);
        }
        if (
          candidate.execution.profile !== undefined &&
          candidate.execution.profile !==
            "stationeerslua-0.9.5.0-lua5.2-pure-module-v1"
        ) {
          errors.push(`${location} requests an unsupported Lua profile.`);
        }
        if (
          candidate.execution.moduleRoots !== undefined &&
          (!Array.isArray(candidate.execution.moduleRoots) ||
            candidate.execution.moduleRoots.some(
              (root) =>
                typeof root !== "string" ||
                root.trim() === "" ||
                !isPortableRelativePath(root),
            ))
        ) {
          errors.push(
            `${location} moduleRoots must contain non-empty, test-relative paths.`,
          );
        }
        for (const key of [
          "memoryLimitBytes",
          "maxOutputBytes",
          "maxModules",
          "maxSourceBytes",
          "maxRecursionDepth",
        ] as const) {
          if (
            candidate.execution[key] !== undefined &&
            positiveInteger(candidate.execution[key], 1) === undefined
          ) {
            errors.push(`${location} ${key} must be a positive integer.`);
          }
        }
        if (
          maxOperations !== undefined &&
          maxOperations > LUA_MAX_INSTRUCTIONS
        ) {
          errors.push(
            `${location} maxOperations exceeds the hard Lua sandbox limit.`,
          );
        }
        for (const [key, maximum] of Object.entries(LUA_LIMIT_MAXIMA) as [
          keyof typeof LUA_LIMIT_MAXIMA,
          number,
        ][]) {
          const configured = candidate.execution[key];
          if (typeof configured === "number" && configured > maximum) {
            errors.push(`${location} ${key} exceeds the hard Lua sandbox limit.`);
          }
        }
        if (
          typeof candidate.focusProgram !== "string" ||
          candidate.focusProgram.trim() === ""
        ) {
          errors.push(`${location} luaModule execution requires focusProgram.`);
        }
        if (
          (isRecord(candidate.initial) &&
            Object.keys(candidate.initial).length > 0) ||
          (Array.isArray(candidate.timeline) && candidate.timeline.length > 0) ||
          (Array.isArray(candidate.drivers) && candidate.drivers.length > 0) ||
          (Array.isArray(candidate.expect) && candidate.expect.length > 0) ||
          candidate.snapshot !== undefined
        ) {
          errors.push(
            `${location} luaModule execution cannot use world state, timelines, drivers, world assertions, or snapshots.`,
          );
        }
      }
    }

    if (candidate.initial !== undefined && !isStateMap(candidate.initial)) {
      errors.push(`${location} initial state contains an invalid target or value.`);
    }
    if (candidate.snapshot !== undefined) {
      if (
        !isRecord(candidate.snapshot) ||
        !isValueMap(candidate.snapshot.values)
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
    const placeholders = collectParameterPlaceholders(
      Object.fromEntries(
        Object.entries(candidate).filter(([key]) => key !== "parameters"),
      ),
    );
    if (placeholders.length > 0) {
      if (
        !Array.isArray(candidate.parameters) ||
        candidate.parameters.length === 0
      ) {
        errors.push(
          `${location} uses ${placeholders.map((name) => `\${${name}}`).join(", ")} but has no parameter sets.`,
        );
      } else {
        candidate.parameters.forEach((parameter, parameterIndex) => {
          if (!isRecord(parameter)) {
            return;
          }
          const missing = placeholders.filter(
            (name) => !Object.hasOwn(parameter, name),
          );
          if (missing.length > 0) {
            errors.push(
              `${location} parameter ${parameterIndex + 1} is missing ${missing.map((name) => `\${${name}}`).join(", ")}.`,
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
          if (entry.set !== undefined && !isStateMap(entry.set)) {
            errors.push(`${entryLocation} set values are invalid.`);
          }
          if (
            entry.events !== undefined &&
            (!Array.isArray(entry.events) ||
              entry.events.some(
                (event) =>
                  !isRecord(event) ||
                  typeof event.target !== "string" ||
                  !isStateTarget(event.target) ||
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
    if (candidate.drivers !== undefined) {
      if (!Array.isArray(candidate.drivers) || candidate.drivers.length > 32) {
        errors.push(`${location} scripted drivers must be a list of at most 32.`);
      } else {
        candidate.drivers.forEach((driver, driverIndex) => {
          const driverLocation = `${location} driver ${driverIndex + 1}`;
          if (!isRecord(driver)) {
            errors.push(`${driverLocation} must be an object.`);
            return;
          }
          validateKeys(driver, ["id", "model", "version", "rules"], driverLocation, errors);
          if (typeof driver.id !== "string" || driver.id.trim() === "") {
            errors.push(`${driverLocation} needs an id.`);
          }
          if (driver.model !== undefined && (typeof driver.model !== "string" || driver.model.trim() === "")) {
            errors.push(`${driverLocation} model must be non-empty.`);
          }
          if (driver.version !== undefined && (!Number.isInteger(driver.version) || Number(driver.version) < 1)) {
            errors.push(`${driverLocation} version must be a positive integer.`);
          }
          if (!Array.isArray(driver.rules) || driver.rules.length === 0 || driver.rules.length > 256) {
            errors.push(`${driverLocation} needs 1–256 rules.`);
            return;
          }
          driver.rules.forEach((rule, ruleIndex) => validateScriptRule(
            rule,
            `${driverLocation} rule ${ruleIndex + 1}`,
            errors,
          ));
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

function validateScriptRule(value: unknown, location: string, errors: string[]): void {
  if (!isRecord(value)) {
    errors.push(`${location} must be an object.`);
    return;
  }
  validateKeys(value, ["name", "when", "actions"], location, errors);
  if (!isRecord(value.when) || typeof value.when.target !== "string" || !isStateTarget(value.when.target)) {
    errors.push(`${location} trigger needs a valid simulator target.`);
  } else {
    validateKeys(value.when, ["target", "equals"], `${location} trigger`, errors);
    if (value.when.equals !== undefined && !isScalar(value.when.equals)) {
      errors.push(`${location} trigger equals value is invalid.`);
    }
  }
  if (!Array.isArray(value.actions) || value.actions.length === 0) {
    errors.push(`${location} needs at least one action.`);
  } else {
    value.actions.forEach((action, index) =>
      validateScriptAction(action, `${location} action ${index + 1}`, errors, 0),
    );
  }
}

function validateScriptAction(
  value: unknown,
  location: string,
  errors: string[],
  depth: number,
): void {
  if (!isRecord(value) || depth > 8) {
    errors.push(`${location} is invalid or exceeds nesting depth 8.`);
    return;
  }
  switch (value.action) {
    case "set":
      validateKeys(value, ["action", "target", "value"], location, errors);
      if (typeof value.target !== "string" || !isStateTarget(value.target) || !isScalar(value.value)) {
        errors.push(`${location} set requires a valid target and value.`);
      }
      break;
    case "moveSlot":
      validateKeys(value, ["action", "from", "to"], location, errors);
      if (![value.from, value.to].every((entry) =>
        typeof entry === "string" && /^device\("[^"]+"\)\.slot\[\d+\]$/.test(entry))) {
        errors.push(`${location} moveSlot requires two device slot endpoints.`);
      }
      break;
    case "publish":
      validateKeys(value, ["action", "network", "channel", "value"], location, errors);
      if (typeof value.network !== "string" || value.network.trim() === "" ||
          !Number.isInteger(value.channel) || Number(value.channel) < 0 || Number(value.channel) > 7 ||
          !isScalar(value.value)) {
        errors.push(`${location} publish requires a network, Channel0–7, and value.`);
      }
      break;
    case "schedule":
      validateKeys(value, ["action", "afterTicks", "actions"], location, errors);
      if (!Number.isInteger(value.afterTicks) || Number(value.afterTicks) < 0 ||
          !Array.isArray(value.actions) || value.actions.length === 0) {
        errors.push(`${location} schedule requires a delay and nested actions.`);
      } else {
        value.actions.forEach((action, index) =>
          validateScriptAction(action, `${location}.${index + 1}`, errors, depth + 1),
        );
      }
      break;
    default:
      errors.push(`${location} action must be set, moveSlot, publish, or schedule.`);
  }
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

function collectParameterPlaceholders(value: unknown): string[] {
  const found = new Set<string>();
  const visit = (candidate: unknown): void => {
    if (typeof candidate === "string") {
      for (const match of candidate.matchAll(
        /\$\{([A-Za-z_][A-Za-z0-9_]*)\}/gu,
      )) {
        found.add(match[1]!);
      }
    } else if (Array.isArray(candidate)) {
      candidate.forEach(visit);
    } else if (isRecord(candidate)) {
      Object.entries(candidate).forEach(([key, entry]) => {
        visit(key);
        visit(entry);
      });
    }
  };
  visit(value);
  return [...found].sort();
}

function positiveInteger(value: unknown, fallback: number): number | undefined {
  const candidate = value === undefined ? fallback : value;
  return Number.isInteger(candidate) && Number(candidate) > 0
    ? Number(candidate)
    : undefined;
}

function isPortableRelativePath(value: string): boolean {
  return (
    value.trim() !== "" &&
    !path.posix.isAbsolute(value) &&
    !path.win32.isAbsolute(value) &&
    !/^[A-Za-z]:/u.test(value) &&
    !value.split(/[\\/]/u).includes("..")
  );
}

function isStateMap(value: unknown): boolean {
  return (
    isValueMap(value) &&
    Object.keys(value).every((key) => isStateTarget(key))
  );
}

function isValueMap(
  value: unknown,
): value is Record<string, TestScalar> {
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
    (typeof value === "string" &&
      (["NaN", "Infinity", "-Infinity", "-0"].includes(value) ||
        isParameterPlaceholder(value)))
  );
}

function isStateTarget(value: string): boolean {
  return (
    isParameterPlaceholder(value) ||
    /^(?:r(?:[0-9]|1[0-7])|ra|sp|stack\[[0-9]+\]|device\("[^"]+"\)\.(?:[A-Za-z][A-Za-z0-9]*|slot\[[0-9]+\]\.[A-Za-z][A-Za-z0-9]*|memory\[[0-9]+\])|network\("[^"]+"\)\.Channel[0-7])$/u.test(
      value,
    )
  );
}

function isParameterPlaceholder(value: string): boolean {
  return /^\$\{[A-Za-z_][A-Za-z0-9_]*\}$/u.test(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
