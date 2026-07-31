import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

import { resolveScenarioProgramPath } from "./scenarioUri";
import {
  isSimulationPath,
  isStationeersProgramPath,
  SIM_GLOB,
} from "./workspaceFormats.ts";

export interface EnvironmentTarget {
  readonly scenarioUri: string;
  readonly icId: string;
  readonly deviceId?: string;
  readonly property?: string;
}

interface ContextStatusItem {
  readonly scenarioUri: string;
  readonly icId: string;
  readonly label: string;
}

interface ContextStatus {
  readonly uri: string;
  readonly contexts: readonly ContextStatusItem[];
  readonly active?: ContextStatusItem;
  readonly ambiguous: boolean;
}

export function resolveScenarioProgram(
  scenarioUri: vscode.Uri,
  program: string,
): vscode.Uri {
  if (/^[a-z][a-z0-9+.-]*:/i.test(program)) {
    return vscode.Uri.parse(program, true);
  }
  const resolved = resolveScenarioProgramPath(scenarioUri, program);
  return scenarioUri.with({ path: resolved.path });
}

export class EnvironmentIntelligence implements vscode.Disposable {
  private readonly statuses = new Map<string, ContextStatus>();
  private readonly restoredSelections = new Set<string>();
  private scenarioVersion = 0;
  private readonly statusBar: vscode.StatusBarItem;
  private readonly disposables: vscode.Disposable[] = [];

