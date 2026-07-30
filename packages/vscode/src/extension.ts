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
import { EnvironmentDebugOverlayService } from "./environmentDebugOverlay";
import { createEnvironmentFromTemplate } from "./environmentTemplates";
import { EnvironmentProposalService } from "./environmentProposal";
import { SimulationLaunchService } from "./simulationLaunch";
import { Ic10StateViewProvider } from "./stateView";
import { registerIc10Testing } from "./testing";
import { shouldWarnForLegacyLuaExtension } from "./workspaceFormats.ts";
import { registerLiveExplorer } from "./liveExplorer";
import {
  Ic10ScenarioTestEditorProvider,
  createScenarioTest,
} from "./scenarioTestEditor";

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
  warnForLegacyLuaExtension();
  await ensureLuaAnnotationLibrary(context).catch(() => {
    // Lua integration remains available through the explicit setup command
    // when workspace settings are read-only or managed by the user.
  });
  void warnForObsoleteWorkspaceFiles();
  const coverageDecoration = vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    backgroundColor: new vscode.ThemeColor(
      "editor.wordHighlightStrongBackground",
    ),
    overviewRulerColor: new vscode.ThemeColor(
      "testing.iconPassed",
    ),
    overviewRulerLane: vscode.OverviewRulerLane.Left,
  });
  outputChannel = vscode.window.createOutputChannel(
    "Stationeers Toolkit",
    {
      log: true,
    },
  );
  context.subscriptions.push(outputChannel, coverageDecoration);
  registerLiveExplorer(context);
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
  const environmentDebugOverlays = new EnvironmentDebugOverlayService();
  context.subscriptions.push(environmentDebugOverlays);
  const testingService = registerIc10Testing(context, outputChannel);
  registerSimulationProgramRenameTracking(context, () => testingService.refresh());
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
      new Ic10EnvironmentEditorProvider(
        context,
        simulationLaunchService,
        environmentDebugOverlays,
        async (document) => {
          if (!client) {
            throw new Error("The IC10 language server is not ready.");
          }
          return new EnvironmentProposalService(client).preview(document);
        },
      ),
      {
        supportsMultipleEditorsPerDocument: false,
        webviewOptions: { retainContextWhenHidden: true },
      },
    ),
    vscode.window.registerCustomEditorProvider(
      Ic10ScenarioTestEditorProvider.viewType,
      new Ic10ScenarioTestEditorProvider(context, testingService),
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
    vscode.commands.registerCommand(
      "ic10.createEnvironmentFromTemplate",
      () => createEnvironmentFromTemplate(context),
    ),
    vscode.commands.registerCommand("ic10.createScenarioTest", createScenarioTest),
    vscode.commands.registerCommand(
      "ic10.configureLuaIntegration",
      () => configureLuaIntegration(context),
    ),
    vscode.commands.registerCommand(
      "ic10.filterTrace",
      (value: { targetId?: string } | undefined) =>
        value?.targetId
          ? stateViewProvider.filterTrace(value.targetId)
          : undefined,
    ),
    vscode.commands.registerCommand("ic10.stepWorldTick", () =>
      stateViewProvider.stepWorldTick(),
    ),
    vscode.commands.registerCommand("ic10.hotReload", async () => {
      const session = vscode.debug.activeDebugSession;
      if (session?.type !== debugType) {
        void vscode.window.showInformationMessage(
          "Pause an IC10 debug session before hot reloading.",
        );
        return;
      }
      const choice = await vscode.window.showQuickPick(
        [
          {
            label: "Preserve CPU and world state",
            description: "Compile new source and continue from the current state",
            preserveState: true,
          },
          {
            label: "Reset to launch state",
            description: "Reload the original scenario and test configuration",
            preserveState: false,
          },
        ],
        { placeHolder: "Choose explicit IC10 hot-reload state semantics" },
      );
      if (!choice) {
        return;
      }
      try {
        await session.customRequest("ic10/hotReload", {
          preserveState: choice.preserveState,
        });
      } catch (error) {
        void vscode.window.showErrorMessage(
          `IC10 hot reload failed: ${String(error)}`,
        );
      }
    }),
    vscode.commands.registerCommand("ic10.exportTrace", async () => {
      const session = vscode.debug.activeDebugSession;
      if (session?.type !== debugType) {
        void vscode.window.showInformationMessage(
          "Start an IC10 debug session before exporting a trace.",
        );
        return;
      }
      const uri = await vscode.window.showSaveDialog({
        filters: { "IC10 debug trace": ["ic10trace.json"] },
        defaultUri: vscode.Uri.joinPath(
          vscode.workspace.workspaceFolders?.[0]?.uri ??
            vscode.Uri.file(process.cwd()),
          "debug.ic10trace.json",
        ),
      });
      if (!uri) {
        return;
      }
      try {
        await session.customRequest("ic10/exportTrace", { path: uri.fsPath });
        void vscode.window.showInformationMessage(
          `Exported redacted IC10 trace to ${uri.fsPath}.`,
        );
      } catch (error) {
        void vscode.window.showErrorMessage(
          `Could not export IC10 trace: ${String(error)}`,
        );
      }
    }),
    vscode.commands.registerCommand("ic10.showTraceSummary", async () => {
      const session = vscode.debug.activeDebugSession;
      if (session?.type !== debugType) {
        void vscode.window.showInformationMessage(
          "Start an IC10 debug session before viewing trace analysis.",
        );
        return;
      }
      try {
        const trace = (await session.customRequest("ic10/getTrace", {
          summaryOnly: true,
        })) as {
          coverage?: Record<string, number[]>;
        };
        for (const editor of vscode.window.visibleTextEditors) {
          const lines =
            trace.coverage?.[
              path
                .normalize(editor.document.uri.fsPath)
                .replaceAll("\\", "/")
                .toLowerCase()
            ] ?? [];
          editor.setDecorations(
            coverageDecoration,
            lines.map(
              (line) =>
                new vscode.Range(
                  Math.max(0, line - 1),
                  0,
                  Math.max(0, line - 1),
                  0,
                ),
            ),
          );
        }
        const document = await vscode.workspace.openTextDocument({
          language: "json",
          content: `${JSON.stringify(trace, null, 2)}\n`,
        });
        await vscode.window.showTextDocument(document, {
          preview: true,
          viewColumn: vscode.ViewColumn.Beside,
        });
      } catch (error) {
        void vscode.window.showErrorMessage(
          `Could not read IC10 trace analysis: ${String(error)}`,
        );
      }
    }),
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

async function warnForObsoleteWorkspaceFiles(): Promise<void> {
  const obsolete = await vscode.workspace.findFiles(
    "**/*.{ic10sim.json,ic10test.json,ic10sim.layout.json,stationeerssim.json,stationeerstest.json,stationeerssim.layout.json}",
    "**/{.git,node_modules,target}/**",
    20,
  );
  if (obsolete.length === 0) return;
  const obsoleteNames = obsolete.map((file) => path.basename(file.fsPath));
  const suffixes = [
    obsoleteNames.some((name) => name.endsWith("test.json"))
      ? "legacy test"
      : undefined,
    obsoleteNames.some((name) => name.endsWith("sim.json"))
      ? "legacy simulation"
      : undefined,
    obsoleteNames.some((name) => name.endsWith(".layout.json"))
      ? "legacy layout"
      : undefined,
  ].filter((suffix): suffix is string => suffix !== undefined);
  void vscode.window.showWarningMessage(
    `This workspace contains obsolete simulation files (${suffixes.join(", ")}). They are no longer valid; please migrate to the new .icsim, .ictest and .icsimlayout (it may be best to create them from scratch). New releases will avoid breaking changes or provide migration paths.`,
  );
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
  const fileWatcher = vscode.workspace.createFileSystemWatcher("**/*.{ic10,lua}");
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
    "Stationeers Toolkit",
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

function warnForLegacyLuaExtension(): void {
  const oldExtension = vscode.extensions.getExtension(
    "OrbitalFoundryModdingCrew.stationeers-lua",
  );
  if (oldExtension && shouldWarnForLegacyLuaExtension([oldExtension.id])) {
    void vscode.window.showWarningMessage(
      "StationeersLua can be used alongside Stationeers Toolkit, but avoid editing the same code in both extensions at once.",
    );
  }
}

/** Preview and explicitly apply/restore optional sumneko.lua runtime settings.
 * The annotation library itself is registered automatically during activation. */
function getLuaLibraryPaths(context: vscode.ExtensionContext): string[] {
  const stationeersLua = vscode.extensions.getExtension(
    "OrbitalFoundryModdingCrew.stationeers-lua",
  );
  if (stationeersLua) {
    return [
      path.join(stationeersLua.extensionUri.fsPath, "library"),
      vscode.Uri.joinPath(
        context.extensionUri,
        "assets",
        "lua",
        "stationeers-toolkit",
      ).fsPath,
    ];
  }
  return [
    vscode.Uri.joinPath(
      context.extensionUri,
      "assets",
      "lua",
      "stationeers-v1",
    ).fsPath,
    vscode.Uri.joinPath(
      context.extensionUri,
      "assets",
      "lua",
      "stationeers-v1",
      "stationeerslua-0.2.3",
    ).fsPath,
  ];
}

async function configureLuaIntegration(
  context: vscode.ExtensionContext,
): Promise<void> {
  warnForLegacyLuaExtension();
  const config = vscode.workspace.getConfiguration("Lua");
  const libraryPaths = getLuaLibraryPaths(context);
  const existingLibrary = config.get<unknown>("workspace.library");
  const library = Array.isArray(existingLibrary)
    ? existingLibrary.map(String)
    : existingLibrary && typeof existingLibrary === "object"
      ? { ...(existingLibrary as Record<string, unknown>) }
      : [];
  const runtimeVersion = config.get<unknown>("runtime.version");
  const saved = context.globalState.get<{
    library: unknown;
    runtimeVersion: unknown;
  }>("luaIntegration.previousSettings");
  const action = await vscode.window.showInformationMessage(
    [
      "Lua integration preview",
      `Add Stationeers Lua annotations: ${libraryPaths.join(", ")}`,
      `Set Lua runtime: ${runtimeVersion === undefined ? "Lua 5.2" : "leave existing value unchanged"}`,
      saved ? "A previous configuration can be restored." : "",
    ].filter(Boolean).join("\n"),
    { modal: true },
    "Apply Lua Integration",
    ...(saved ? ["Restore Previous Settings"] : []),
  );
  if (action === "Restore Previous Settings" && saved) {
    await config.update("workspace.library", saved.library, vscode.ConfigurationTarget.Global);
    await config.update("runtime.version", saved.runtimeVersion, vscode.ConfigurationTarget.Global);
    await context.globalState.update("luaIntegration.previousSettings", undefined);
    void vscode.window.showInformationMessage("Restored the Lua settings saved before Stationeers integration.");
    return;
  }
  if (action !== "Apply Lua Integration") {
    return;
  }
  await context.globalState.update("luaIntegration.previousSettings", {
    library: existingLibrary,
    runtimeVersion,
  });
  if (Array.isArray(library)) {
    const additions = libraryPaths.filter((candidate) => !library.includes(candidate));
    if (additions.length > 0) {
      await config.update("workspace.library", [...library, ...additions], vscode.ConfigurationTarget.Global);
    }
  } else {
    const additions = libraryPaths.filter((candidate) => !(candidate in library));
    if (additions.length > 0) {
      await config.update(
        "workspace.library",
        { ...library, ...Object.fromEntries(additions.map((candidate) => [candidate, true])) },
        vscode.ConfigurationTarget.Global,
      );
    }
  }
  if (runtimeVersion === undefined) {
    await config.update("runtime.version", "Lua 5.2", vscode.ConfigurationTarget.Global);
  }
}

async function ensureLuaAnnotationLibrary(
  context: vscode.ExtensionContext,
): Promise<void> {
  const libraryPaths = getLuaLibraryPaths(context);
  const config = vscode.workspace.getConfiguration("Lua");
  const existing = config.get<unknown>("workspace.library");
  const target = vscode.workspace.workspaceFolders?.length
    ? vscode.ConfigurationTarget.Workspace
    : vscode.ConfigurationTarget.Global;
  const includesPath = (values: readonly string[], candidate: string): boolean =>
    values.some((value) => path.normalize(value).toLowerCase() === path.normalize(candidate).toLowerCase());
  if (Array.isArray(existing)) {
    const library = existing.map(String);
    const additions = libraryPaths.filter((candidate) => !includesPath(library, candidate));
    if (additions.length > 0) {
      await config.update(
        "workspace.library",
        [...library, ...additions],
        target,
      );
    }
    return;
  }
  if (existing && typeof existing === "object") {
    const library = { ...(existing as Record<string, unknown>) };
    const existingPaths = Object.keys(library);
    const additions = libraryPaths.filter((candidate) => !includesPath(existingPaths, candidate));
    if (additions.length > 0) {
      await config.update(
        "workspace.library",
        {
          ...library,
          ...Object.fromEntries(additions.map((candidate) => [candidate, true])),
        },
        target,
      );
    }
    return;
  }
  await config.update("workspace.library", libraryPaths, target);
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
