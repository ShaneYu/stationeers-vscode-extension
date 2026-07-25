import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

import {
  configuredBuildOptions,
  optimizationReport,
  requestBuild,
  writeBuildFiles,
} from "./build";
import {
  Ic10DebugAdapterFactory,
  Ic10DebugConfigurationProvider,
  debugType,
} from "./debug";
import {
  Ic10EnvironmentEditorProvider,
  createSimulationEnvironment,
  openEnvironmentTarget,
  registerSimulationProgramRenameTracking,
} from "./environmentEditor";
import { EnvironmentIntelligence } from "./environmentIntelligence";
import { SimulationLaunchService } from "./simulationLaunch";
import { Ic10StateViewProvider } from "./stateView";
import { registerIc10Testing } from "./testing";

let client: LanguageClient | undefined;
let outputChannel: vscode.LogOutputChannel | undefined;
let budgetStatusBar: vscode.StatusBarItem | undefined;
let environmentIntelligence: EnvironmentIntelligence | undefined;
const programBudgets = new Map<string, ProgramBudget>();

interface ProgramBudget {
  uri: string;
  physicalLines: number;
  programLines: number;
  maximumProgramLines: number;
  estimatedOperationsPerTick?: number;
  maximumOperationsPerTick: number;
}

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  outputChannel = vscode.window.createOutputChannel(
    "Stationeers IC10 Toolkit",
    {
      log: true,
    },
  );
  context.subscriptions.push(outputChannel);
  budgetStatusBar = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    90,
  );
  context.subscriptions.push(
    budgetStatusBar,
    vscode.window.onDidChangeActiveTextEditor(() => updateBudgetStatusBar()),
  );
  const simulationLaunchService = new SimulationLaunchService();
  const debugConfigurationProvider = new Ic10DebugConfigurationProvider(
    simulationLaunchService,
  );
  const stateViewProvider = new Ic10StateViewProvider(context);
  registerIc10Testing(context, outputChannel);
  registerSimulationProgramRenameTracking(context);
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      debugType,
      new Ic10DebugAdapterFactory(context, outputChannel),
    ),
    vscode.debug.registerDebugConfigurationProvider(
      debugType,
      debugConfigurationProvider,
    ),
    vscode.debug.registerDebugConfigurationProvider(
      debugType,
      debugConfigurationProvider,
      vscode.DebugConfigurationProviderTriggerKind.Dynamic,
    ),
    vscode.window.registerCustomEditorProvider(
      Ic10EnvironmentEditorProvider.viewType,
      new Ic10EnvironmentEditorProvider(context, simulationLaunchService),
      {
        supportsMultipleEditorsPerDocument: false,
        webviewOptions: { retainContextWhenHidden: true },
      },
    ),
    vscode.window.registerWebviewViewProvider(
      Ic10StateViewProvider.viewType,
      stateViewProvider,
      { webviewOptions: { retainContextWhenHidden: true } },
    ),
    vscode.commands.registerCommand(
      "ic10.createEnvironment",
      createSimulationEnvironment,
    ),
    vscode.commands.registerCommand("ic10.stepWorldTick", () =>
      stateViewProvider.stepWorldTick(),
    ),
  );
  context.subscriptions.push(
    vscode.commands.registerCommand("ic10.restartServer", async () => {
      await stopClient();
      await startClient(context);
      void vscode.window.showInformationMessage(
        "IC10 language server restarted.",
      );
    }),
  );
  context.subscriptions.push(
    vscode.commands.registerCommand(
      "ic10.removeAllComments",
      async () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
          return;
        }
        if (editor.document.languageId !== "ic10") {
          return;
        }
        const activeClient = client;
        if (!activeClient) {
          return;
        }
        try {
          const result = await requestBuild(activeClient, editor.document, {
            optimization: "readable",
          });
          await editor.edit((edit) =>
            edit.replace(
              new vscode.Range(
                editor.document.positionAt(0),
                editor.document.positionAt(editor.document.getText().length),
              ),
              result.code,
            ),
          );
        } catch (error) {
          void vscode.window.showErrorMessage(
            `Comments were not removed: ${error instanceof Error ? error.message : String(error)}`,
          );
        }
      },
    ),
  );
  await startClient(context);
  registerBuildCommands(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
}

