import { spawn } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";

import {
  scenarioLanguageLabel,
  scenarioLanguageSummary,
  ScenarioSource,
  ScenarioTestFixture,
  expandScenarioTestCases,
  stringOffset,
} from "./scenarioTestModel";
import { isTestPath, TEST_GLOB } from "./workspaceFormats.ts";

interface ItemData {
  readonly kind: "file" | "case" | "parameter";
  readonly fixture: vscode.Uri;
  readonly scenario?: vscode.Uri;
  readonly caseName?: string;
  readonly focusIc?: string;
  readonly focusProgram?: string;
  readonly executionKind?: string;
  readonly languageSummary?: ReturnType<typeof scenarioLanguageSummary>;
  readonly parameterName?: string;
}

interface CliSummary {
  readonly files: readonly {
    readonly path: string;
    readonly error?: string;
    readonly cases: readonly CliCase[];
  }[];
}

interface CliCase {
  readonly name: string;
  readonly status: "passed" | "failed" | "invalid";
  readonly failures: readonly CliFailure[];
}

interface CliFailure {
  readonly message: string;
  readonly expression?: string;
  readonly expected?: string;
  readonly actual?: string;
  readonly tick?: number;
  readonly source?: string;
  readonly line?: number;
}

export interface ScenarioTestOperationResult {
  readonly status: "passed" | "failed" | "error";
  readonly message: string;
}

export interface Ic10TestingService {
  validateFixture(fixture: vscode.Uri): Promise<ScenarioTestOperationResult>;
  runCase(
    fixture: vscode.Uri,
    caseName: string,
  ): Promise<ScenarioTestOperationResult>;
}