  public constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly client: LanguageClient,
    private readonly reveal: (target: EnvironmentTarget) => Promise<void>,
  ) {
    this.statusBar = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Right,
      91,
    );
    this.statusBar.command = "ic10.selectEnvironmentContext";
    this.disposables.push(
      this.statusBar,
      client.onNotification("ic10/contextStatus", (status: ContextStatus) => {
        this.statuses.set(status.uri, status);
        void this.restoreSelection(status);
        this.updateStatus();
      }),
      vscode.window.onDidChangeActiveTextEditor(() => this.updateStatus()),
      vscode.commands.registerCommand(
        "ic10.selectEnvironmentContext",
        () => this.selectContext(),
      ),
      vscode.commands.registerCommand(
        "ic10.openEnvironmentTarget",
        (target: EnvironmentTarget) => this.reveal(target),
      ),
    );
  }

  public async start(): Promise<void> {
    const watcher = vscode.workspace.createFileSystemWatcher(
      SIM_GLOB,
    );
    this.disposables.push(
      watcher,
      watcher.onDidCreate((uri) => void this.publishScenario(uri)),
      watcher.onDidChange((uri) => void this.publishScenario(uri)),
      watcher.onDidDelete((uri) => void this.removeScenario(uri)),
      vscode.workspace.onDidRenameFiles((event) => {
        for (const file of event.files) {
          if (isSimulationPath(file.oldUri.path)) {
            void this.removeScenario(file.oldUri);
          }
          if (isSimulationPath(file.newUri.path)) {
            void this.publishScenario(file.newUri);
          }
        }
      }),
    );
    const scenarios = await vscode.workspace.findFiles(
      SIM_GLOB,
      "**/{node_modules,target,dist}/**",
      500,
    );
    await Promise.all(scenarios.map((uri) => this.publishScenario(uri)));
  }

  public dispose(): void {
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
  }

  private async publishScenario(uri: vscode.Uri): Promise<void> {
    let source: string;
    try {
      source = Buffer.from(await vscode.workspace.fs.readFile(uri)).toString(
        "utf8",
      );
    } catch {
      return;
    }
    let parsed: {
      programs?: readonly { id?: string; path?: string; language?: "ic10" | "lua" }[];
      devices?: readonly {
        ic?: { program?: string };
        programId?: string;
      }[];
    };
    try {
      parsed = JSON.parse(source) as typeof parsed;
    } catch {
      // The JSON language service owns syntax errors, while the LSP must still
      // invalidate the last valid context immediately.
      await this.client.sendNotification("ic10/scenarioChanged", {
        scenarioUri: uri.toString(true),
        version: ++this.scenarioVersion,
        source,
        resolvedPrograms: {},
      });
      return;
    }
    const resolvedPrograms: Record<string, string> = {};
    for (const program of parsed.programs ?? []) {
      if (program.path && isStationeersProgramPath(program.path)) {
        // ScenarioIndex asks the host to resolve the program path stored in the
        // canonical program entry, not that entry's ID.
        resolvedPrograms[program.path] = resolveScenarioProgram(uri, program.path).toString(true);
      }
    }
    for (const device of parsed.devices ?? []) {
      const program = device.ic?.program;
      if (program) {
        resolvedPrograms[program] = resolveScenarioProgram(
          uri,
          program,
        ).toString(true);
      }
    }
    await this.client.sendNotification("ic10/scenarioChanged", {
      scenarioUri: uri.toString(true),
      version: ++this.scenarioVersion,
      source,
      resolvedPrograms,
    });
  }

  private async removeScenario(uri: vscode.Uri): Promise<void> {
    await this.client.sendNotification("ic10/scenarioChanged", {
      scenarioUri: uri.toString(true),
      version: ++this.scenarioVersion,
    });
  }

  private updateStatus(): void {
    const editor = vscode.window.activeTextEditor;
    if (editor?.document.languageId !== "ic10") {
      this.statusBar.hide();
      return;
    }
    const status = this.statuses.get(editor.document.uri.toString(true));
    if (!status || status.contexts.length === 0) {
      this.statusBar.text = "$(debug-disconnect) IC10: no environment";
      this.statusBar.tooltip =
        "Document-only intelligence is active. No simulation environment references this program.";
    } else if (status.ambiguous) {
      this.statusBar.text = `$(warning) IC10: choose environment (${status.contexts.length})`;
      this.statusBar.tooltip =
        "Multiple simulation contexts reference this program. Select one explicitly to enable environment-aware intelligence.";
    } else {
      this.statusBar.text = `$(circuit-board) ${status.active?.label ?? "IC10 environment"}`;
      this.statusBar.tooltip =
        "Active simulation environment and IC housing. Click to switch or deselect.";
    }
    this.statusBar.show();
  }

  private async selectContext(): Promise<void> {
    const editor = vscode.window.activeTextEditor;
    if (editor?.document.languageId !== "ic10") {
      return;
    }
    const programUri = editor.document.uri.toString(true);
    const status = this.statuses.get(programUri);
    if (!status || status.contexts.length === 0) {
      void vscode.window.showInformationMessage(
        "No simulation environment references this IC10 program.",
      );
      return;
    }
    const items = [
      ...status.contexts.map((item) => ({
        label: item.label,
        description:
          status.active?.scenarioUri === item.scenarioUri &&
          status.active.icId === item.icId
            ? "Active"
            : undefined,
        item,
      })),
      {
        label: "$(circle-slash) Document only",
        description: "Disable environment-derived intelligence",
        item: undefined,
      },
    ];
    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: "Select the simulation environment and IC housing",
      matchOnDescription: true,
    });
    if (!picked) {
      return;
    }
    await this.client.sendNotification("ic10/selectContext", {
      programUri,
      scenarioUri: picked.item?.scenarioUri,
      icId: picked.item?.icId,
    });
    await this.context.workspaceState.update(
      `ic10.context.${programUri}`,
      picked.item,
    );
  }

  private async restoreSelection(status: ContextStatus): Promise<void> {
    if (
      !status.ambiguous ||
      this.restoredSelections.has(status.uri)
    ) {
      return;
    }
    this.restoredSelections.add(status.uri);
    const saved = this.context.workspaceState.get<ContextStatusItem>(
      `ic10.context.${status.uri}`,
    );
    if (
      !saved ||
      !status.contexts.some(
        (context) =>
          context.scenarioUri === saved.scenarioUri &&
          context.icId === saved.icId,
      )
    ) {
      return;
    }
    await this.client.sendNotification("ic10/selectContext", {
      programUri: status.uri,
      scenarioUri: saved.scenarioUri,
      icId: saved.icId,
    });
  }
}
