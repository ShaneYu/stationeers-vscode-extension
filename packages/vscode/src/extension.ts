import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

import { removeIc10Comments } from "./comments";
import {
  Ic10DebugAdapterFactory,
  Ic10DebugConfigurationProvider,
  debugType,
} from "./debug";
import {
  Ic10EnvironmentEditorProvider,
  createSimulationEnvironment,
  registerSimulationProgramRenameTracking,
} from "./environmentEditor";
import { SimulationLaunchService } from "./simulationLaunch";
import { Ic10StateViewProvider } from "./stateView";
import { registerIc10Testing } from "./testing";

let client: LanguageClient | undefined;
let outputChannel: vscode.LogOutputChannel | undefined;
let budgetStatusBar: vscode.StatusBarItem | undefined;
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
    vscode.commands.registerTextEditorCommand(
      "ic10.removeAllComments",
      (editor, edit) => {
        if (editor.document.languageId !== "ic10") {
          return;
        }

        const source = editor.document.getText();
        const result = removeIc10Comments(source);
        if (result.text === source) {
          return;
        }

        edit.replace(
          new vscode.Range(
            editor.document.positionAt(0),
            editor.document.positionAt(source.length),
          ),
          result.text,
        );

        if (
          result.removedCommentLines > 0 &&
          result.unadjustedRelativeBranches > 0
        ) {
          void vscode.window.showWarningMessage(
            `Removed IC10 comment lines, but ${result.unadjustedRelativeBranches} relative branch offset(s) use a register, alias, define, or non-integer value and could not be adjusted automatically.`,
          );
        }
      },
    ),
  );
  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await stopClient();
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
}

async function stopClient(): Promise<void> {
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
    `$(symbol-number) ${budget.physicalLines}/${budget.maximumProgramLines} lines`;
  budgetStatusBar.tooltip = new vscode.MarkdownString(
    [
      "### IC10 program budget",
      "",
      `- Physical lines: **${budget.physicalLines} / ${budget.maximumProgramLines}**`,
      `- Non-empty program lines: **${budget.programLines}**`,
      `- Static estimate: **${operations}**`,
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