export function registerIc10Testing(
  context: vscode.ExtensionContext,
  output: vscode.LogOutputChannel,
): Ic10TestingService {
  const controller = vscode.tests.createTestController(
    "ic10ScenarioTests",
    "IC10 Scenario Tests",
  );
  const data = new WeakMap<vscode.TestItem, ItemData>();
  const dependencies = new Map<string, Set<vscode.TestItem>>();
  context.subscriptions.push(controller);

  const discover = async (): Promise<void> => {
    const files = await vscode.workspace.findFiles(
      TEST_GLOB,
      "**/{node_modules,target,dist}/**",
      500,
    );
    const found = new Set(files.map((file) => file.toString()));
    for (const [id] of controller.items) {
      if (!found.has(id)) {
        controller.items.delete(id);
      }
    }
    for (const file of files.sort((left, right) =>
      left.fsPath.localeCompare(right.fsPath),
    )) {
      await discoverFile(controller, data, dependencies, file);
    }
  };

  controller.resolveHandler = async (item) => {
    if (!item) {
      await discover();
    } else if (data.get(item)?.kind === "file") {
      await discoverFile(controller, data, dependencies, item.uri!);
    }
  };

  const runHandler = async (
    request: vscode.TestRunRequest,
    token: vscode.CancellationToken,
  ): Promise<void> => {
    const run = controller.createTestRun(request);
    const items = runnableItems(controller, request, data);
    for (const item of items) {
      if (token.isCancellationRequested) {
        run.skipped(item);
        continue;
      }
      run.started(item);
      const startedAt = Date.now();
      const metadata = data.get(item)!;
      try {
        const result = await runCliCase(
          context,
          metadata.fixture,
          metadata.parameterName ?? metadata.caseName!,
          token,
        );
        if (!result) {
          run.errored(item, new vscode.TestMessage("No CLI result for case."));
        } else if (result.status === "passed") {
          run.passed(item, Date.now() - startedAt);
        } else {
          run.failed(
            item,
            result.failures.map((failure) =>
              testMessage(failure, metadata.languageSummary, metadata.scenario),
            ),
            Date.now() - startedAt,
          );
        }
      } catch (error) {
        run.errored(
          item,
          new vscode.TestMessage(
            error instanceof Error ? error.message : String(error),
          ),
        );
      }
    }
    run.end();
  };

  controller.createRunProfile(
    "Run",
    vscode.TestRunProfileKind.Run,
    runHandler,
    true,
  );
  controller.createRunProfile(
    "Debug",
    vscode.TestRunProfileKind.Debug,
    async (request, token) => {
      const run = controller.createTestRun(request);
      for (const item of runnableItems(controller, request, data)) {
        if (token.isCancellationRequested) {
          run.skipped(item);
          continue;
        }
        const metadata = data.get(item)!;
        if (!metadata.scenario) {
          run.errored(item, new vscode.TestMessage("Test has no scenario."));
          continue;
        }
        if (metadata.executionKind === "luaModule") {
          run.errored(
            item,
            new vscode.TestMessage(
              "Local Lua module debugging is not available in P3-09A; run the test normally.",
            ),
          );
          continue;
        }
        if (metadata.languageSummary === "lua" || metadata.languageSummary === "mixed") {
          run.errored(
            item,
            new vscode.TestMessage(
              `Local ${metadata.languageSummary === "mixed" ? "mixed IC10/Lua" : "Lua chip"} scenario debugging is unsupported. Run it to receive the explicit Lua runner result; StationeersLua remote debugging is a separate live-game path.`,
            ),
          );
          continue;
        }
        run.started(item);
        const folder = vscode.workspace.getWorkspaceFolder(metadata.fixture);
        const started = await vscode.debug.startDebugging(folder, {
          type: "ic10",
          request: "launch",
          name: `IC10 Test: ${metadata.parameterName ?? metadata.caseName}`,
          scenario: metadata.scenario.fsPath,
          focusIc: metadata.focusIc,
          focusProgram: metadata.focusProgram,
          testFile: metadata.fixture.fsPath,
          testName: metadata.parameterName ?? metadata.caseName,
          stopOnEntry: true,
          pauseOnAssertionFailure: true,
          enableHistory: true,
        });
        if (started) {
          run.passed(item);
        } else {
          run.errored(
            item,
            new vscode.TestMessage("Could not start the IC10 debug adapter."),
          );
        }
      }
      run.end();
    },
    true,
  );

  const watcher = vscode.workspace.createFileSystemWatcher(
    "**/*.{ic10,lua,stationeerssim.json,ic10sim.json,stationeerstest.json,ic10test.json}",
  );
  const update = (uri: vscode.Uri): void => {
    if (isTestPath(uri.fsPath)) {
      void discoverFile(controller, data, dependencies, uri);
    }
    const affected = dependencies.get(normalize(uri.fsPath));
    const invalidated = new Set(affected ?? []);
    if (uri.fsPath.toLowerCase().endsWith(".lua")) {
      const collectLuaModules = (item: vscode.TestItem): void => {
        if (data.get(item)?.executionKind === "luaModule") {
          invalidated.add(item);
        }
        for (const [, child] of item.children) {
          collectLuaModules(child);
        }
      };
      for (const [, item] of controller.items) {
        collectLuaModules(item);
      }
    }
    if (invalidated.size > 0) {
      controller.invalidateTestResults([...invalidated]);
      if (
        vscode.workspace
          .getConfiguration("ic10.testing", uri)
          .get<boolean>("rerunOnSave", false)
      ) {
        void runHandler(
          new vscode.TestRunRequest([...invalidated]),
          new vscode.CancellationTokenSource().token,
        );
      }
    }
  };
  watcher.onDidCreate(update, undefined, context.subscriptions);
  watcher.onDidChange(update, undefined, context.subscriptions);
  watcher.onDidDelete((uri) => controller.items.delete(uri.toString()));
  context.subscriptions.push(
    watcher,
    vscode.workspace.onDidSaveTextDocument((document) => update(document.uri)),
  );
  void discover().catch((error: unknown) =>
    output.error(
      `IC10 test discovery failed: ${error instanceof Error ? error.message : String(error)}`,
    ),
  );
  return {
    async validateFixture(fixture) {
      const cancellation = new vscode.CancellationTokenSource();
      try {
        return await validateCliFixture(context, fixture, cancellation.token);
      } finally {
        cancellation.dispose();
      }
    },
    async runCase(fixture, caseName) {
      const cancellation = new vscode.CancellationTokenSource();
      try {
        const cases = await runCliCases(
          context,
          fixture,
          caseName,
          cancellation.token,
        );
        if (cases.length === 0) {
          return {
            status: "error",
            message: `No runnable result matched “${caseName}”.`,
          };
        }
        const unsuccessful = cases.filter((testCase) => testCase.status !== "passed");
        if (unsuccessful.length === 0) {
          return {
            status: "passed",
            message: `${cases.length} ${cases.length === 1 ? "run" : "parameter runs"} passed.`,
          };
        }
        return {
          status: "failed",
          message: unsuccessful
            .flatMap((testCase) =>
              testCase.failures.length > 0
                ? testCase.failures.map(
                    (failure) => `${testCase.name}: ${failure.message}`,
                  )
                : [`${testCase.name}: ${testCase.status}`],
            )
            .join("\n"),
        };
      } catch (error) {
        return {
          status: "error",
          message: error instanceof Error ? error.message : String(error),
        };
      } finally {
        cancellation.dispose();
      }
    },
  };
}