function registerBuildCommands(context: vscode.ExtensionContext): void {
  const buildDocument = async (
    mode: "file" | "clipboard" | "open",
  ): Promise<void> => {
    const document = vscode.window.activeTextEditor?.document;
    const activeClient = client;
    if (!document || document.languageId !== "ic10" || !activeClient) {
      return;
    }
    try {
      const options = configuredBuildOptions(document);
      const output = await requestBuild(activeClient, document, options);
      outputChannel?.info(optimizationReport(output));
      if (mode === "clipboard") {
        await vscode.env.clipboard.writeText(output.code);
        void vscode.window.showInformationMessage(
          `Copied ${output.report.generatedLines} deployable IC10 lines.`,
        );
        return;
      }
      if (options.optimization === "compact") {
        const preview = await vscode.workspace.openTextDocument({
          language: "ic10",
          content: output.code,
        });
        await vscode.commands.executeCommand(
          "vscode.diff",
          document.uri,
          preview.uri,
          `IC10 build preview: ${path.basename(document.fileName)}`,
          { preview: true },
        );
      }
      const files = await writeBuildFiles(document, output);
      if (mode === "open") {
        await vscode.window.showTextDocument(
          await vscode.workspace.openTextDocument(files.code),
        );
      } else {
        void vscode.window.showInformationMessage(
          `Built deployable IC10: ${files.code.fsPath}`,
        );
      }
    } catch (error) {
      void vscode.window.showErrorMessage(
        `IC10 build failed: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("ic10.buildForGame", () =>
      buildDocument("file"),
    ),
    vscode.commands.registerCommand("ic10.copyDeployableCode", () =>
      buildDocument("clipboard"),
    ),
    vscode.commands.registerCommand("ic10.openBuiltCode", () =>
      buildDocument("open"),
    ),
  );
}

async function startClient(
  context: vscode.ExtensionContext,
): Promise<void> {
  const executable = resolveServerExecutable(context);
  if (!executable) {
    const message =
      "The IC10 language server was not found. Build it with `cargo build -p ic10-lsp`, or set `ic10.server.path`.";
    outputChannel?.appendLine(message);
    void vscode.window.showErrorMessage(message);
    return;
  }
  outputChannel?.info(`Starting IC10 language server: ${executable}`);

  const serverOptions: ServerOptions = {
    command: executable,
    args: [],
    options: {
      env: { ...process.env, RUST_BACKTRACE: "1" },
    },
  };
  const fileWatcher = vscode.workspace.createFileSystemWatcher("**/*.ic10");
  context.subscriptions.push(fileWatcher);
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { language: "ic10", scheme: "file" },
      { language: "ic10", scheme: "untitled" },
    ],
    synchronize: {
      fileEvents: fileWatcher,
      configurationSection: "ic10",
    },
    outputChannel,
    markdown: {
      supportHtml: true,
    },
    initializationOptions: {
      assetUri: vscode.Uri.joinPath(
        context.extensionUri,
        "assets",
        "devices",
      ).toString(true),
      unusedDiagnostics: vscode.workspace
        .getConfiguration("ic10")
        .get<string>("diagnostics.unused", "hint"),
    },
  };

  client = new LanguageClient(
    "ic10",
    "Stationeers IC10 Toolkit",
    serverOptions,
    clientOptions,
  );
  context.subscriptions.push(
    client.onNotification(
      "ic10/programBudget",
      (budget: ProgramBudget): void => {
        programBudgets.set(budget.uri, budget);
        updateBudgetStatusBar();
      },
    ),
  );
  await client.start();
  environmentIntelligence = new EnvironmentIntelligence(
    context,
    client,
    openEnvironmentTarget,
  );
  context.subscriptions.push(environmentIntelligence);
  await environmentIntelligence.start();
}

async function stopClient(): Promise<void> {
  environmentIntelligence?.dispose();
  environmentIntelligence = undefined;
  const activeClient = client;
  client = undefined;
  if (activeClient) {
    await activeClient.stop();
  }
}

function updateBudgetStatusBar(): void {
  const editor = vscode.window.activeTextEditor;
  if (!budgetStatusBar || editor?.document.languageId !== "ic10") {
    budgetStatusBar?.hide();
    return;
  }
  const budget = programBudgets.get(editor.document.uri.toString());
  if (!budget) {
    budgetStatusBar.hide();
    return;
  }
  const operations =
    budget.estimatedOperationsPerTick === undefined
      ? "ops/tick unknown"
      : `${budget.estimatedOperationsPerTick}/${budget.maximumOperationsPerTick} ops/tick`;
  budgetStatusBar.text =
    `${budget.physicalLines * 10 >= budget.maximumProgramLines * 9 ? "$(warning)" : "$(symbol-number)"} ${budget.physicalLines}/${budget.maximumProgramLines} lines`;
  budgetStatusBar.backgroundColor =
    budget.physicalLines * 10 >= budget.maximumProgramLines * 9
      ? new vscode.ThemeColor("statusBarItem.warningBackground")
      : undefined;
  budgetStatusBar.tooltip = new vscode.MarkdownString(
    [
      "### IC10 program budget",
      "",
      `- Physical lines: **${budget.physicalLines} / ${budget.maximumProgramLines}**`,
      `- Non-empty program lines: **${budget.programLines}**`,
      `- Static estimate: **${operations}**`,
      "- Program bytes: **unknown (no official generated limit)**",
      "- Bytes per line: **unknown (no official generated limit)**",
      "",
      "Operation count is shown only when control flow can be estimated safely.",
    ].join("\n"),
  );
  budgetStatusBar.show();
}

function resolveServerExecutable(
  context: vscode.ExtensionContext,
): string | undefined {
  const configuredPath = vscode.workspace
    .getConfiguration("ic10")
    .get<string>("server.path", "")
    .trim();
  if (configuredPath) {
    if (fs.existsSync(configuredPath)) {
      return configuredPath;
    }
    outputChannel?.appendLine(
      `Configured ic10.server.path does not exist: ${configuredPath}`,
    );
  }

  const executableName = process.platform === "win32" ? "ic10-lsp.exe" : "ic10-lsp";
  const development = path.resolve(
    context.extensionPath,
    "..",
    "..",
    "target",
    "debug",
    executableName,
  );
  if (
    context.extensionMode === vscode.ExtensionMode.Development &&
    fs.existsSync(development)
  ) {
    return development;
  }

  const platformDirectory = `${process.platform}-${process.arch}`;
  const bundled = vscode.Uri.joinPath(
    context.extensionUri,
    "server",
    platformDirectory,
    executableName,
  ).fsPath;
  if (fs.existsSync(bundled)) {
    return bundled;
  }

  return fs.existsSync(development) ? development : undefined;
}
