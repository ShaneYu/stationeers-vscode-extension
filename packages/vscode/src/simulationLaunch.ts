import * as path from "node:path";

import * as vscode from "vscode";
import { SIM_GLOB } from "./workspaceFormats.ts";

export interface SimulationLaunchTarget {
  readonly scenario: vscode.Uri;
  readonly icId: string;
  readonly icName: string;
  readonly program: vscode.Uri;
}

interface ScenarioShape {
  readonly programs?: readonly {
    readonly id?: string;
    readonly path?: string;
    readonly language?: "ic10" | "lua";
  }[];
  readonly devices?: readonly {
    readonly id?: string;
    readonly name?: string;
    readonly programId?: string;
    readonly ic?: {
      readonly program?: string;
      readonly enabled?: boolean;
    };
  }[];
}

export class SimulationLaunchService {
  private activeEnvironment: vscode.Uri | undefined;
  private readonly selectedIcs = new Map<string, string | undefined>();

  public setEnvironmentActive(uri: vscode.Uri, active: boolean): void {
    if (active) {
      this.activeEnvironment = uri;
    } else if (this.activeEnvironment?.toString() === uri.toString()) {
      this.activeEnvironment = undefined;
    }
  }

  public setSelectedIc(uri: vscode.Uri, icId: string | undefined): void {
    this.selectedIcs.set(uri.toString(), icId);
  }

  public hasActiveLaunchContext(): boolean {
    return (
      this.activeEnvironment !== undefined ||
      vscode.window.activeTextEditor?.document.languageId === "ic10"
    );
  }

  public async resolveF5Target(): Promise<SimulationLaunchTarget | undefined> {
    if (this.activeEnvironment) {
      const targets = await this.targetsForScenario(this.activeEnvironment);
      const selected = this.selectedIcs.get(this.activeEnvironment.toString());
      const target = targets.find((candidate) => candidate.icId === selected);
      if (target) {
        return this.prepareTarget(target);
      }
      void vscode.window.showErrorMessage(
        "Select an IC housing under Devices and ICs before starting the simulation.",
      );
      return undefined;
    }

    const activeDocument = vscode.window.activeTextEditor?.document;
    if (activeDocument?.languageId === "ic10" || activeDocument?.languageId === "lua") {
      const matches = (await this.allTargets()).filter(
        (target) =>
          normalizePath(target.program.fsPath) ===
          normalizePath(activeDocument.uri.fsPath),
      );
      if (matches.length === 1) {
        return this.prepareTarget(matches[0]!);
      }
      if (matches.length > 1) {
        const picked = await this.pickTarget(
          matches,
          "Choose which simulated IC should run this program",
        );
        return picked ? this.prepareTarget(picked) : undefined;
      }
      void vscode.window.showErrorMessage(
        "The active Stationeers program is not referenced by a simulation environment. Run “IC10: Create Simulation Environment” first.",
      );
      return undefined;
    }

    const targets = await this.allTargets();
    if (targets.length === 1) {
      return this.prepareTarget(targets[0]!);
    }
    if (targets.length > 1) {
      const picked = await this.pickTarget(
        targets,
        "Choose an IC10 simulation and housing",
      );
      return picked ? this.prepareTarget(picked) : undefined;
    }
    void vscode.window.showErrorMessage(
      "No simulation environment contains a runnable IC10 program.",
    );
    return undefined;
  }

  public async startEnvironment(
    scenario: vscode.Uri,
    icId: string,
  ): Promise<boolean> {
    this.setSelectedIc(scenario, icId);
    const targets = await this.targetsForScenario(scenario);
    const target = targets.find((candidate) => candidate.icId === icId);
    if (!target) {
      void vscode.window.showErrorMessage(
        `The simulation does not contain a runnable IC named “${icId}”.`,
      );
      return false;
    }
    return this.startTarget(target);
  }

  public async startTarget(target: SimulationLaunchTarget): Promise<boolean> {
    const prepared = await this.prepareTarget(target);
    if (!prepared) {
      return false;
    }
    const folder = vscode.workspace.getWorkspaceFolder(target.scenario);
    return vscode.debug.startDebugging(folder, this.configuration(target));
  }

  public async prepareTarget(
    target: SimulationLaunchTarget,
  ): Promise<SimulationLaunchTarget | undefined> {
    const scenarioTargets = await this.targetsForScenario(target.scenario);
    const uris = [
      target.scenario,
      ...scenarioTargets.map((candidate) => candidate.program),
    ];
    for (const uri of uris) {
      const document = vscode.workspace.textDocuments.find(
        (candidate) => candidate.uri.toString() === uri.toString(),
      );
      if (document?.isDirty && !(await document.save())) {
        void vscode.window.showErrorMessage(
          `Save ${path.basename(uri.fsPath)} before starting IC10 debugging.`,
        );
        return undefined;
      }
    }
    return target;
  }

  public configuration(
    target: SimulationLaunchTarget,
  ): vscode.DebugConfiguration {
    return {
      type: "ic10",
      request: "launch",
      name: `IC10: ${target.icName}`,
      scenario: target.scenario.fsPath,
      focusIc: target.icId,
      stopOnEntry: true,
      enableHistory: true,
    };
  }

  public async allTargets(): Promise<SimulationLaunchTarget[]> {
    const scenarios = await vscode.workspace.findFiles(
      SIM_GLOB,
      "**/{node_modules,target,dist}/**",
      200,
    );
    const nested = await Promise.all(
      scenarios.map((scenario) => this.targetsForScenario(scenario)),
    );
    return nested.flat();
  }

  public async targetsForScenario(
    scenario: vscode.Uri,
  ): Promise<SimulationLaunchTarget[]> {
    const parsed = await readScenario(scenario);
    if (!parsed) {
      return [];
    }
    const base = path.dirname(scenario.fsPath);
    const programs = new Map(
      (parsed.programs ?? []).map((program) => [program.id, program]),
    );
    return (parsed.devices ?? []).flatMap((device) => {
      const canonical = device.programId ? programs.get(device.programId) : undefined;
      const program = canonical?.language === "ic10" ? canonical.path : device.ic?.program;
      const id = device.id;
      if (!program || !id || device.ic?.enabled === false) {
        return [];
      }
      const resolved = path.isAbsolute(program)
        ? program
        : path.resolve(base, program);
      return [
        {
          scenario,
          icId: id,
          icName: device.name || id,
          program: vscode.Uri.file(resolved),
        },
      ];
    });
  }

  private async pickTarget(
    targets: readonly SimulationLaunchTarget[],
    placeHolder: string,
  ): Promise<SimulationLaunchTarget | undefined> {
    const picked = await vscode.window.showQuickPick(
      targets.map((target) => ({
        label: target.icName,
        description: path.basename(target.scenario.fsPath),
        detail: vscode.workspace.asRelativePath(target.program, false),
        target,
      })),
      { placeHolder },
    );
    return picked?.target;
  }
}

async function readScenario(
  uri: vscode.Uri,
): Promise<ScenarioShape | undefined> {
  try {
    const open = vscode.workspace.textDocuments.find(
      (document) => document.uri.toString() === uri.toString(),
    );
    const source = open
      ? open.getText()
      : Buffer.from(await vscode.workspace.fs.readFile(uri)).toString("utf8");
    return JSON.parse(source) as ScenarioShape;
  } catch {
    return undefined;
  }
}

function normalizePath(value: string): string {
  const normalized = path.normalize(value);
  return process.platform === "win32"
    ? normalized.toLocaleLowerCase()
    : normalized;
}