async function discoverFile(
  controller: vscode.TestController,
  data: WeakMap<vscode.TestItem, ItemData>,
  dependencies: Map<string, Set<vscode.TestItem>>,
  uri: vscode.Uri,
): Promise<void> {
  let item = controller.items.get(uri.toString());
  if (!item) {
    item = controller.createTestItem(
      uri.toString(),
      path.basename(uri.fsPath),
      uri,
    );
    controller.items.add(item);
  }
  item.canResolveChildren = true;
  item.error = undefined;
  item.children.replace([]);
  data.set(item, { kind: "file", fixture: uri });
  try {
    const source = Buffer.from(await vscode.workspace.fs.readFile(uri)).toString(
      "utf8",
    );
    const fixture = JSON.parse(source) as ScenarioTestFixture;
    if (!fixture.scenario || !Array.isArray(fixture.cases)) {
      item.error = "Test fixture requires `scenario` and `cases`.";
      return;
    }
    const scenario = vscode.Uri.file(
      path.resolve(path.dirname(uri.fsPath), fixture.scenario),
    );
    const scenarioSource = (await readJson(scenario)) as ScenarioSource | undefined;
    const languageSummary = scenarioLanguageSummary(scenarioSource);
    item.label = `${path.basename(uri.fsPath)} (${scenarioLanguageLabel(languageSummary)})`;
    addDependency(dependencies, scenario.fsPath, item);
    const expanded = expandScenarioTestCases(fixture);
    const caseIndexes = new Set(expanded.map((testCase) => testCase.caseIndex));
    for (const index of caseIndexes) {
      const testCase = expanded.find(
        (candidate) => candidate.caseIndex === index,
      )!;
      const caseItem = controller.createTestItem(
        `${uri.toString()}::${index}`,
        testCase.caseName,
        uri,
      );
      caseItem.range = stringRange(source, testCase.caseName);
      item.children.add(caseItem);
      const base: ItemData = {
        kind: "case",
        fixture: uri,
        scenario,
        caseName: testCase.caseName,
        focusIc: testCase.focusIc,
        focusProgram: testCase.focusProgram,
        executionKind: testCase.executionKind,
        languageSummary,
      };
      caseItem.label = `${testCase.caseName} (${scenarioLanguageLabel(languageSummary, testCase.executionKind)})`;
      data.set(caseItem, base);
      addDependency(dependencies, scenario.fsPath, caseItem);
      const parameters = expanded.filter(
        (candidate) =>
          candidate.caseIndex === index &&
          candidate.parameterIndex !== undefined,
      );
      if (parameters.length) {
        for (const parameter of parameters) {
          const parameterItem = controller.createTestItem(
            `${caseItem.id}::${parameter.parameterIndex}`,
            parameter.displayName,
            uri,
          );
          caseItem.children.add(parameterItem);
          data.set(parameterItem, {
            ...base,
            kind: "parameter",
            parameterName: parameter.expandedName,
            languageSummary,
          });
          parameterItem.label = `${parameter.displayName} (${scenarioLanguageLabel(languageSummary, testCase.executionKind)})`;
          addDependency(dependencies, scenario.fsPath, parameterItem);
        }
      }
    }
    for (const program of scenarioSource?.programs ?? []) {
      if (typeof program.path === "string") {
        const resolved = path.resolve(path.dirname(scenario.fsPath), program.path);
        addDependency(dependencies, resolved, item);
        for (const [, child] of item.children) addDependency(dependencies, resolved, child);
      }
    }
    for (const device of scenarioSource?.devices ?? []) {
      const program = device.ic?.program;
      if (typeof program === "string") {
        const resolved = path.resolve(path.dirname(scenario.fsPath), program);
        addDependency(dependencies, resolved, item);
        for (const [, child] of item.children) {
          addDependency(dependencies, resolved, child);
        }
      }
    }
  } catch (error) {
    item.error = error instanceof Error ? error.message : String(error);
  }
}

