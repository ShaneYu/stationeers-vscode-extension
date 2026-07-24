import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";

import { SimulationLaunchService } from "./simulationLaunch";

export const debugType = "ic10";

export class Ic10DebugAdapterFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  public constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly output: vscode.LogOutputChannel,
  ) {}

  public createDebugAdapterDescriptor(): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    const executable = resolveDebugAdapter(this.context, this.output);
    if (!executable) {
      void vscode.window.showErrorMessage(
        "The IC10 debug adapter was not found. Build it with `cargo build -p ic10-dap`, or set `ic10.debugAdapter.path`.",
      );
      return undefined;
    }
    this.output.info(`Starting IC10 debug adapter: ${executable}`);
    return new vscode.DebugAdapterExecutable(executable);
  }
}

export class Ic10DebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  public constructor(
    private readonly launchService: SimulationLaunchService,
  ) {}

  public async provideDebugConfigurations(): Promise<vscode.DebugConfiguration[]> {
    const targets = await this.launchService.allTargets();
    return targets.map((target) => this.launchService.configuration(target));
  }

  public async resolveDebugConfiguration(
    folder: vscode.WorkspaceFolder | undefined,
    configuration: vscode.DebugConfiguration,
  ): Promise<vscode.DebugConfiguration | undefined> {
    const isEmptyConfiguration =
      !configuration.type && !configuration.request && !configuration.name;
    if (isEmptyConfiguration || this.launchService.hasActiveLaunchContext()) {
      const target = await this.launchService.resolveF5Target();
      return target
        ? this.launchService.configuration(target)
        : undefined;
    }
    if (!configuration.scenario && folder) {
      const relative = new vscode.RelativePattern(folder, "*.ic10sim.json");
      const scenarios = await vscode.workspace.findFiles(relative, undefined, 2);
      if (scenarios.length === 1) {
        configuration.scenario = scenarios[0]?.fsPath;
      }
    }
    if (!configuration.scenario) {
      void vscode.window.showErrorMessage(
        "Choose an IC10 simulation environment, or run “IC10: Create Simulation Environment”.",
      );
      return undefined;
    }
    return configuration;
  }

  public resolveDebugConfigurationWithSubstitutedVariables(
    _folder: vscode.WorkspaceFolder | undefined,
    configuration: vscode.DebugConfiguration,
  ): vscode.DebugConfiguration | undefined {
    const scenario = String(configuration.scenario ?? "");
    if (!scenario || !fs.existsSync(scenario)) {
      void vscode.window.showErrorMessage(
        `IC10 simulation environment does not exist: ${scenario}`,
      );
      return undefined;
    }
    return configuration;
  }
}

export function resolveDebugAdapter(
  context: vscode.ExtensionContext,
  output: vscode.LogOutputChannel,
): string | undefined {
  const configuredPath = vscode.workspace
    .getConfiguration("ic10")
    .get<string>("debugAdapter.path", "")
    .trim();
  if (configuredPath) {
    if (fs.existsSync(configuredPath)) {
      return configuredPath;
    }
    output.warn(
      `Configured ic10.debugAdapter.path does not exist: ${configuredPath}`,
    );
  }

  const executableName =
    process.platform === "win32" ? "ic10-dap.exe" : "ic10-dap";
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
  const bundled = vscode.Uri.joinPath(
    context.extensionUri,
    "server",
    `${process.platform}-${process.arch}`,
    executableName,
  ).fsPath;
  if (fs.existsSync(bundled)) {
    return bundled;
  }
  return fs.existsSync(development) ? development : undefined;
}