function runnableItems(
  controller: vscode.TestController,
  request: vscode.TestRunRequest,
  data: WeakMap<vscode.TestItem, ItemData>,
): vscode.TestItem[] {
  const roots = request.include ? [...request.include] : [...controller.items].map(([, item]) => item);
  const excluded = new Set(request.exclude?.map((item) => item.id) ?? []);
  const result: vscode.TestItem[] = [];
  const visit = (item: vscode.TestItem): void => {
    if (excluded.has(item.id)) {
      return;
    }
    const metadata = data.get(item);
    if (
      metadata &&
      (metadata.kind === "parameter" ||
        (metadata.kind === "case" && item.children.size === 0))
    ) {
      result.push(item);
      return;
    }
    for (const [, child] of item.children) {
      visit(child);
    }
  };
  roots.forEach(visit);
  return result;
}

async function runCliCase(
  context: vscode.ExtensionContext,
  fixture: vscode.Uri,
  name: string,
  token: vscode.CancellationToken,
): Promise<CliCase | undefined> {
  return (await runCliCases(context, fixture, name, token)).find(
    (testCase) => testCase.name === name,
  );
}

async function runCliCases(
  context: vscode.ExtensionContext,
  fixture: vscode.Uri,
  name: string,
  token: vscode.CancellationToken,
): Promise<CliCase[]> {
  const executable = resolveCli(context);
  if (!executable) {
    throw new Error(
      "The IC10 CLI was not found. Build it with `cargo build -p ic10-runner`, or set `ic10.cli.path`.",
    );
  }
  const summary = await new Promise<CliSummary>((resolve, reject) => {
    const child = spawn(
      executable,
      ["test", "--format", "json", "--filter", name, fixture.fsPath],
      { cwd: path.dirname(fixture.fsPath), windowsHide: true },
    );
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => {
      stderr += chunk;
    });
    const cancellation = token.onCancellationRequested(() => child.kill());
    child.on("error", reject);
    child.on("close", () => {
      cancellation.dispose();
      try {
        resolve(JSON.parse(stdout) as CliSummary);
      } catch {
        reject(new Error(stderr || stdout || "IC10 CLI produced no result."));
      }
    });
  });
  return summary.files
    .flatMap((file) => file.cases)
    .filter(
      (testCase) =>
        testCase.name === name || testCase.name.startsWith(`${name} [`),
    );
}

async function validateCliFixture(
  context: vscode.ExtensionContext,
  fixture: vscode.Uri,
  token: vscode.CancellationToken,
): Promise<ScenarioTestOperationResult> {
  const executable = resolveCli(context);
  if (!executable) {
    return {
      status: "error",
      message:
        "The IC10 CLI was not found. Build it with `cargo build -p ic10-runner`, or set `ic10.cli.path`.",
    };
  }
  return new Promise((resolve) => {
    const child = spawn(executable, ["check", fixture.fsPath], {
      cwd: path.dirname(fixture.fsPath),
      windowsHide: true,
    });
    let stdout = "";
    let stderr = "";
    let settled = false;
    child.stdout.setEncoding("utf8").on("data", (chunk: string) => {
      stdout += chunk;
    });
    child.stderr.setEncoding("utf8").on("data", (chunk: string) => {
      stderr += chunk;
    });
    const cancellation = token.onCancellationRequested(() => child.kill());
    child.on("error", (error) => {
      if (settled) {
        return;
      }
      settled = true;
      cancellation.dispose();
      resolve({ status: "error", message: error.message });
    });
    child.on("close", (code) => {
      if (settled) {
        return;
      }
      settled = true;
      cancellation.dispose();
      const message = (stderr || stdout).trim();
      resolve(
        code === 0
          ? {
              status: "passed",
              message: message || "Fixture, scenario, and programs are valid.",
            }
          : {
              status: "failed",
              message: message || `Validation exited with code ${code}.`,
            },
      );
    });
  });
}

function resolveCli(context: vscode.ExtensionContext): string | undefined {
  const configured = vscode.workspace
    .getConfiguration("ic10")
    .get<string>("cli.path", "")
    .trim();
  if (configured && fs.existsSync(configured)) {
    return configured;
  }
  const executable = process.platform === "win32" ? "ic10.exe" : "ic10";
  const development = path.resolve(
    context.extensionPath,
    "..",
    "..",
    "target",
    "debug",
    executable,
  );
  if (
    context.extensionMode === vscode.ExtensionMode.Development &&
    fs.existsSync(development)
  ) {
    return development;
  }
  const bundled = vscode.Uri.joinPath(
    context.extensionUri,
    "server",
    `${process.platform}-${process.arch}`,
    executable,
  ).fsPath;
  if (fs.existsSync(development) && fs.existsSync(bundled)) {
    try {
      if (fs.statSync(development).mtimeMs > fs.statSync(bundled).mtimeMs) {
        return development;
      }
    } catch {
      // Fall through to bundled preference if stat fails.
    }
  }
  return fs.existsSync(bundled)
    ? bundled
    : fs.existsSync(development)
      ? development
      : undefined;
}

function testMessage(
  failure: CliFailure,
  languageSummary?: ReturnType<typeof scenarioLanguageSummary>,
  scenario?: vscode.Uri,
): vscode.TestMessage {
  const context =
    failure.expected !== undefined || failure.actual !== undefined
      ? `\nExpected: ${failure.expected ?? "-"}\nActual: ${failure.actual ?? "-"}`
      : "";
  const message = new vscode.TestMessage(
    `${languageSummary ? `[${languageSummary}] ` : ""}${failure.message}${failure.expression ? `\nExpression: ${failure.expression}` : ""}${failure.tick === undefined ? "" : `\nTick: ${failure.tick}`}${context}`,
  );
  if (failure.source) {
    const line = Math.max(0, (failure.line ?? 1) - 1);
    const source = path.isAbsolute(failure.source)
      ? failure.source
      : path.resolve(scenario ? path.dirname(scenario.fsPath) : ".", failure.source);
    message.location = new vscode.Location(
      vscode.Uri.file(source),
      new vscode.Position(line, 0),
    );
  }
  return message;
}

function stringRange(source: string, value: string): vscode.Range | undefined {
  const offset = stringOffset(source, value);
  if (offset === undefined) {
    return undefined;
  }
  const before = source.slice(0, offset);
  const line = before.split(/\r?\n/u).length - 1;
  const column = offset - Math.max(before.lastIndexOf("\n") + 1, 0);
  return new vscode.Range(line, column, line, column + value.length + 2);
}

function addDependency(
  dependencies: Map<string, Set<vscode.TestItem>>,
  file: string,
  item: vscode.TestItem,
): void {
  const key = normalize(file);
  const items = dependencies.get(key) ?? new Set<vscode.TestItem>();
  items.add(item);
  dependencies.set(key, items);
}

function normalize(file: string): string {
  const normalized = path.normalize(file);
  return process.platform === "win32"
    ? normalized.toLocaleLowerCase()
    : normalized;
}

async function readJson(uri: vscode.Uri): Promise<any | undefined> {
  try {
    return JSON.parse(
      Buffer.from(await vscode.workspace.fs.readFile(uri)).toString("utf8"),
    );
  } catch {
    return undefined;
  }
}
