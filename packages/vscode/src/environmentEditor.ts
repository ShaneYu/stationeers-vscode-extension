import * as fs from "node:fs/promises";
import * as path from "node:path";

import * as vscode from "vscode";

import { SimulationLaunchService } from "./simulationLaunch";
import type { EnvironmentTarget } from "./environmentIntelligence";
import { resolveScenarioProgramPath } from "./scenarioUri";
import {
  applyFragmentImport,
  buildTopologyGraph,
  exportTopologyFragment,
  parseEnvironmentLayoutSidecar,
  parseTopologyFragment,
  previewFragmentImport,
  type EnvironmentLayoutSidecar,
  type EnvironmentScenario,
  type FragmentImportPreview,
} from "./environmentTopologyModel";
import {
  buildEnvironmentTopologyView,
  duplicateTopologySelection,
  savedTopologyLayout,
  topologyLayoutFilename,
} from "./environmentTopologyController";
import {
  EnvironmentDebugOverlayService,
  type TopologyRuntimeMessage,
} from "./environmentDebugOverlay";
import type {
  EnvironmentProposalPreview,
} from "./environmentProposalModel";
import { scenarioFromEnvironmentProposal } from "./environmentProposalApplyModel";

interface DeviceMetadata {
  readonly prefabName: string;
  readonly prefabHash: number;
  readonly displayName: string;
  readonly description?: string;
  readonly image?: string;
  readonly modes?: Record<string, unknown>;
  readonly memory?: {
    readonly size?: number;
    readonly access?: unknown;
  };
  readonly logicTypes: Record<string, { read: boolean; write: boolean }>;
  readonly slots: Record<
    string,
    {
      name: string;
      logicTypes: Record<string, { read: boolean; write: boolean }>;
    }
  >;
  readonly connections: readonly { type: unknown; role: unknown }[];
}

interface DeviceCatalogFile {
  readonly devices: Record<string, DeviceMetadata>;
  readonly otherLogicables: Record<string, DeviceMetadata>;
}

type DeviceCatalog = Record<string, DeviceMetadata>;

interface EditorReference {
  readonly catalog: DeviceCatalog;
  readonly items: Record<string, ItemMetadata>;
  readonly logicHelp: Record<string, string>;
  readonly slotHelp: Record<string, string>;
}

interface ItemMetadata {
  readonly prefabName: string;
  readonly prefabHash: number;
  readonly displayName: string;
  readonly kind: string;
  readonly slotClass?: unknown;
  readonly slotClassValue?: number;
  readonly sortingClass?: unknown;
  readonly sortingClassValue?: number;
  readonly maxQuantity?: unknown;
}

export class Ic10EnvironmentEditorProvider
  implements vscode.CustomTextEditorProvider
{
  public static readonly viewType = "ic10.environment";
  private static readonly pendingTargets = new Map<string, EnvironmentTarget>();

  public static queueReveal(target: EnvironmentTarget): void {
    this.pendingTargets.set(target.scenarioUri, target);
  }

  private readonly reference: Promise<EditorReference>;

  public constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly launchService: SimulationLaunchService,
    private readonly debugOverlays: EnvironmentDebugOverlayService,
    private readonly proposeEnvironment: (
      document: vscode.TextDocument,
    ) => Promise<EnvironmentProposalPreview>,
  ) {
    this.reference = loadReference(context);
  }

  public async resolveCustomTextEditor(
    document: vscode.TextDocument,
    panel: vscode.WebviewPanel,
  ): Promise<void> {
    panel.webview.options = {
      enableScripts: true,
      localResourceRoots: [
        vscode.Uri.joinPath(this.context.extensionUri, "assets", "devices"),
      ],
    };
    panel.webview.html = environmentHtml(panel.webview);
    this.launchService.setEnvironmentActive(document.uri, panel.active);
    let currentLayout: EnvironmentLayoutSidecar | undefined;
    let pendingImport: FragmentImportPreview | undefined;
    let pendingProposal: EnvironmentProposalPreview | undefined;
    const debugOverlaySubscription = this.debugOverlays.subscribe(
      document.uri,
      (message: TopologyRuntimeMessage) => {
        void panel.webview.postMessage({
          type: "topologyRuntime",
          runtime: message,
        });
      },
    );

    const update = async (): Promise<void> => {
      let scenario: EnvironmentScenario;
      try {
        scenario = JSON.parse(document.getText()) as EnvironmentScenario;
      } catch (error) {
        await panel.webview.postMessage({
          type: "parseError",
          message: String(error),
        });
        return;
      }
      const assetBase = panel.webview.asWebviewUri(
        vscode.Uri.joinPath(this.context.extensionUri, "assets", "devices"),
      );
      const reference = await this.reference;
      currentLayout = await readTopologyLayout(document.uri, scenario, reference.catalog);
      await panel.webview.postMessage({
        type: "update",
        scenario,
        ...reference,
        topology: buildEnvironmentTopologyView(
          scenario,
          reference.catalog,
          currentLayout,
        ),
        programs: await findPrograms(document.uri),
        assetBase: assetBase.toString(true),
      });
      const target = Ic10EnvironmentEditorProvider.pendingTargets.get(
        document.uri.toString(true),
      );
      if (target) {
        Ic10EnvironmentEditorProvider.pendingTargets.delete(
          document.uri.toString(true),
        );
        await panel.webview.postMessage({ type: "reveal", target });
      }
    };
    const writeScenario = async (scenario: unknown): Promise<boolean> => {
      const replacement = `${JSON.stringify(scenario, null, 2)}\n`;
      if (replacement === document.getText()) {
        return true;
      }
      const edit = new vscode.WorkspaceEdit();
      edit.replace(
        document.uri,
        new vscode.Range(
          document.positionAt(0),
          document.positionAt(document.getText().length),
        ),
        replacement,
      );
      return vscode.workspace.applyEdit(edit);
    };

    const changeSubscription = vscode.workspace.onDidChangeTextDocument(
      (event) => {
        if (event.document.uri.toString() === document.uri.toString()) {
          void update();
        }
      },
    );
    const viewSubscription = panel.onDidChangeViewState((event) => {
      this.launchService.setEnvironmentActive(
        document.uri,
        event.webviewPanel.active,
      );
    });
    const programWatcher = vscode.workspace.createFileSystemWatcher("**/*.ic10");
    const refreshPrograms = (): void => {
      void findPrograms(document.uri).then((programs) =>
        panel.webview.postMessage({ type: "programs", programs }),
      );
    };
    const programCreateSubscription = programWatcher.onDidCreate(refreshPrograms);
    const programDeleteSubscription = programWatcher.onDidDelete(refreshPrograms);
    panel.onDidDispose(() => {
      changeSubscription.dispose();
      viewSubscription.dispose();
      programCreateSubscription.dispose();
      programDeleteSubscription.dispose();
      programWatcher.dispose();
      debugOverlaySubscription.dispose();
      this.launchService.setEnvironmentActive(document.uri, false);
      this.launchService.setSelectedIc(document.uri, undefined);
    });
    panel.webview.onDidReceiveMessage(
      async (message: {
        type: string;
        scenario?: unknown;
        icId?: string;
        deviceId?: string;
        program?: string;
        positions?: Record<string, { x: number; y: number }>;
        viewport?: { x: number; y: number; zoom: number };
        topologySelection?: { kind: "device" | "network"; id: string };
        action?: "source" | "variables" | "watch" | "trace";
        targetId?: string;
        selectedPrefabs?: Record<string, string>;
        confirmAssumptions?: boolean;
      }) => {
        if (message.type === "ready") {
          await update();
          return;
        }
        if (message.type === "selectionChanged") {
          this.launchService.setSelectedIc(document.uri, message.icId);
          return;
        }
        if (message.type === "startDebug" && message.icId) {
          if (message.scenario && !(await writeScenario(message.scenario))) {
            void vscode.window.showErrorMessage(
              "Could not save the simulation changes before debugging.",
            );
            return;
          }
          await this.launchService.startEnvironment(document.uri, message.icId);
          return;
        }
        if (message.type === "browseProgram" && message.deviceId) {
          const picked = await vscode.window.showOpenDialog({
            canSelectFiles: true,
            canSelectFolders: false,
            canSelectMany: false,
            defaultUri: vscode.Uri.file(path.dirname(document.uri.fsPath)),
            filters: { "IC10 programs": ["ic10"] },
            openLabel: "Use IC10 Program",
          });
          const selectedProgram = picked?.[0];
          if (selectedProgram) {
            await panel.webview.postMessage({
              type: "programSelected",
              deviceId: message.deviceId,
              program: programPathForScenario(
                document.uri.fsPath,
                selectedProgram.fsPath,
              ),
            });
          }
          return;
        }
        if (message.type === "openProgram" && message.program) {
          const programUri = /^[a-z][a-z0-9+.-]*:/i.test(message.program)
            ? vscode.Uri.parse(message.program, true)
            : document.uri.with({
                path: resolveScenarioProgramPath(
                  document.uri,
                  message.program,
                ).path,
              });
          await vscode.window.showTextDocument(
            await vscode.workspace.openTextDocument(programUri),
          );
          return;
        }
        if (message.type === "openJson") {
          await vscode.commands.executeCommand(
            "vscode.openWith",
            document.uri,
            "default",
          );
          return;
        }
        if (message.type === "requestEnvironmentProposal") {
          const program = await pickProposalProgram(document.uri);
          if (!program) {
            return;
          }
          try {
            pendingProposal = await this.proposeEnvironment(
              await vscode.workspace.openTextDocument(program),
            );
            await panel.webview.postMessage({
              type: "environmentProposalPreview",
              preview: pendingProposal,
              destination: document.uri.toString(true),
              destinationEmpty:
                (JSON.parse(document.getText()) as EnvironmentScenario).networks
                  .length === 0 &&
                (JSON.parse(document.getText()) as EnvironmentScenario).devices
                  .length === 0,
            });
          } catch (error) {
            void vscode.window.showErrorMessage(
              `Could not build an environment proposal: ${String(error)}`,
            );
          }
          return;
        }
        if (
          message.type === "applyEnvironmentProposal" &&
          pendingProposal &&
          message.selectedPrefabs
        ) {
          const current = JSON.parse(document.getText()) as EnvironmentScenario;
          if (current.networks.length > 0 || current.devices.length > 0) {
            void vscode.window.showErrorMessage(
              "Source proposals never overwrite a populated environment. Create an empty environment and preview again.",
            );
            return;
          }
          if (
            pendingProposal.blockers.length > 0 &&
            message.confirmAssumptions !== true
          ) {
            void vscode.window.showWarningMessage(
              "Explicitly confirm every unresolved assumption before applying this proposal.",
            );
            return;
          }
          try {
            const proposed = scenarioFromProposal(
              pendingProposal,
              message.selectedPrefabs,
              document.uri,
              (await this.reference).catalog,
            );
            if (await writeScenario(proposed)) {
              pendingProposal = undefined;
              await panel.webview.postMessage({
                type: "environmentProposalApplied",
              });
            }
          } catch (error) {
            void vscode.window.showErrorMessage(String(error));
          }
          return;
        }
        if (
          message.type === "topologyDebugAction" &&
          message.action &&
          message.targetId
        ) {
          await this.debugOverlays.action(
            document.uri,
            message.action,
            message.targetId,
          );
          return;
        }
        if (message.type === "saveTopologyLayout" && message.positions) {
          const scenario = JSON.parse(document.getText()) as EnvironmentScenario;
          const reference = await this.reference;
          currentLayout = savedTopologyLayout(
            scenario,
            reference.catalog,
            message.positions,
            message.viewport,
          );
          await vscode.workspace.fs.writeFile(
            topologyLayoutUri(document.uri),
            Buffer.from(`${JSON.stringify(currentLayout, null, 2)}\n`, "utf8"),
          );
          return;
        }
        if (message.type === "resetTopologyLayout") {
          try {
            await vscode.workspace.fs.delete(topologyLayoutUri(document.uri));
          } catch {
            // A missing sidecar already represents automatic layout.
          }
          currentLayout = undefined;
          await update();
          return;
        }
        if (message.type === "duplicateTopology" && message.topologySelection) {
          const scenario = JSON.parse(document.getText()) as EnvironmentScenario;
          await writeScenario(
            duplicateTopologySelection(scenario, message.topologySelection),
          );
          return;
        }
        if (message.type === "exportTopology") {
          const scenario = JSON.parse(document.getText()) as EnvironmentScenario;
          const selected = message.topologySelection;
          if (!selected) {
            void vscode.window.showWarningMessage(
              "Select a topology node before exporting a fragment.",
            );
            return;
          }
          const exported = exportTopologyFragment(scenario, {
            ...(selected.kind === "device"
              ? { deviceIds: [selected.id] }
              : {
                  networkIds: [selected.id],
                  deviceIds: scenario.devices
                    .filter((device) =>
                      Object.values(device.connections ?? {}).includes(
                        selected.id,
                      ),
                    )
                    .map(({ id }) => id),
                }),
            layout: currentLayout,
          });
          const destination = await vscode.window.showSaveDialog({
            defaultUri: document.uri.with({
              path: document.uri.path.replace(
                /\.ic10sim\.json$/,
                ".ic10topology.json",
              ),
            }),
            filters: { "IC10 topology fragment": ["ic10topology.json"] },
            saveLabel: "Export Topology Fragment",
          });
          if (destination) {
            await vscode.workspace.fs.writeFile(
              destination,
              Buffer.from(
                `${JSON.stringify(exported.fragment, null, 2)}\n`,
                "utf8",
              ),
            );
            if (exported.warnings.length > 0) {
              void vscode.window.showWarningMessage(
                exported.warnings.join(" "),
              );
            }
          }
          return;
        }
        if (message.type === "importTopology") {
          const picked = await vscode.window.showOpenDialog({
            canSelectFiles: true,
            canSelectFolders: false,
            canSelectMany: false,
            filters: { "IC10 topology fragment": ["ic10topology.json"] },
            openLabel: "Preview Topology Fragment",
          });
          if (!picked?.[0]) {
            return;
          }
          const fragmentUri = picked[0];
          const parsed = parseTopologyFragment(
            Buffer.from(
              await vscode.workspace.fs.readFile(fragmentUri),
            ).toString("utf8"),
          );
          if (!parsed.fragment) {
            void vscode.window.showErrorMessage(parsed.errors.join(" "));
            return;
          }
          const scenario = JSON.parse(document.getText()) as EnvironmentScenario;
          pendingImport = previewFragmentImport(scenario, parsed.fragment, {
            origin: fragmentUri.toString(true),
            destination: document.uri.toString(true),
            resolveProgramPath: ({ program }) => {
              if (/^[a-z][a-z0-9+.-]*:/i.test(program)) {
                return program;
              }
              const source = fragmentUri.with({
                path: path.posix.resolve(
                  path.posix.dirname(fragmentUri.path),
                  program,
                ),
              });
              return relativeUriPath(document.uri, source);
            },
          });
          const summary = [
            `Import ${pendingImport.fragment.networks.length} network(s) and ${pendingImport.fragment.devices.length} device(s)?`,
            ...pendingImport.warnings,
          ].join("\n");
          const confirmed = await vscode.window.showWarningMessage(
            summary,
            { modal: true },
            "Import Atomically",
          );
          if (confirmed === "Import Atomically" && pendingImport) {
            const applied = applyFragmentImport(scenario, pendingImport);
            await writeScenario(applied);
            if (pendingImport.fragment.layout) {
              const reference = await this.reference;
              currentLayout = savedTopologyLayout(
                applied,
                reference.catalog,
                {
                  ...(currentLayout?.nodes ?? {}),
                  ...pendingImport.fragment.layout.nodes,
                },
                currentLayout?.viewport,
              );
              await vscode.workspace.fs.writeFile(
                topologyLayoutUri(document.uri),
                Buffer.from(
                  `${JSON.stringify(currentLayout, null, 2)}\n`,
                  "utf8",
                ),
              );
            }
            pendingImport = undefined;
          }
          return;
        }
        if (message.type !== "save" || !message.scenario) {
          return;
        }
        await writeScenario(message.scenario);
      },
      undefined,
      this.context.subscriptions,
    );
  }
}

export async function createSimulationEnvironment(): Promise<void> {
  const active = vscode.window.activeTextEditor?.document;
  const workspaceFolder =
    (active && vscode.workspace.getWorkspaceFolder(active.uri)) ??
    vscode.workspace.workspaceFolders?.[0];
  const defaultUri = workspaceFolder
    ? vscode.Uri.joinPath(workspaceFolder.uri, "simulation.ic10sim.json")
    : undefined;
  const destination = await vscode.window.showSaveDialog({
    defaultUri,
    filters: { "IC10 Simulation Environment": ["ic10sim.json"] },
    saveLabel: "Create Simulation Environment",
  });
  if (!destination) {
    return;
  }
  const base = path.dirname(destination.fsPath);
  const program =
    active?.languageId === "ic10"
      ? path.relative(base, active.uri.fsPath).replaceAll("\\", "/")
      : undefined;
  const scenario = {
    schemaVersion: 1,
    networks: [
      { id: "data", kind: "cable", cableRole: "data" },
      { id: "power", kind: "cable", cableRole: "power" },
    ],
    devices: program
      ? [
          {
            id: "main-ic",
            prefab: "StructureCircuitHousing",
            name: "Main IC",
            connections: { "0": "data", "1": "power" },
            fields: {},
            ic: {
              program,
              enabled: true,
              pins: {},
              registers: {},
              stack: {},
            },
          },
        ]
      : [],
  };
  await vscode.workspace.fs.writeFile(
    destination,
    Buffer.from(`${JSON.stringify(scenario, null, 2)}\n`, "utf8"),
  );
  await vscode.commands.executeCommand(
    "vscode.openWith",
    destination,
    Ic10EnvironmentEditorProvider.viewType,
  );
}

export async function openEnvironmentTarget(
  target: EnvironmentTarget,
): Promise<void> {
  Ic10EnvironmentEditorProvider.queueReveal(target);
  await vscode.commands.executeCommand(
    "vscode.openWith",
    vscode.Uri.parse(target.scenarioUri, true),
    Ic10EnvironmentEditorProvider.viewType,
  );
}

export function registerSimulationProgramRenameTracking(
  context: vscode.ExtensionContext,
): void {
  context.subscriptions.push(
    vscode.workspace.onDidRenameFiles(async (event) => {
      const renames = event.files.map(({ oldUri, newUri }) => ({
        oldPath: path.resolve(oldUri.fsPath),
        newPath: path.resolve(newUri.fsPath),
      }));
      const scenarios = await vscode.workspace.findFiles(
        "**/*.ic10sim.json",
        "**/{node_modules,target,dist}/**",
        200,
      );
      let changedPrograms = 0;
      for (const scenarioUri of scenarios) {
        const open = vscode.workspace.textDocuments.find(
          (document) =>
            document.uri.toString() === scenarioUri.toString(),
        );
        const source = open
          ? open.getText()
          : Buffer.from(
              await vscode.workspace.fs.readFile(scenarioUri),
            ).toString("utf8");
        let parsed: {
          devices?: {
            ic?: { program?: string };
          }[];
        };
        try {
          parsed = JSON.parse(source) as typeof parsed;
        } catch {
          continue;
        }
        let changed = false;
        for (const device of parsed.devices ?? []) {
          const program = device.ic?.program;
          if (!program) {
            continue;
          }
          const resolved = path.resolve(
            path.dirname(scenarioUri.fsPath),
            program,
          );
          const matched = renames.find(({ oldPath }) =>
            isSameOrChildPath(resolved, oldPath),
          );
          if (!matched) {
            continue;
          }
          const suffix = path.relative(matched.oldPath, resolved);
          const renamed = path.join(matched.newPath, suffix);
          device.ic!.program = programPathForScenario(
            scenarioUri.fsPath,
            renamed,
          );
          changed = true;
          changedPrograms += 1;
        }
        if (!changed) {
          continue;
        }
        const replacement = `${JSON.stringify(parsed, null, 2)}\n`;
        if (open) {
          const edit = new vscode.WorkspaceEdit();
          edit.replace(
            open.uri,
            new vscode.Range(
              open.positionAt(0),
              open.positionAt(source.length),
            ),
            replacement,
          );
          await vscode.workspace.applyEdit(edit);
        } else {
          await vscode.workspace.fs.writeFile(
            scenarioUri,
            Buffer.from(replacement, "utf8"),
          );
        }
      }
      if (changedPrograms > 0) {
        void vscode.window.showInformationMessage(
          `Updated ${changedPrograms} renamed IC10 program reference${
            changedPrograms === 1 ? "" : "s"
          } in simulation environments.`,
        );
      }
    }),
  );
}

function isSameOrChildPath(candidate: string, parent: string): boolean {
  const normalizedCandidate = normalizeFsPath(candidate);
  const normalizedParent = normalizeFsPath(parent);
  return (
    normalizedCandidate === normalizedParent ||
    normalizedCandidate.startsWith(`${normalizedParent}${path.sep}`)
  );
}

function normalizeFsPath(value: string): string {
  const normalized = path.normalize(value);
  return process.platform === "win32"
    ? normalized.toLocaleLowerCase()
    : normalized;
}

async function loadReference(
  context: vscode.ExtensionContext,
): Promise<EditorReference> {
  const deviceCandidates = [
    vscode.Uri.joinPath(context.extensionUri, "reference", "devices.json")
      .fsPath,
    path.resolve(
      context.extensionPath,
      "..",
      "..",
      "data",
      "generated",
      "devices.json",
    ),
  ];
  let catalog: DeviceCatalog | undefined;
  for (const candidate of deviceCandidates) {
    try {
      const source = await fs.readFile(candidate, "utf8");
      const parsed = JSON.parse(source) as DeviceCatalogFile;
      catalog = { ...parsed.devices, ...parsed.otherLogicables };
      break;
    } catch {
      // Try the development or packaged location next.
    }
  }
  if (!catalog) {
    throw new Error(
      "The Stationpedia device catalog is missing. Run the extension packaging preparation step.",
    );
  }
  let items: Record<string, ItemMetadata> = {};
  const resourceCandidates = [
    vscode.Uri.joinPath(context.extensionUri, "reference", "resources.json")
      .fsPath,
    path.resolve(
      context.extensionPath,
      "..",
      "..",
      "data",
      "generated",
      "resources.json",
    ),
  ];
  for (const candidate of resourceCandidates) {
    try {
      const source = await fs.readFile(candidate, "utf8");
      const parsed = JSON.parse(source) as {
        resources?: Record<string, ItemMetadata>;
      };
      items = parsed.resources ?? {};
      break;
    } catch {
      // Try the development or packaged location next.
    }
  }
  const instructionCandidates = [
    vscode.Uri.joinPath(context.extensionUri, "reference", "instructions.json")
      .fsPath,
    path.resolve(
      context.extensionPath,
      "..",
      "..",
      "data",
      "generated",
      "instructions.json",
    ),
  ];
  for (const candidate of instructionCandidates) {
    try {
      const source = await fs.readFile(candidate, "utf8");
      const parsed = JSON.parse(source) as {
        enums?: Record<
          string,
          { values?: Record<string, { description?: string }> }
        >;
      };
      return {
        catalog,
        items,
        logicHelp: descriptions(parsed.enums?.LogicType?.values),
        slotHelp: descriptions(parsed.enums?.LogicSlotType?.values),
      };
    } catch {
      // Try the development or packaged location next.
    }
  }
  return { catalog, items, logicHelp: {}, slotHelp: {} };
}

function descriptions(
  values: Record<string, { description?: string }> | undefined,
): Record<string, string> {
  return Object.fromEntries(
    Object.entries(values ?? {}).map(([name, value]) => [
      name,
      value.description ?? "",
    ]),
  );
}

async function findPrograms(scenario: vscode.Uri): Promise<string[]> {
  const programs = await vscode.workspace.findFiles(
    "**/*.ic10",
    "**/{node_modules,target,dist}/**",
    500,
  );
  return programs
    .map((program) => programPathForScenario(scenario.fsPath, program.fsPath))
    .sort((left, right) => left.localeCompare(right));
}

function programPathForScenario(
  scenarioPath: string,
  programPath: string,
): string {
  const relative = path.relative(path.dirname(scenarioPath), programPath);
  return (path.isAbsolute(relative) ? programPath : relative).replaceAll(
    "\\",
    "/",
  );
}

function topologyLayoutUri(scenario: vscode.Uri): vscode.Uri {
  return scenario.with({
    path: path.posix.join(
      path.posix.dirname(scenario.path),
      topologyLayoutFilename(path.posix.basename(scenario.path)),
    ),
  });
}

async function readTopologyLayout(
  scenarioUri: vscode.Uri,
  scenario: EnvironmentScenario,
  catalog: DeviceCatalog,
): Promise<EnvironmentLayoutSidecar | undefined> {
  try {
    const source = Buffer.from(
      await vscode.workspace.fs.readFile(topologyLayoutUri(scenarioUri)),
    ).toString("utf8");
    const graph = buildTopologyGraph(scenario, catalog);
    const parsed = parseEnvironmentLayoutSidecar(source, graph);
    if (parsed.errors.length > 0) {
      void vscode.window.showWarningMessage(
        `Ignoring invalid topology layout: ${parsed.errors.join(" ")}`,
      );
      return undefined;
    }
    return parsed.layout;
  } catch {
    return undefined;
  }
}

function relativeUriPath(
  destinationScenario: vscode.Uri,
  source: vscode.Uri,
): string {
  if (
    destinationScenario.scheme !== source.scheme ||
    destinationScenario.authority !== source.authority
  ) {
    return source.toString(true);
  }
  return path.posix.relative(
    path.posix.dirname(destinationScenario.path),
    source.path,
  );
}

async function pickProposalProgram(
  scenario: vscode.Uri,
): Promise<vscode.Uri | undefined> {
  const programs = await vscode.workspace.findFiles(
    "**/*.ic10",
    "**/{node_modules,target,dist}/**",
    500,
  );
  const selected = await vscode.window.showQuickPick(
    programs.map((uri) => ({
      label: path.posix.basename(uri.path),
      description: relativeUriPath(scenario, uri),
      uri,
    })),
    {
      title: "Propose environment from IC10 source",
      placeHolder: "Choose the program to analyse without modifying files",
      matchOnDescription: true,
    },
  );
  return selected?.uri;
}

export function scenarioFromProposal(
  preview: EnvironmentProposalPreview,
  selectedPrefabs: Readonly<Record<string, string>>,
  destination: vscode.Uri,
  catalog: DeviceCatalog,
): EnvironmentScenario {
  return scenarioFromEnvironmentProposal(
    preview,
    selectedPrefabs,
    relativeUriPath(
      destination,
      vscode.Uri.parse(preview.proposal.housing.programUri, true),
    ),
    catalog,
  );
}

function environmentHtml(webview: vscode.Webview): string {
  const nonce = getNonce();
  return /* html */ `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource}; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>IC10 Simulation Environment</title>
  <style>
    * { box-sizing: border-box; }
    body { padding: 0; margin: 0; color: var(--vscode-foreground); background: var(--vscode-editor-background); font-family: var(--vscode-font-family); }
    button, input, select, textarea { font: inherit; color: var(--vscode-input-foreground); background: var(--vscode-input-background); border: 1px solid var(--vscode-input-border, transparent); }
    button { color: var(--vscode-button-foreground); background: var(--vscode-button-background); padding: 5px 10px; cursor: pointer; }
    button:hover { background: var(--vscode-button-hoverBackground); }
    button.secondary { color: var(--vscode-foreground); background: var(--vscode-button-secondaryBackground); }
    button.danger { color: var(--vscode-errorForeground); background: transparent; border-color: var(--vscode-errorForeground); }
    .view-tabs { display: flex; gap: 2px; padding: 7px 10px 0; background: var(--vscode-sideBar-background); border-bottom: 1px solid var(--vscode-panel-border); }
    .view-tab { color: var(--vscode-foreground); background: transparent; border: 0; border-bottom: 2px solid transparent; }
    .view-tab[aria-selected="true"] { border-bottom-color: var(--vscode-focusBorder); }
    .toolbar { display: grid; grid-template-columns: minmax(320px, 520px) auto auto minmax(180px, auto) auto auto; gap: 7px; padding: 10px; border-bottom: 1px solid var(--vscode-panel-border); background: var(--vscode-sideBar-background); }
    .icon-button { display: inline-flex; align-items: center; justify-content: center; min-width: 30px; padding: 5px 7px; font-family: var(--vscode-editor-font-family); font-weight: 700; }
    .device-picker { position: relative; min-width: 0; }
    .picker-trigger { width: 100%; display: grid; grid-template-columns: 34px minmax(0, 1fr) auto; gap: 8px; align-items: center; padding: 4px 8px; text-align: left; color: var(--vscode-foreground); background: var(--vscode-dropdown-background); border-color: var(--vscode-dropdown-border); }
    .picker-trigger img { width: 32px; height: 32px; object-fit: contain; }
    .picker-trigger .picker-copy { min-width: 0; }
    .picker-trigger .picker-name, .picker-trigger .picker-meta { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .picker-trigger .picker-meta { color: var(--vscode-descriptionForeground); font-size: 11px; }
    .picker-panel { position: absolute; z-index: 20; top: calc(100% + 4px); left: 0; width: min(560px, calc(100vw - 24px)); padding: 8px; background: var(--vscode-editorWidget-background); border: 1px solid var(--vscode-widget-border, var(--vscode-panel-border)); box-shadow: 0 6px 18px var(--vscode-widget-shadow); }
    .picker-panel[hidden] { display: none; }
    .picker-panel input { width: 100%; min-height: 30px; padding: 5px 8px; margin-bottom: 7px; }
    .picker-results { max-height: min(520px, 65vh); overflow: auto; }
    .catalog-item { width: 100%; display: grid; grid-template-columns: 52px minmax(0, 1fr); gap: 10px; align-items: center; padding: 7px 9px; text-align: left; color: var(--vscode-foreground); background: transparent; border: 1px solid transparent; }
    .catalog-item:hover, .catalog-item.selected { color: var(--vscode-list-activeSelectionForeground); background: var(--vscode-list-activeSelectionBackground); }
    .catalog-item img, .catalog-placeholder { width: 48px; height: 48px; object-fit: contain; }
    .catalog-item strong, .catalog-item span { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .catalog-item span { color: var(--vscode-descriptionForeground); font-size: 11px; }
    .layout { display: grid; grid-template-columns: 260px minmax(400px, 1fr); min-height: calc(100vh - 52px); }
    .sidebar { padding: 10px; border-right: 1px solid var(--vscode-panel-border); background: var(--vscode-sideBar-background); overflow: auto; }
    .inspector { padding: 18px 22px 60px; overflow: auto; }
    .topology { display: none; min-height: calc(100vh - 96px); }
    body.topology-mode .layout { display: none; }
    body.topology-mode .topology { display: block; }
    .topology-tools { display: flex; flex-wrap: wrap; gap: 7px; padding: 9px 10px; background: var(--vscode-sideBar-background); border-bottom: 1px solid var(--vscode-panel-border); }
    .topology-tools input { min-width: 220px; padding: 5px 7px; }
    .topology-tools select { padding: 5px 7px; }
    .topology-scroll { position: relative; min-height: calc(100vh - 142px); overflow: auto; }
    .topology-surface { position: relative; min-width: 900px; min-height: 620px; transform-origin: 0 0; }
    .topology-svg { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; }
    .topology-edge { stroke-width: 3; fill: none; }
    .topology-edge.cable { stroke: var(--vscode-charts-yellow); }
    .topology-edge.gas { stroke: var(--vscode-charts-blue); }
    .topology-edge.liquid { stroke: var(--vscode-charts-cyan); }
    .topology-edge.chute { stroke: var(--vscode-charts-green); }
    .topology-edge.pin { stroke: var(--vscode-charts-purple); stroke-dasharray: 6 4; }
    .topology-edge.error { stroke: var(--vscode-errorForeground); }
    .edge-label { fill: var(--vscode-foreground); font: 11px var(--vscode-font-family); paint-order: stroke; stroke: var(--vscode-editor-background); stroke-width: 4px; text-anchor: middle; }
    .topology-node { position: absolute; width: 245px; min-height: 86px; padding: 8px; text-align: left; color: var(--vscode-foreground); background: var(--vscode-editorWidget-background); border: 1px solid var(--vscode-panel-border); box-shadow: 0 2px 8px var(--vscode-widget-shadow); }
    .topology-node.active { outline: 2px solid var(--vscode-focusBorder); }
    .topology-node.warning { border-color: var(--vscode-editorWarning-foreground); }
    .topology-node.error { border-color: var(--vscode-errorForeground); }
    .topology-node strong, .topology-node small { display: block; }
    .topology-node small { color: var(--vscode-descriptionForeground); }
    .topology-ports { display: flex; flex-wrap: wrap; gap: 3px; margin-top: 7px; }
    .topology-port { padding: 2px 4px; border: 1px solid var(--vscode-panel-border); color: var(--vscode-descriptionForeground); font-size: 10px; }
    .runtime-line { display: block; margin-top: 4px; color: var(--vscode-descriptionForeground); font-size: 10px; }
    .recent-write { animation: topology-write 900ms ease-out; }
    @keyframes topology-write {
      from { box-shadow: 0 0 0 3px var(--vscode-editorInfo-foreground); }
      to { box-shadow: 0 0 0 0 transparent; }
    }
    .validation-badge { float: right; font-weight: 700; }
    .topology-empty { padding: 24px; color: var(--vscode-descriptionForeground); }
    .proposal-dialog { width: min(900px, calc(100vw - 40px)); max-height: 85vh; padding: 0; color: var(--vscode-foreground); background: var(--vscode-editor-background); border: 1px solid var(--vscode-panel-border); }
    .proposal-dialog::backdrop { background: rgba(0, 0, 0, .55); }
    .proposal-head, .proposal-actions { position: sticky; padding: 12px 16px; background: var(--vscode-editor-background); z-index: 2; }
    .proposal-head { top: 0; border-bottom: 1px solid var(--vscode-panel-border); }
    .proposal-actions { bottom: 0; display: flex; justify-content: flex-end; gap: 7px; border-top: 1px solid var(--vscode-panel-border); }
    .proposal-body { padding: 8px 16px 18px; overflow: auto; }
    .proposal-item { margin: 9px 0; padding: 9px; border: 1px solid var(--vscode-panel-border); }
    .proposal-item select { width: 100%; margin-top: 5px; padding: 4px; }
    .proposal-reason { color: var(--vscode-descriptionForeground); font-size: 11px; }
    [data-focus]:focus { outline: 2px solid var(--vscode-focusBorder); outline-offset: 2px; }
    @media (forced-colors: active) {
      .topology-edge { stroke: CanvasText; }
      .topology-node { border: 2px solid CanvasText; }
    }
    @media (prefers-reduced-motion: reduce) {
      *, *::before, *::after { scroll-behavior: auto !important; transition: none !important; animation: none !important; }
    }
    h2 { margin: 0 0 5px; font-size: 18px; }
    h3 { margin: 20px 0 8px; padding-bottom: 5px; border-bottom: 1px solid var(--vscode-panel-border); font-size: 12px; text-transform: uppercase; letter-spacing: .06em; }
    .list-title { margin: 12px 0 5px; color: var(--vscode-descriptionForeground); font-size: 11px; text-transform: uppercase; }
    .item { width: 100%; display: grid; grid-template-columns: 1fr auto; gap: 5px; margin-bottom: 3px; padding: 7px 8px; text-align: left; color: var(--vscode-foreground); background: transparent; border: 1px solid transparent; }
    .item:hover, .item.active { color: var(--vscode-list-activeSelectionForeground); background: var(--vscode-list-activeSelectionBackground); }
    .badge { color: var(--vscode-descriptionForeground); font-size: 11px; }
    .form { display: grid; grid-template-columns: minmax(140px, 220px) minmax(220px, 1fr); gap: 7px 12px; align-items: center; width: 100%; }
    .form label { color: var(--vscode-descriptionForeground); }
    .form input, .form select, .form textarea { width: 100%; min-height: 27px; padding: 4px 6px; }
    .form textarea { min-height: 86px; font-family: var(--vscode-editor-font-family); resize: vertical; }
    .input-action { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 6px; }
    .field-row { display: grid; grid-template-columns: minmax(170px, 220px) minmax(130px, 1fr) 55px; gap: 8px; align-items: center; width: 100%; padding: 3px 0; }
    .field-row input, .field-row select { width: 100%; min-height: 25px; padding: 3px 5px; font-family: var(--vscode-editor-font-family); }
    .sparse-row { display: grid; grid-template-columns: minmax(110px, 180px) minmax(160px, 1fr) auto; gap: 7px; align-items: center; width: 100%; margin-bottom: 5px; }
    .sparse-row input, .sparse-row select { width: 100%; min-height: 27px; padding: 3px 5px; }
    .sparse-row button { padding: 3px 8px; }
    .section-actions { display: flex; align-items: center; justify-content: space-between; width: 100%; margin: 18px 0 8px; padding-bottom: 5px; border-bottom: 1px solid var(--vscode-panel-border); }
    .section-actions h3 { margin: 0; padding: 0; border: 0; }
    .slot-item-control { position: relative; width: 100%; margin: 7px 0 12px; }
    .slot-item-input { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; }
    .slot-item-input input { width: 100%; min-height: 29px; padding: 4px 7px; }
    .slot-item-results { position: absolute; z-index: 12; left: 0; right: 0; max-height: 320px; overflow: auto; background: var(--vscode-editorWidget-background); border: 1px solid var(--vscode-widget-border, var(--vscode-panel-border)); box-shadow: 0 5px 14px var(--vscode-widget-shadow); }
    .slot-item-results[hidden] { display: none; }
    .slot-catalog-item { width: 100%; display: block; padding: 7px 9px; text-align: left; color: var(--vscode-foreground); background: transparent; border: 0; border-bottom: 1px solid var(--vscode-panel-border); }
    .slot-catalog-item:hover { color: var(--vscode-list-activeSelectionForeground); background: var(--vscode-list-activeSelectionBackground); }
    .slot-catalog-item strong, .slot-catalog-item span { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .slot-catalog-item span { color: var(--vscode-descriptionForeground); font-size: 11px; }
    .slots-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(210px, 1fr)); gap: 6px; width: 100%; }
    .slot-section { min-width: 0; border: 1px solid var(--vscode-panel-border); background: var(--vscode-editorWidget-background); }
    .slot-section[open] { grid-column: 1 / -1; }
    .slot-section summary { display: flex; justify-content: space-between; gap: 8px; padding: 7px 9px; cursor: pointer; }
    .slot-section summary strong { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .slot-section summary span { flex: none; color: var(--vscode-descriptionForeground); font-size: 11px; }
    .slot-content { padding: 0 10px 10px; }
    .access { color: var(--vscode-descriptionForeground); font-size: 11px; text-align: right; }
    .device-head { display: grid; grid-template-columns: 70px 1fr auto; align-items: center; gap: 12px; width: 100%; }
    .device-head img { width: 64px; height: 64px; object-fit: contain; }
    .empty, .error { padding: 20px; color: var(--vscode-descriptionForeground); }
    .error { color: var(--vscode-errorForeground); }
    .validation-summary { width: 100%; margin: 0 0 16px; padding: 10px 12px; color: var(--vscode-errorForeground); background: var(--vscode-inputValidation-errorBackground); border: 1px solid var(--vscode-inputValidation-errorBorder); }
    .validation-summary strong, .validation-summary span { display: block; }
    .validation-summary span { margin-top: 3px; }
    .hint { color: var(--vscode-descriptionForeground); font-size: 12px; margin: 5px 0 12px; }
    .checkbox { width: auto !important; min-height: auto !important; justify-self: start; }
    .help { display: inline-flex; align-items: center; justify-content: center; width: 15px; height: 15px; margin-left: 4px; border: 1px solid var(--vscode-descriptionForeground); border-radius: 50%; color: var(--vscode-descriptionForeground); font-size: 10px; cursor: help; vertical-align: 1px; }
    .debug-select { min-width: 180px; }
    button:disabled, select:disabled { cursor: default; opacity: .55; }
    @media (max-width: 760px) {
      .layout { grid-template-columns: 1fr; }
      .sidebar { border-right: 0; border-bottom: 1px solid var(--vscode-panel-border); max-height: 250px; }
      .form { grid-template-columns: 1fr; }
      .toolbar { grid-template-columns: 1fr auto; }
    }
  </style>
</head>
<body>
  <nav class="view-tabs" role="tablist" aria-label="Environment views">
    <button id="inspectorTab" class="view-tab" role="tab" aria-selected="true">Inspector</button>
    <button id="topologyTab" class="view-tab" role="tab" aria-selected="false">Topology</button>
  </nav>
  <div class="toolbar">
    <div id="devicePicker" class="device-picker">
      <button id="devicePickerButton" class="picker-trigger" type="button" aria-haspopup="listbox" aria-expanded="false"></button>
      <div id="devicePickerPanel" class="picker-panel" hidden>
        <input id="deviceFilter" type="search" placeholder="Filter by name, prefab, or hash…" aria-label="Filter device catalogue">
        <div id="deviceResults" class="picker-results" role="listbox"></div>
      </div>
    </div>
    <button id="addDevice">Add device</button>
    <button id="addNetwork" class="secondary">Add network</button>
    <select id="debugIc" class="debug-select" aria-label="IC housing to debug"></select>
    <button id="startDebug">▶ Debug</button>
    <button id="openJson" class="secondary icon-button" title="Open simulation JSON" aria-label="Open simulation JSON">&#123;&#125;</button>
  </div>
  <div class="layout">
    <aside id="sidebar" class="sidebar"></aside>
    <main id="inspector" class="inspector"><div class="empty">Add or select a network or device.</div></main>
  </div>
  <section id="topology" class="topology" role="tabpanel" aria-label="Topology">
    <div class="topology-tools">
      <input id="topologySearch" type="search" placeholder="Search devices and networks…" aria-label="Search topology">
      <select id="topologyKind" aria-label="Filter network kind">
        <option value="">All network kinds</option>
        <option>cable</option><option>gas</option><option>liquid</option><option>chute</option>
      </select>
      <select id="topologyPrefab" aria-label="Filter device prefab">
        <option value="">All device prefabs</option>
      </select>
      <label><input id="topologyIcOnly" type="checkbox"> ICs only</label>
      <select id="topologyValidation" aria-label="Filter validation status">
        <option value="">All validation states</option>
        <option>valid</option><option>warning</option><option>error</option>
      </select>
      <button id="topologyDuplicate" class="secondary">Duplicate</button>
      <button id="topologyExport" class="secondary">Export fragment</button>
      <button id="topologyImport" class="secondary">Import fragment…</button>
      <button id="topologyPropose" class="secondary">Source proposal…</button>
      <button id="topologyReset" class="secondary">Auto layout</button>
      <button id="topologySource" class="secondary">Source</button>
      <button id="topologyVariables" class="secondary">Variables</button>
      <button id="topologyWatch" class="secondary">Watch</button>
      <button id="topologyTrace" class="secondary">Trace</button>
      <button id="topologyZoomOut" class="secondary" aria-label="Zoom out">−</button>
      <button id="topologyZoomIn" class="secondary" aria-label="Zoom in">+</button>
    </div>
    <div id="topologyScroll" class="topology-scroll">
      <div id="topologySurface" class="topology-surface"></div>
    </div>
  </section>
  <dialog id="proposalDialog" class="proposal-dialog" aria-labelledby="proposalTitle">
    <div class="proposal-head"><h2 id="proposalTitle">Environment proposal</h2><div id="proposalDestination" class="hint"></div></div>
    <div id="proposalBody" class="proposal-body"></div>
    <div class="proposal-actions">
      <button id="proposalCancel" class="secondary">Cancel</button>
      <button id="proposalApply">Apply to empty environment</button>
    </div>
  </dialog>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    let scenario;
    let catalog = {};
    let items = {};
    let logicHelp = {};
    let slotHelp = {};
    let programs = [];
    let assetBase = '';
    let topology = null;
    let viewMode = 'inspector';
    let topologyZoom = 1;
    let topologyFocusKey = null;
    let topologyRuntime = null;
    const recentTopologyWrites = new Map();
    let environmentProposalPreview = null;
    let selectedPrefab = '';
    let selection = null;
    let saveTimer;
    const sidebar = document.getElementById('sidebar');
    const inspector = document.getElementById('inspector');
    const devicePicker = document.getElementById('devicePicker');
    const devicePickerButton = document.getElementById('devicePickerButton');
    const devicePickerPanel = document.getElementById('devicePickerPanel');
    const deviceFilter = document.getElementById('deviceFilter');
    const deviceResults = document.getElementById('deviceResults');
    const debugSelect = document.getElementById('debugIc');
    const debugButton = document.getElementById('startDebug');
    const topologySurface = document.getElementById('topologySurface');
    const topologySearch = document.getElementById('topologySearch');
    const topologyKind = document.getElementById('topologyKind');
    const topologyIcOnly = document.getElementById('topologyIcOnly');
    const topologyPrefab = document.getElementById('topologyPrefab');
    const topologyValidation = document.getElementById('topologyValidation');
    const proposalDialog = document.getElementById('proposalDialog');
    const proposalBody = document.getElementById('proposalBody');
    const proposalApply = document.getElementById('proposalApply');
    document.getElementById('openJson').addEventListener('click', () =>
      vscode.postMessage({ type: 'openJson' }));
    const escapeHtml = (value) => String(value ?? '')
      .replaceAll('&', '&amp;').replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
    const slug = (value) => String(value).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'device';
    const unique = (base, values) => {
      let value = base;
      let suffix = 2;
      while (values.includes(value)) value = base + '-' + suffix++;
      return value;
    };
    const scalar = (value) => {
      const trimmed = String(value).trim();
      if (['NaN', 'Infinity', '-Infinity', '-0'].includes(trimmed)) return trimmed;
      const numeric = Number(trimmed);
      return Number.isNaN(numeric) ? trimmed : numeric;
    };
    const help = (text) => text
      ? '<span class="help" title="' + escapeHtml(text) + '" aria-label="' +
        escapeHtml(text) + '">?</span>'
      : '';
    const queueSave = () => {
      clearTimeout(saveTimer);
      saveTimer = setTimeout(() => vscode.postMessage({ type: 'save', scenario }), 120);
    };
    const selected = () => selection?.type === 'network'
      ? scenario.networks[selection.index]
      : selection?.type === 'device' ? scenario.devices[selection.index] : undefined;

    window.addEventListener('message', (event) => {
      const message = event.data;
      if (message.type === 'topologyRuntime') {
        const runtime = message.runtime;
        if (runtime?.type === 'snapshot') {
          topologyRuntime = runtime.state;
          recentTopologyWrites.clear();
        } else if (runtime?.type === 'traceBatch' && topologyRuntime) {
          recentTopologyWrites.clear();
          runtime.writes.slice(-128).forEach((write) => {
            recentTopologyWrites.set(write.targetId, write);
            const target = write.targetKind === 'network'
              ? topologyRuntime.networks[write.targetId]
              : topologyRuntime.devices[write.targetId];
            if (target) {
              target.lastWriter = write.cpuId || write.sourceId;
              if (write.field && write.after !== undefined) {
                if (target.channels) target.channels[write.field] = write.after;
                if (target.fields) target.fields[write.field] = write.after;
              }
            }
          });
          runtime.reads.slice(-128).forEach((read) => {
            const target = read.targetKind === 'network'
              ? topologyRuntime.networks[read.targetId]
              : topologyRuntime.devices[read.targetId];
            if (target) target.lastReader = read.cpuId || read.sourceId;
          });
          if (runtime.ics) topologyRuntime.ics = runtime.ics;
          topologyRuntime.tick = Math.max(
            topologyRuntime.tick,
            ...runtime.writes.map((write) => write.tick),
            ...runtime.reads.map((read) => read.tick)
          );
        } else if (runtime?.type === 'ended') {
          topologyRuntime = null;
          recentTopologyWrites.clear();
        }
        renderTopology();
        return;
      }
      if (message.type === 'environmentProposalPreview') {
        environmentProposalPreview = message.preview;
        renderEnvironmentProposal(message.destination, message.destinationEmpty);
        proposalDialog.showModal();
        return;
      }
      if (message.type === 'environmentProposalApplied') {
        environmentProposalPreview = null;
        proposalDialog.close();
        return;
      }
      if (message.type === 'parseError') {
        inspector.innerHTML = '<div class="error">' + escapeHtml(message.message) + '</div>';
        return;
      }
      if (message.type === 'programs') {
        programs = message.programs || [];
        if (selection?.type === 'device' && selected()?.ic) render();
        return;
      }
      if (message.type === 'programSelected') {
        const device = scenario?.devices?.find((candidate) => candidate.id === message.deviceId);
        if (device?.ic) {
          device.ic.program = message.program;
          queueSave();
          render();
        }
        return;
      }
      if (message.type === 'reveal') {
        const target = message.target || {};
        const index = scenario?.devices?.findIndex((device) =>
          device.id === (target.deviceId || target.icId)
        );
        if (index >= 0) {
          selection = { type: 'device', index };
          render();
          requestAnimationFrame(() => {
            const property = String(target.property || '').split('.').pop();
            const element = property
              ? document.querySelector('[data-field="' + CSS.escape(property) + '"], [data-slot-field="' + CSS.escape(property) + '"], #' + CSS.escape(property))
              : undefined;
            element?.scrollIntoView({ block: 'center' });
            element?.focus?.();
          });
        }
        return;
      }
      if (message.type !== 'update') return;
      scenario = message.scenario;
      topology = message.topology || null;
      if (topology?.viewport?.zoom) {
        topologyZoom = topology.viewport.zoom;
      } else {
        topologyZoom = calculateFitZoom();
      }
      const selectedTopologyPrefab = topologyPrefab.value;
      const topologyPrefabs = Array.from(new Set(
        (topology?.nodes || []).map((node) => node.prefab).filter(Boolean)
      )).sort((a, b) => a.localeCompare(b));
      topologyPrefab.innerHTML = '<option value="">All device prefabs</option>' +
        topologyPrefabs.map((prefab) => '<option value="' + escapeHtml(prefab) +
          '">' + escapeHtml(prefab) + '</option>').join('');
      if (topologyPrefabs.includes(selectedTopologyPrefab)) topologyPrefab.value = selectedTopologyPrefab;
      scenario.networks ??= [];
      scenario.devices ??= [];
      catalog = message.catalog;
      items = message.items || {};
      logicHelp = message.logicHelp || {};
      slotHelp = message.slotHelp || {};
      programs = message.programs || [];
      assetBase = message.assetBase;
      const icIndexes = scenario.devices.map((device, index) => device.ic ? index : -1).filter((index) => index >= 0);
      if (!selection && icIndexes.length === 1) selection = { type: 'device', index: icIndexes[0] };
      else if (!selection && scenario.networks.length) selection = { type: 'network', index: 0 };
      const networksChanged = normalizeNetworkRoles();
      const connectionsChanged = sanitizeConnections();
      const stateChanged = normalizeInitialStates();
      const pinsChanged = sanitizeAllPins();
      populateCatalog();
      render();
      if (networksChanged || connectionsChanged || pinsChanged || stateChanged) queueSave();
    });

    function populateCatalog() {
      if (!catalog[selectedPrefab]) {
        selectedPrefab = Object.values(catalog)
          .sort((a, b) => a.displayName.localeCompare(b.displayName))[0]?.prefabName || '';
      }
      renderDevicePickerButton();
      renderDeviceResults();
    }

    function catalogImage(device, className) {
      return device?.image
        ? '<img class="' + className + '" src="' + assetBase + '/' +
          encodeURIComponent(device.image) + '" alt="">'
        : '<span class="' + className + ' catalog-placeholder"></span>';
    }

    function renderDevicePickerButton() {
      const device = catalog[selectedPrefab];
      if (!device) {
        devicePickerButton.innerHTML = '<span></span><span>Choose a device</span><span>⌄</span>';
        return;
      }
      devicePickerButton.innerHTML = catalogImage(device, '') +
        '<span class="picker-copy"><span class="picker-name">' +
        escapeHtml(device.displayName) + '</span><span class="picker-meta">' +
        escapeHtml(device.prefabName) + ' · ' + escapeHtml(device.prefabHash) +
        '</span></span><span>⌄</span>';
    }

    function renderDeviceResults() {
      const filter = deviceFilter.value.trim().toLowerCase();
      const matches = Object.values(catalog)
        .filter((device) => !filter || [
          device.displayName, device.prefabName, String(device.prefabHash)
        ].some((value) => String(value).toLowerCase().includes(filter)))
        .sort((a, b) => a.displayName.localeCompare(b.displayName));
      const visible = matches.slice(0, 100);
      deviceResults.innerHTML = visible.map((device) =>
        '<button type="button" class="catalog-item ' +
        (device.prefabName === selectedPrefab ? 'selected' : '') +
        '" data-prefab="' + escapeHtml(device.prefabName) + '" role="option" aria-selected="' +
        (device.prefabName === selectedPrefab) + '">' +
        catalogImage(device, '') + '<span><strong>' + escapeHtml(device.displayName) +
        '</strong><span>' + escapeHtml(device.prefabName) + '</span><span>PrefabHash ' +
        escapeHtml(device.prefabHash) + '</span></span></button>'
      ).join('') + (matches.length > visible.length
        ? '<div class="hint">Showing the first ' + visible.length + ' of ' +
          matches.length + ' matches. Refine the filter to narrow the list.</div>'
        : (!matches.length ? '<div class="hint">No devices match this filter.</div>' : ''));
      deviceResults.querySelectorAll('[data-prefab]').forEach((button) =>
        button.addEventListener('click', () => {
          selectedPrefab = button.dataset.prefab;
          renderDevicePickerButton();
          devicePickerPanel.hidden = true;
          devicePickerButton.setAttribute('aria-expanded', 'false');
          devicePickerButton.focus();
        })
      );
    }

    function duplicateValues(values) {
      const counts = new Map();
      values.forEach((value) => counts.set(value, (counts.get(value) || 0) + 1));
      return Array.from(counts)
        .filter(([value, count]) => value !== '' && count > 1)
        .map(([value]) => value);
    }

    function scenarioProblems() {
      const problems = [];
      const networkIds = scenario.networks.map((network) => String(network.id || '').trim());
      const deviceIds = scenario.devices.map((device) => String(device.id || '').trim());
      if (networkIds.some((id) => !id)) problems.push('Every network needs a stable ID.');
      if (deviceIds.some((id) => !id)) problems.push('Every device needs a stable ID.');
      duplicateValues(networkIds).forEach((id) =>
        problems.push('Duplicate network stable ID: ' + id + '.')
      );
      duplicateValues(deviceIds).forEach((id) =>
        problems.push('Duplicate device stable ID: ' + id + '.')
      );
      const referenceIds = scenario.devices
        .filter((device) => device.referenceId !== undefined && device.referenceId !== null)
        .map((device) => Number(device.referenceId));
      referenceIds.forEach((value) => {
        if (!Number.isInteger(value) || value < -2147483648 || value > 2147483647) {
          problems.push('Reference IDs must be 32-bit integers.');
        }
      });
      duplicateValues(referenceIds.map(String)).forEach((id) =>
        problems.push('Duplicate device Reference ID: ' + id + '.')
      );
      scenario.devices
        .filter((device) => device.ic && device.ic.enabled !== false && !device.ic.program)
        .forEach((device) =>
          problems.push('Enabled IC “' + (device.name || device.id) + '” needs a program.')
        );
      return Array.from(new Set(problems));
    }

    function topologySelectionForNode(node) {
      const values = node.kind === 'network' ? scenario.networks : scenario.devices;
      const index = values.findIndex((value) => value.id === node.id);
      return index < 0 ? null : { type: node.kind, index };
    }

    function renderEnvironmentProposal(destination, destinationEmpty) {
      const preview = environmentProposalPreview;
      if (!preview) return;
      document.getElementById('proposalDestination').textContent =
        'Destination: ' + destination +
        (destinationEmpty ? '' : ' — populated; overwrite is refused');
      const candidateItem = (key, title, candidates, reasons, evidence) => {
        const options = candidates.map((candidate, index) =>
          '<option value="' + escapeHtml(candidate.prefabName) + '"' +
          (preview.selectedPrefabs[key] === candidate.prefabName ? ' selected' : '') + '>' +
          escapeHtml(candidate.displayName + ' · ' + Math.round(candidate.confidence * 100) +
            '% · ' + candidate.reason) + '</option>'
        ).join('');
        return '<div class="proposal-item"><strong>' + escapeHtml(title) + '</strong>' +
          '<div class="proposal-reason">' +
          escapeHtml((reasons || []).join(' · ')) + '</div><select data-proposal-key="' +
          escapeHtml(key) + '" aria-label="Prefab for ' + escapeHtml(title) + '">' +
          options + '</select>' +
          (evidence?.length ? '<div class="proposal-reason">Evidence: ' +
            evidence.map((item) => 'line ' + item.line + ': ' + item.text).map(escapeHtml).join(' · ') +
            '</div>' : '') + '</div>';
      };
      const proposal = preview.proposal;
      const devices = proposal.devices.map((device) =>
        candidateItem(device.reference, device.reference, device.candidates,
          device.reasons, device.evidence)
      ).join('');
      const batches = proposal.batchGroups.map((group, index) =>
        candidateItem('batch:' + index,
          group.suggestedName || group.prefabHashExpression,
          group.candidates, group.reasons, group.evidence)
      ).join('');
      const blockers = preview.blockers.length
        ? '<div class="validation-summary"><strong>Unresolved assumptions</strong>' +
          preview.blockers.map((blocker) => '<span>• ' + escapeHtml(blocker) + '</span>').join('') +
          '<label><input id="proposalConfirm" type="checkbox"> I reviewed and explicitly confirm these assumptions.</label></div>'
        : '<div class="hint">No unresolved assumptions.</div>';
      proposalBody.innerHTML =
        '<p>This is a preview only. Review ranked prefab candidates and evidence before one coherent apply action.</p>' +
        blockers + '<h3>IC housing</h3><div class="proposal-item"><strong>' +
        escapeHtml(proposal.housing.suggestedName) + '</strong><div class="proposal-reason">' +
        escapeHtml(proposal.housing.prefab.reason) + '</div></div><h3>Devices</h3>' +
        (devices || '<div class="hint">No direct device references.</div>') +
        '<h3>Batch groups</h3>' + (batches || '<div class="hint">No batch groups.</div>') +
        '<h3>Networks</h3>' + proposal.networks.map((network) =>
          '<div class="proposal-item"><strong>' + escapeHtml(network.suggestedId) +
          '</strong><div class="proposal-reason">' + escapeHtml(network.reason) +
          '</div></div>').join('');
      const updateApply = () => {
        const candidatesComplete = Array.from(
          proposalBody.querySelectorAll('[data-proposal-key]')
        ).every((select) => Boolean(select.value));
        const assumptionsConfirmed = !preview.blockers.length ||
          document.getElementById('proposalConfirm')?.checked;
        proposalApply.disabled = !destinationEmpty || !candidatesComplete || !assumptionsConfirmed;
      };
      proposalBody.querySelectorAll('select').forEach((select) =>
        select.addEventListener('change', updateApply));
      document.getElementById('proposalConfirm')?.addEventListener('change', updateApply);
      updateApply();
    }

    function selectTopologyNode(node) {
      const next = topologySelectionForNode(node);
      if (!next) return;
      selection = next;
      render();
    }

    function renderTopology() {
      if (!topology) {
        topologySurface.innerHTML = '<div class="topology-empty">Topology is unavailable while the scenario is invalid.</div>';
        return;
      }
      const query = topologySearch.value.trim().toLowerCase().split(/\\s+/).filter(Boolean);
      const selectedValue = selected();
      ['topologySource', 'topologyVariables', 'topologyWatch', 'topologyTrace']
        .forEach((id) => {
          document.getElementById(id).disabled = !selectedValue || !topologyRuntime;
        });
      const kind = topologyKind.value;
      const validation = topologyValidation.value;
      const prefab = topologyPrefab.value;
      const visible = topology.nodes.filter((node) => {
        if (topologyIcOnly.checked && !node.isIc) return false;
        if (validation && node.validationState !== validation) return false;
        if (prefab && node.prefab !== prefab) return false;
        if (kind && !(node.kind === 'network' && node.secondaryLabel.toLowerCase().includes(kind))) return false;
        const text = (node.label + ' ' + node.secondaryLabel + ' ' + node.id).toLowerCase();
        return query.every((term) => text.includes(term));
      });
      const keys = new Set(visible.map((node) => node.key));
      const offsetX = 80 - Math.min(0, ...visible.map((node) => node.x));
      const offsetY = 70 - Math.min(0, ...visible.map((node) => node.y));
      const point = (key) => {
        const node = topology.nodes.find((candidate) => candidate.key === key);
        return node ? { x: node.x + offsetX + 122, y: node.y + offsetY + 43 } : null;
      };
      const edges = topology.edges.filter((edge) =>
        keys.has(edge.sourceKey) && (!edge.targetKey || keys.has(edge.targetKey))
      );
      const maxX = Math.max(900, ...visible.map((node) => node.x + offsetX + 330));
      const maxY = Math.max(620, ...visible.map((node) => node.y + offsetY + 180));
      topologySurface.style.width = maxX + 'px';
      topologySurface.style.height = maxY + 'px';
      topologySurface.style.transform = 'scale(' + topologyZoom + ')';
      const nodeBoxes = visible.map((n) => ({
        key: n.key,
        left: n.x + offsetX - 8,
        top: n.y + offsetY - 8,
        right: n.x + offsetX + 253,
        bottom: n.y + offsetY + 128
      }));
      const edgeMarkup = edges.map((edge) => {
        const source = point(edge.sourceKey);
        if (!source) return '';
        const target = edge.targetKey ? point(edge.targetKey) : { x: source.x + 80, y: source.y };
        if (!target) return '';
        const start = edge.direction === 'toDevice' ? target : source;
        const end = edge.direction === 'toDevice' ? source : target;
        const pathInfo = computeEdgePath(start, end, edge.sourceKey, edge.targetKey, nodeBoxes);
        const classes = ['topology-edge', edge.kind === 'pin' ? 'pin' : edge.networkKind || 'cable', edge.validationState].join(' ');
        const aria = edge.label + ', ' + edge.validationState;
        const marker = edge.direction ? ' marker-end="url(#topologyArrow)"' : '';
        return '<g data-focus="' + escapeHtml(edge.key) + '" data-edge-source="' +
          escapeHtml(edge.sourceKey) + '" tabindex="-1" role="button" aria-label="' +
          escapeHtml(aria) + '"><path class="' + classes + '" d="' + pathInfo.d + '"' + marker +
          '></path><text class="edge-label" x="' + pathInfo.labelPos.x + '" y="' + pathInfo.labelPos.y +
          '">' + escapeHtml(edge.label) + '</text></g>';
      }).join('');
      const svg = '<svg class="topology-svg" width="' + maxX + '" height="' + maxY +
        '" aria-label="Environment connections"><defs><marker id="topologyArrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7 z" fill="context-stroke"></path></marker></defs>' +
        edgeMarkup + '</svg>';
      const nodeMarkup = visible.map((node, index) => {
        const active = selection?.type === node.kind &&
          (node.kind === 'network' ? scenario.networks[selection.index]?.id : scenario.devices[selection.index]?.id) === node.id;
        const badge = node.validationState === 'valid' ? '' :
          '<span class="validation-badge" title="' + escapeHtml(node.validationState) +
          '" aria-label="' + escapeHtml(node.validationState) + '">' +
          (node.validationState === 'error' ? '!' : '⚠') + '</span>';
        const ports = node.ports.map((port) =>
          '<span class="topology-port" tabindex="-1" data-focus="' +
          escapeHtml(node.key + ':port:' + port.connectionKey) +
          '" data-node-key="' + escapeHtml(node.key) + '" role="button" aria-label="' +
          escapeHtml(port.label + ' on ' + node.label) + '">' +
          escapeHtml(port.label) + '</span>').join('');
        const runtime = node.kind === 'network'
          ? topologyRuntime?.networks?.[node.id]
          : topologyRuntime?.devices?.[node.id];
        const icRuntime = topologyRuntime?.ics?.[node.id];
        const behaviour = runtime?.behaviour
          ? (runtime.behaviour.modelled
            ? runtime.behaviour.model + '@' + runtime.behaviour.version
            : 'Passive')
          : '';
        const channelSummary = runtime?.channels
          ? Object.entries(runtime.channels).slice(0, 3)
            .map(([field, value]) => field + '=' + value).join(' · ')
          : '';
        const activity = [
          runtime?.lastReader ? 'read ' + runtime.lastReader : '',
          runtime?.lastWriter ? 'wrote ' + runtime.lastWriter : ''
        ].filter(Boolean).join(' · ');
        const runtimeMarkup = behaviour || channelSummary || activity || icRuntime
          ? '<span class="runtime-line">' +
            escapeHtml([
              behaviour,
              icRuntime ? 'IC ' + icRuntime.runState +
                (icRuntime.line ? ' · line ' + icRuntime.line : '') : '',
              channelSummary,
              activity
            ].filter(Boolean).join(' · ')) + '</span>'
          : '';
        return '<div role="button" class="topology-node ' + node.validationState +
          (recentTopologyWrites.has(node.id) ? ' recent-write' : '') +
          (active ? ' active' : '') + '" style="left:' + (node.x + offsetX) +
          'px;top:' + (node.y + offsetY) + 'px" data-focus="' +
          escapeHtml(node.key) + '" data-node-key="' + escapeHtml(node.key) +
          '" data-node-id="' + escapeHtml(node.id) + '" data-node-kind="' +
          node.kind + '" tabindex="' + (index === 0 ? '0' : '-1') +
          '" aria-label="' + escapeHtml(node.label + ', ' + node.secondaryLabel +
          ', ' + node.validationState) + '">' + badge + '<strong>' +
          escapeHtml(node.label) + '</strong><small>' +
          escapeHtml(node.secondaryLabel) + '</small><span class="topology-ports">' +
          ports + '</span>' + runtimeMarkup + '</div>';
      }).join('');
      topologySurface.innerHTML = visible.length
        ? svg + nodeMarkup
        : '<div class="topology-empty">No topology objects match these filters.</div>';
      topologySurface.querySelectorAll('[data-node-key]').forEach((element) => {
        element.addEventListener('click', (event) => {
          event.stopPropagation();
          const node = topology.nodes.find((candidate) => candidate.key === element.dataset.nodeKey);
          if (node) selectTopologyNode(node);
        });
      });
      topologySurface.querySelectorAll('[data-edge-source]').forEach((element) =>
        element.addEventListener('click', () => {
          const node = topology.nodes.find((candidate) => candidate.key === element.dataset.edgeSource);
          if (node) selectTopologyNode(node);
        })
      );
      installTopologyDragging(offsetX, offsetY);
      installTopologyKeyboard();
      if (topologyFocusKey) {
        requestAnimationFrame(() =>
          topologySurface.querySelector('[data-focus="' + CSS.escape(topologyFocusKey) + '"]')?.focus()
        );
      }
    }

    function computeEdgePath(start, end, sourceKey, targetKey, nodeBoxes) {
      const x1 = start.x;
      const y1 = start.y;
      const x2 = end.x;
      const y2 = end.y;
      const dx = x2 - x1;
      const dy = y2 - y1;

      const obstacles = nodeBoxes.filter((b) => b.key !== sourceKey && b.key !== targetKey);

      let offset = Math.abs(dx) < 40 ? 140 : Math.max(60, Math.abs(dx) * 0.5);
      let side = dx >= 0 ? 1 : -1;
      if (Math.abs(dx) < 40) side = 1;

      let cx1 = Math.abs(dx) < 40 ? x1 + offset * side : (dx >= 0 ? x1 + offset : x1 - offset);
      let cy1 = Math.abs(dx) < 40 ? y1 + dy * 0.25 : y1;
      let cx2 = Math.abs(dx) < 40 ? x2 + offset * side : (dx >= 0 ? x2 - offset : x2 + offset);
      let cy2 = Math.abs(dx) < 40 ? y2 - dy * 0.25 : y2;

      const getPoint = (t, p1x, p1y, c1x, c1y, c2x, c2y, p2x, p2y) => {
        const mt = 1 - t;
        return {
          x: mt * mt * mt * p1x + 3 * mt * mt * t * c1x + 3 * mt * t * t * c2x + t * t * t * p2x,
          y: mt * mt * mt * p1y + 3 * mt * mt * t * c1y + 3 * mt * t * t * c2y + t * t * t * p2y
        };
      };

      const intersectsObstacle = (c1x, c1y, c2x, c2y) => {
        const samples = [0.15, 0.3, 0.5, 0.7, 0.85];
        for (let i = 0; i < samples.length; i++) {
          const pt = getPoint(samples[i], x1, y1, c1x, c1y, c2x, c2y, x2, y2);
          for (let j = 0; j < obstacles.length; j++) {
            const b = obstacles[j];
            if (pt.x >= b.left && pt.x <= b.right && pt.y >= b.top && pt.y <= b.bottom) {
              return b;
            }
          }
        }
        return null;
      };

      let hit = intersectsObstacle(cx1, cy1, cx2, cy2);
      let attempts = 0;
      while (hit && attempts < 4) {
        attempts++;
        offset += 80;
        if (attempts === 2) {
          side = -side;
          offset = Math.abs(dx) < 40 ? 140 : Math.max(60, Math.abs(dx) * 0.5);
        }
        if (Math.abs(dx) < 40) {
          cx1 = x1 + offset * side;
          cy1 = y1 + dy * 0.25;
          cx2 = x2 + offset * side;
          cy2 = y2 - dy * 0.25;
        } else {
          cx1 = dx >= 0 ? x1 + offset * side : x1 - offset * side;
          cx2 = dx >= 0 ? x2 - offset * side : x2 + offset * side;
        }
        hit = intersectsObstacle(cx1, cy1, cx2, cy2);
      }

      const d = 'M ' + x1 + ',' + y1 + ' C ' + cx1 + ',' + cy1 + ' ' + cx2 + ',' + cy2 + ' ' + x2 + ',' + y2;

      const labelSamples = [0.5, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8, 0.15, 0.85];
      let labelPos = getPoint(0.5, x1, y1, cx1, cy1, cx2, cy2, x2, y2);
      labelPos.y -= 5;
      let maxClearance = -Infinity;

      for (let i = 0; i < labelSamples.length; i++) {
        const t = labelSamples[i];
        const pt = getPoint(t, x1, y1, cx1, cy1, cx2, cy2, x2, y2);
        pt.y -= 5;
        let inside = false;
        let minMargin = Infinity;

        for (let j = 0; j < nodeBoxes.length; j++) {
          const b = nodeBoxes[j];
          if (pt.x >= b.left && pt.x <= b.right && pt.y >= b.top && pt.y <= b.bottom) {
            inside = true;
            break;
          }
          const dxDist = Math.max(b.left - pt.x, pt.x - b.right, 0);
          const dyDist = Math.max(b.top - pt.y, pt.y - b.bottom, 0);
          const dist = Math.sqrt(dxDist * dxDist + dyDist * dyDist);
          if (dist < minMargin) minMargin = dist;
        }

        if (!inside) {
          labelPos = pt;
          break;
        }

        if (minMargin > maxClearance) {
          maxClearance = minMargin;
          labelPos = pt;
        }
      }

      return { d, labelPos };
    }

    function calculateFitZoom() {
      if (!topology || !topology.nodes || !topology.nodes.length) return 1;
      const visible = topology.nodes;
      const offsetX = 80 - Math.min(0, ...visible.map((node) => node.x));
      const offsetY = 70 - Math.min(0, ...visible.map((node) => node.y));
      const maxX = Math.max(900, ...visible.map((node) => node.x + offsetX + 330));
      const maxY = Math.max(620, ...visible.map((node) => node.y + offsetY + 180));

      const scrollEl = document.getElementById('topologyScroll');
      const viewWidth = (scrollEl && scrollEl.clientWidth > 0) ? scrollEl.clientWidth : (window.innerWidth - 40);
      const viewHeight = (scrollEl && scrollEl.clientHeight > 0) ? scrollEl.clientHeight : (window.innerHeight - 150);

      const scaleX = viewWidth / maxX;
      const scaleY = viewHeight / maxY;
      const fit = Math.min(scaleX, scaleY);
      return Math.max(0.1, Math.min(1.0, Math.round(fit * 100) / 100));
    }

    function updateTopologyEdges(offsetX, offsetY) {
      if (!topology) return;
      const kind = topologyKind.value;
      const validation = topologyValidation.value;
      const prefab = topologyPrefab.value;
      const query = topologySearch.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
      const visible = topology.nodes.filter((node) => {
        if (topologyIcOnly.checked && !node.isIc) return false;
        if (validation && node.validationState !== validation) return false;
        if (prefab && node.prefab !== prefab) return false;
        if (kind && !(node.kind === 'network' && node.secondaryLabel.toLowerCase().includes(kind))) return false;
        const text = (node.label + ' ' + node.secondaryLabel + ' ' + node.id).toLowerCase();
        return query.every((term) => text.includes(term));
      });
      const keys = new Set(visible.map((node) => node.key));
      const point = (key) => {
        const node = topology.nodes.find((candidate) => candidate.key === key);
        return node ? { x: node.x + offsetX + 122, y: node.y + offsetY + 43 } : null;
      };
      const edges = topology.edges.filter((edge) =>
        keys.has(edge.sourceKey) && (!edge.targetKey || keys.has(edge.targetKey))
      );
      const maxX = Math.max(900, ...visible.map((node) => node.x + offsetX + 330));
      const maxY = Math.max(620, ...visible.map((node) => node.y + offsetY + 180));
      topologySurface.style.width = maxX + 'px';
      topologySurface.style.height = maxY + 'px';
      const nodeBoxes = visible.map((n) => ({
        key: n.key,
        left: n.x + offsetX - 8,
        top: n.y + offsetY - 8,
        right: n.x + offsetX + 253,
        bottom: n.y + offsetY + 128
      }));
      const svg = topologySurface.querySelector('.topology-svg');
      if (!svg) return;
      svg.setAttribute('width', maxX);
      svg.setAttribute('height', maxY);
      edges.forEach((edge) => {
        const group = svg.querySelector('[data-focus="' + CSS.escape(edge.key) + '"]');
        if (!group) return;
        const path = group.querySelector('path');
        const text = group.querySelector('text');
        const source = point(edge.sourceKey);
        if (!source || !path) return;
        const target = edge.targetKey ? point(edge.targetKey) : { x: source.x + 80, y: source.y };
        if (!target) return;
        const start = edge.direction === 'toDevice' ? target : source;
        const end = edge.direction === 'toDevice' ? source : target;
        const pathInfo = computeEdgePath(start, end, edge.sourceKey, edge.targetKey, nodeBoxes);
        path.setAttribute('d', pathInfo.d);
        if (text) {
          text.setAttribute('x', pathInfo.labelPos.x);
          text.setAttribute('y', pathInfo.labelPos.y);
        }
      });
    }

    function installTopologyDragging(offsetX, offsetY) {
      topologySurface.querySelectorAll('.topology-node').forEach((nodeElement) => {
        let drag = null;
        nodeElement.addEventListener('pointerdown', (event) => {
          if (event.button !== 0 || event.target.closest('.topology-port')) return;
          const node = topology.nodes.find((candidate) => candidate.key === nodeElement.dataset.nodeKey);
          if (!node) return;
          drag = { x: event.clientX, y: event.clientY, nodeX: node.x, nodeY: node.y };
          nodeElement.setPointerCapture(event.pointerId);
        });
        nodeElement.addEventListener('pointermove', (event) => {
          if (!drag) return;
          const node = topology.nodes.find((candidate) => candidate.key === nodeElement.dataset.nodeKey);
          node.x = drag.nodeX + (event.clientX - drag.x) / topologyZoom;
          node.y = drag.nodeY + (event.clientY - drag.y) / topologyZoom;
          nodeElement.style.left = (node.x + offsetX) + 'px';
          nodeElement.style.top = (node.y + offsetY) + 'px';
          updateTopologyEdges(offsetX, offsetY);
        });
        nodeElement.addEventListener('pointerup', () => {
          if (!drag) return;
          drag = null;
          persistTopologyLayout();
          renderTopology();
        });
      });
    }

    function persistTopologyLayout() {
      if (!topology) return;
      vscode.postMessage({
        type: 'saveTopologyLayout',
        positions: Object.fromEntries(topology.nodes.map((node) => [
          node.key, { x: node.x, y: node.y }
        ])),
        viewport: { x: 0, y: 0, zoom: topologyZoom }
      });
    }

    function installTopologyKeyboard() {
      const focusables = Array.from(topologySurface.querySelectorAll('[data-focus]'));
      focusables.forEach((element) => element.addEventListener('keydown', (event) => {
        topologyFocusKey = element.dataset.focus;
        if (event.key === 'Escape') {
          document.getElementById('topologyTab').focus();
          return;
        }
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          element.click();
          return;
        }
        const directions = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] };
        const direction = directions[event.key];
        if (!direction) return;
        event.preventDefault();
        const box = element.getBoundingClientRect();
        const origin = { x: box.left + box.width / 2, y: box.top + box.height / 2 };
        const next = focusables.map((candidate) => {
          const candidateBox = candidate.getBoundingClientRect();
          const dx = candidateBox.left + candidateBox.width / 2 - origin.x;
          const dy = candidateBox.top + candidateBox.height / 2 - origin.y;
          const primary = direction[0] ? dx * direction[0] : dy * direction[1];
          const cross = direction[0] ? Math.abs(dy) : Math.abs(dx);
          return { candidate, primary, score: primary + cross * 2 };
        }).filter(({ candidate, primary }) => candidate !== element && primary > 0)
          .sort((a, b) => a.score - b.score ||
            String(a.candidate.dataset.focus).localeCompare(String(b.candidate.dataset.focus)))[0]?.candidate;
        if (next) {
          focusables.forEach((item) => item.tabIndex = -1);
          next.tabIndex = 0;
          topologyFocusKey = next.dataset.focus;
          next.focus();
        }
      }));
    }

    function render() {
      renderSidebar();
      renderDebugControls();
      renderTopology();
      if (!selection || !selected()) {
        inspector.innerHTML = '<div class="empty">Add or select a network or device.</div>';
      } else if (selection.type === 'network') {
        renderNetwork(selected());
      } else {
        renderDevice(selected());
      }
      const problems = scenarioProblems();
      if (problems.length) {
        inspector.insertAdjacentHTML(
          'afterbegin',
          '<div class="validation-summary"><strong>Fix this environment before debugging:</strong>' +
          problems.map((problem) => '<span>• ' + escapeHtml(problem) + '</span>').join('') +
          '</div>'
        );
      }
      notifySelection();
    }

    function notifySelection() {
      const device = selection?.type === 'device' ? selected() : undefined;
      vscode.postMessage({
        type: 'selectionChanged',
        icId: device?.ic && device.ic.enabled !== false && device.ic.program
          ? device.id
          : undefined
      });
    }

    function renderDebugControls() {
      const problems = scenarioProblems();
      const finalize = () => {
        debugButton.title = problems.join(' ');
        if (problems.length) debugButton.disabled = true;
      };
      const ics = scenario.devices.filter((device) =>
        device.ic && device.ic.enabled !== false && device.ic.program
      );
      if (!ics.length) {
        debugSelect.innerHTML = '<option value="">No enabled IC programs</option>';
        debugSelect.hidden = false;
        debugSelect.disabled = true;
        debugButton.disabled = true;
        debugButton.textContent = '▶ Debug';
        finalize();
        return;
      }
      debugSelect.disabled = false;
      if (ics.length === 1) {
        debugSelect.innerHTML = '<option value="' + escapeHtml(ics[0].id) + '"></option>';
        debugSelect.value = ics[0].id;
        debugSelect.hidden = true;
        debugButton.disabled = false;
        debugButton.textContent = '▶ Debug ' + (ics[0].name || ics[0].id);
        finalize();
        return;
      }
      const selectedIc = selection?.type === 'device' &&
        ics.some((device) => device.id === selected()?.id) ? selected().id : '';
      debugSelect.hidden = false;
      debugSelect.innerHTML = '<option value="">Select IC housing…</option>' + ics.map((device) =>
        '<option value="' + escapeHtml(device.id) + '">' +
        escapeHtml(device.name || device.id) + '</option>').join('');
      debugSelect.value = selectedIc;
      debugButton.disabled = !selectedIc;
      debugButton.textContent = '▶ Debug';
      finalize();
    }

    function renderSidebar() {
      const networks = scenario.networks.map((network, index) =>
        '<button class="item ' + (selection?.type === 'network' && selection.index === index ? 'active' : '') +
        '" data-select="network" data-index="' + index + '"><span>' + escapeHtml(network.id) +
        '</span><span class="badge">' + escapeHtml(network.kind) +
        (network.kind === 'cable' ? ' · ' + escapeHtml(network.cableRole) : '') +
        '</span></button>').join('');
      const devices = scenario.devices.map((device, index) => {
        const metadata = catalog[device.prefab];
        return '<button class="item ' + (selection?.type === 'device' && selection.index === index ? 'active' : '') +
          '" data-select="device" data-index="' + index + '"><span>' + escapeHtml(device.name || device.id) +
          '</span><span class="badge">' + (device.ic ? 'IC · ' : '') +
          escapeHtml(metadata?.displayName || device.prefab) + '</span></button>';
      }).join('');
      sidebar.innerHTML = '<div class="list-title">Networks</div>' + (networks || '<div class="hint">No networks</div>') +
        '<div class="list-title">Devices and ICs</div>' + (devices || '<div class="hint">No devices</div>');
      sidebar.querySelectorAll('[data-select]').forEach((button) =>
        button.addEventListener('click', () => {
          selection = { type: button.dataset.select, index: Number(button.dataset.index) };
          render();
        })
      );
    }

    function renderNetwork(network) {
      network.channels ??= {};
      const imageNames = {
        cable: 'ItemCableCoil.png',
        chute: 'StructureChuteStraight.png',
        gas: 'ItemKitPipe.png',
        liquid: 'ItemKitPipeLiquid.png'
      };
      const imageName = imageNames[network.kind];
      const image = imageName
        ? '<img src="' + assetBase + '/' + imageName + '" alt="">'
        : '<div></div>';
      const channels = Array.from({ length: 8 }, (_, index) => {
        const name = 'Channel' + index;
        return '<div class="field-row"><span>' + name + '</span><input data-channel="' + name +
          '" value="' + escapeHtml(network.channels[name] ?? 'NaN') + '"><span class="access">R/W</span></div>';
      }).join('');
      const cableRole = network.kind === 'cable'
        ? '<label>Cable purpose' +
          help('Separates logical data and power cable networks so device connection dropdowns only offer compatible networks.') +
          '</label><select id="cableRole">' +
          [
            ['data', 'Data'],
            ['power', 'Power'],
            ['powerAndData', 'Power and data']
          ].map(([value, label]) => '<option value="' + value + '"' +
            (network.cableRole === value ? ' selected' : '') + '>' + label + '</option>').join('') +
          '</select>'
        : '';
      inspector.innerHTML =
        '<div class="device-head">' + image + '<div><h2>' + escapeHtml(network.id) +
        '</h2><div class="hint">' +
        (network.kind === 'cable'
          ? 'Cable channels are shared by every attached compatible connection.'
          : 'A ' + escapeHtml(network.kind) + ' connection network.') +
        '</div></div>' +
        '<button id="delete" class="danger">Delete</button></div>' +
        '<h3>Network</h3><div class="form">' +
        '<label>Stable ID' + help('A unique name used by device connections in this simulation file. Renaming it updates attached connections.') +
        '</label><input id="networkId" value="' + escapeHtml(network.id) + '">' +
        '<label>Kind</label><select id="networkKind">' +
        ['cable', 'chute', 'gas', 'liquid'].map((kind) => '<option' +
          (network.kind === kind ? ' selected' : '') + '>' + kind + '</option>').join('') +
        '</select>' + cableRole + '</div>' +
        (network.kind === 'cable' ? '<h3>Channels</h3>' + channels : '');
      document.getElementById('networkId').addEventListener('change', (event) => {
        const next = event.target.value.trim();
        if (!next) {
          event.target.setCustomValidity('Enter a network stable ID.');
          event.target.reportValidity();
          return;
        }
        if (scenario.networks.some((candidate) =>
          candidate !== network && String(candidate.id).trim() === next
        )) {
          event.target.setCustomValidity('Network stable IDs must be unique.');
          event.target.reportValidity();
          return;
        }
        event.target.setCustomValidity('');
        const previous = network.id;
        network.id = next;
        scenario.devices.forEach((device) => Object.keys(device.connections || {}).forEach((key) => {
          if (device.connections[key] === previous) device.connections[key] = network.id;
        }));
        queueSave(); render();
      });
      document.getElementById('networkKind').addEventListener('change', (event) => {
        network.kind = event.target.value;
        if (network.kind === 'cable') network.cableRole = 'data';
        else delete network.cableRole;
        sanitizeConnections();
        sanitizeAllPins(); queueSave(); render();
      });
      document.getElementById('cableRole')?.addEventListener('change', (event) => {
        network.cableRole = event.target.value;
        sanitizeConnections();
        sanitizeAllPins(); queueSave(); render();
      });
      inspector.querySelectorAll('[data-channel]').forEach((input) =>
        input.addEventListener('change', () => {
          network.channels[input.dataset.channel] = scalar(input.value); queueSave();
        })
      );
      document.getElementById('delete').addEventListener('click', () => {
        scenario.devices.forEach((device) => Object.keys(device.connections || {}).forEach((key) => {
          if (device.connections[key] === network.id) delete device.connections[key];
        }));
        scenario.networks.splice(selection.index, 1);
        sanitizeAllPins();
        selection = null; queueSave(); render();
      });
    }

    function networkSupportsConnection(type, network) {
      const value = String(type || '');
      if (value === 'Data') {
        return network.kind === 'cable' &&
          ['data', 'powerAndData'].includes(network.cableRole);
      }
      if (value === 'Power') {
        return network.kind === 'cable' &&
          ['power', 'powerAndData'].includes(network.cableRole);
      }
      if (value === 'PowerAndData') return network.kind === 'cable';
      if (value === 'Chute') return network.kind === 'chute';
      if (value === 'Pipe') return network.kind === 'gas';
      if (value === 'PipeLiquid') return network.kind === 'liquid';
      return false;
    }

    function normalizeNetworkRoles() {
      let changed = false;
      scenario.networks.forEach((network) => {
        if (network.kind === 'cable' && !network.cableRole) {
          const id = String(network.id).toLowerCase();
          network.cableRole = id.includes('power')
            ? 'power'
            : (id.includes('data') ? 'data' : 'powerAndData');
          changed = true;
        } else if (network.kind !== 'cable' && network.cableRole) {
          delete network.cableRole;
          changed = true;
        }
      });
      return changed;
    }

    function sanitizeConnections() {
      let changed = false;
      scenario.devices.forEach((device) =>
        Object.keys(device.connections || {}).forEach((key) => {
          const network = scenario.networks.find((candidate) =>
            candidate.id === device.connections[key]
          );
          const definition = catalog[device.prefab]?.connections?.[Number(key)];
          if (!network || !definition ||
              !networkSupportsConnection(definition.type, network)) {
            delete device.connections[key];
            changed = true;
          }
        })
      );
      return changed;
    }

    function dataConnectionIndexes(device) {
      const connections = catalog[device.prefab]?.connections || [];
      return connections.map((connection, index) =>
        String(connection.type || '').toLowerCase().includes('data') ? index : -1
      ).filter((index) => index >= 0);
    }

    function dataNetworkId(device) {
      return dataConnectionIndexes(device)
        .map((index) => device.connections?.[String(index)])
        .find((networkId) =>
          scenario.networks.some((network) =>
            network.id === networkId && network.kind === 'cable' &&
            ['data', 'powerAndData'].includes(network.cableRole)
          )
        );
    }

    function validPinDevices(device) {
      const networkId = dataNetworkId(device);
      if (!networkId) return [];
      return scenario.devices.filter((candidate) =>
        candidate.id !== device.id &&
        dataConnectionIndexes(candidate).some((index) =>
          candidate.connections?.[String(index)] === networkId
        )
      );
    }

    function sanitizeAllPins() {
      let changed = false;
      scenario.devices.forEach((device) => {
        if (!device.ic?.pins) return;
        const allowed = new Set(validPinDevices(device).map((candidate) => candidate.id));
        Object.keys(device.ic.pins).forEach((pin) => {
          if (!allowed.has(device.ic.pins[pin])) {
            delete device.ic.pins[pin];
            changed = true;
          }
        });
      });
      return changed;
    }

    function normalizeInitialStates() {
      let changed = false;
      scenario.devices.forEach((device) => {
        if (!device.ic) return;
        device.ic.registers ??= {};
        device.ic.stack ??= {};
        [['r16', 'ra'], ['r17', 'sp']].forEach(([alias, canonical]) => {
          if (!(alias in device.ic.registers)) return;
          if (!(canonical in device.ic.registers)) {
            device.ic.registers[canonical] = device.ic.registers[alias];
          }
          delete device.ic.registers[alias];
          changed = true;
        });
        Object.keys(device.ic.stack).forEach((address) => {
          const numeric = Number(address);
          if (!Number.isInteger(numeric) || numeric < 0 || numeric > 511) return;
          const canonical = String(numeric);
          if (canonical === address) return;
          if (!(canonical in device.ic.stack)) {
            device.ic.stack[canonical] = device.ic.stack[address];
          }
          delete device.ic.stack[address];
          changed = true;
        });
      });
      return changed;
    }

    const booleanLogicFields = new Set([
      'Activate', 'AirRelease', 'AutoLand', 'AutoShutOff', 'ClearMemory',
      'Combustion', 'Error', 'Extended', 'Filtration', 'Flush', 'ForceWrite',
      'Harvest', 'Idle', 'Lock', 'On', 'Open', 'Plant', 'Power', 'Referenced',
      'Reset', 'Survey'
    ]);
    const colors = [
      ['Blue', 0], ['Grey', 1], ['Green', 2], ['Orange', 3],
      ['Red', 4], ['Yellow', 5], ['White', 6], ['Black', 7],
      ['Brown', 8], ['Khaki', 9], ['Pink', 10], ['Purple', 11]
    ];

    function logicChoices(name, metadata) {
      if (name === 'Mode' &&
          ['StructureCircuitHousing', 'StructureCircuitHousingCompact'].includes(metadata?.prefabName)) {
        return [
          ['Number — display Setting numerically', 0],
          ['String — display packed STR text', 1]
        ];
      }
      if (name === 'Mode' && metadata?.modes && Object.keys(metadata.modes).length) {
        return Object.entries(metadata.modes).map(([label, value]) => [label, value]);
      }
      if (name === 'Color') return colors;
      if (booleanLogicFields.has(name)) return [['Off', 0], ['On', 1]];
      return undefined;
    }

    function logicEditor(attributes, name, value, metadata) {
      const choices = logicChoices(name, metadata);
      if (!choices) {
        return '<input ' + attributes + ' value="' + escapeHtml(value) + '">';
      }
      const hasCurrent = choices.some(([, option]) => String(option) === String(value));
      const options = (hasCurrent ? [] : [['Unknown', value]]).concat(choices)
        .map(([label, option]) => '<option value="' + escapeHtml(option) + '"' +
          (String(option) === String(value) ? ' selected' : '') + '>' +
          escapeHtml(label) + ' (' + escapeHtml(option) + ')</option>').join('');
      return '<select ' + attributes + '>' + options + '</select>';
    }

    function logicDescription(name, metadata) {
      const base = logicHelp[name] || '';
      if (name === 'Mode' &&
          ['StructureCircuitHousing', 'StructureCircuitHousingCompact'].includes(metadata?.prefabName)) {
        return 'Controls how the housing displays its Setting value: Number (0) shows the numeric value; String (1) decodes a packed STR value as text.';
      }
      const choices = logicChoices(name, metadata);
      if (!choices?.length) return base;
      const values = choices.map(([label, value]) => label + ' (' + value + ')').join(', ');
      return base + (base ? ' ' : '') + 'Valid values: ' + values + '.';
    }

    function slotAcceptsAnyItemClass(definition) {
      const slotClass = String(definition.class ?? '').trim().toLowerCase();
      return slotClass === '' || slotClass === 'none' || slotClass === '0';
    }

    function slotIsConfigured(values) {
      return Object.values(values || {}).some((value) => {
        if (typeof value === 'number') return value !== 0 || Object.is(value, -0);
        return !['', '0'].includes(String(value));
      });
    }

    function compatibleSlotItems(definition) {
      return Object.values(items).filter((item) =>
        slotAcceptsAnyItemClass(definition) ||
        String(item.slotClass) === String(definition.class)
      );
    }

    function selectedSlotItem(values) {
      const hash = values.OccupantHash ?? values.PrefabHash;
      return Object.values(items).find((item) =>
        Number(item.prefabHash) === Number(hash)
      );
    }

    function slotItemPicker(slot, definition, values) {
      if (!('OccupantHash' in definition.logicTypes) &&
          !('PrefabHash' in definition.logicTypes)) return '';
      const selectedItem = selectedSlotItem(values);
      const compatibility = slotAcceptsAnyItemClass(definition)
        ? 'all item classes'
        : 'slot class ' + definition.class;
      return '<div class="slot-item-control">' +
        '<div class="hint">Optional item preset · ' + escapeHtml(compatibility) +
        '. Selecting an item fills the slot fields supported by the available metadata.</div>' +
        '<div class="slot-item-input"><input data-slot-item-query="' + escapeHtml(slot) +
        '" placeholder="Search item by name, prefab, or hash…" value="' +
        escapeHtml(selectedItem?.displayName || '') + '">' +
        '<button type="button" class="secondary" data-slot-item-clear="' +
        escapeHtml(slot) + '">Clear item</button></div>' +
        (selectedItem
          ? '<div class="hint">Selected: ' + escapeHtml(selectedItem.displayName) +
            ' · ' + escapeHtml(selectedItem.prefabName) + ' · ' +
            escapeHtml(selectedItem.prefabHash) + ' · class ' +
            escapeHtml(selectedItem.slotClass || 'unknown') +
            (selectedItem.slotClassValue != null
              ? ' (' + escapeHtml(selectedItem.slotClassValue) + ')' : '') +
            ' · sorting ' + escapeHtml(selectedItem.sortingClass || 'unknown') +
            (selectedItem.sortingClassValue != null
              ? ' (' + escapeHtml(selectedItem.sortingClassValue) + ')' : '') +
            '</div>'
          : '') +
        '<div class="slot-item-results" data-slot-item-results="' +
        escapeHtml(slot) + '" hidden></div></div>';
    }

    function applySlotItem(device, slot, definition, item) {
      const values = device.slots[slot] ??= {};
      const supported = definition.logicTypes;
      const set = (field, value) => {
        if (field in supported && value !== undefined && value !== null &&
            (typeof value === 'number' || Number.isFinite(Number(value)))) {
          values[field] = Number(value);
        }
      };
      set('OccupantHash', item.prefabHash);
      set('PrefabHash', item.prefabHash);
      set('Occupied', 1);
      set('Quantity', 1);
      set('Damage', 0);
      set('MaxQuantity', item.maxQuantity);
      set('Class', item.slotClassValue);
      set('SortingClass', item.sortingClassValue);
    }

    function clearSlotItem(device, slot, definition) {
      const values = device.slots[slot] || {};
      [
        'OccupantHash', 'PrefabHash', 'Occupied', 'Quantity', 'Damage',
        'MaxQuantity', 'Class', 'SortingClass'
      ].forEach((field) => {
        if (field in definition.logicTypes) delete values[field];
      });
      if (!Object.keys(values).length) delete device.slots[slot];
    }

    function showSlotItemResults(device, slot, definition, query) {
      const results = inspector.querySelector('[data-slot-item-results="' + slot + '"]');
      if (!results) return;
      const filter = String(query || '').trim().toLowerCase();
      const matches = compatibleSlotItems(definition)
        .filter((item) => !filter || [
          item.displayName, item.prefabName, String(item.prefabHash),
          String(item.slotClass || ''), String(item.slotClassValue ?? ''),
          String(item.sortingClass || ''), String(item.sortingClassValue ?? '')
        ].some((value) => value.toLowerCase().includes(filter)))
        .sort((a, b) => a.displayName.localeCompare(b.displayName))
        .slice(0, 60);
      results.innerHTML = matches.map((item) =>
        '<button type="button" class="slot-catalog-item" data-slot-item-prefab="' +
        escapeHtml(item.prefabName) + '"><strong>' + escapeHtml(item.displayName) +
        '</strong><span>' + escapeHtml(item.prefabName) + ' · PrefabHash ' +
        escapeHtml(item.prefabHash) + '</span><span>Class ' +
        escapeHtml(item.slotClass || 'unknown') +
        (item.slotClassValue != null ? ' (' + escapeHtml(item.slotClassValue) + ')' : '') +
        ' · Sorting ' + escapeHtml(item.sortingClass || 'unknown') +
        (item.sortingClassValue != null
          ? ' (' + escapeHtml(item.sortingClassValue) + ')' : '') +
        (item.maxQuantity != null ? ' · Max ' + escapeHtml(item.maxQuantity) : '') +
        '</span></button>'
      ).join('') || '<div class="hint">No compatible items match this search.</div>';
      results.hidden = false;
      results.querySelectorAll('[data-slot-item-prefab]').forEach((button) => {
        button.addEventListener('mousedown', (event) => event.preventDefault());
        button.addEventListener('click', () => {
          const item = items[button.dataset.slotItemPrefab];
          if (!item) return;
          applySlotItem(device, slot, definition, item);
          queueSave(); render();
        });
      });
    }

    function renderDevice(device) {
      const metadata = catalog[device.prefab];
      if (!metadata) {
        inspector.innerHTML = '<div class="error">Unknown device prefab ' + escapeHtml(device.prefab) + '</div>';
        return;
      }
      device.fields ??= {};
      device.connections ??= {};
      device.slots ??= {};
      const image = metadata.image
        ? '<img src="' + assetBase + '/' + encodeURIComponent(metadata.image) + '" alt="">'
        : '<div></div>';
      const connections = metadata.connections.map((connection, index) => {
        const compatibleNetworks = '<option value="">Not attached</option>' + scenario.networks
          .filter((network) => networkSupportsConnection(connection.type, network))
          .map((network) => '<option value="' + escapeHtml(network.id) + '">' + escapeHtml(network.id) +
            ' · ' + escapeHtml(network.kind) +
            (network.kind === 'cable' ? '/' + escapeHtml(network.cableRole) : '') +
            '</option>').join('');
        return '<label>Connection ' + index + ' · ' + escapeHtml(connection.type) +
          help('The ' + connection.type + ' connection (' + connection.role + '). Only compatible network types are shown.') +
          '</label><select data-connection="' + index + '">' + compatibleNetworks + '</select>';
      }).join('');
      const fields = Object.entries(metadata.logicTypes).sort(([a], [b]) => a.localeCompare(b))
        .map(([name, access]) => '<div class="field-row"><span>' + escapeHtml(name) +
          help(logicDescription(name, metadata)) + '</span>' +
          logicEditor('data-field="' + escapeHtml(name) + '"', name, device.fields[name] ?? 0, metadata) +
          '<span class="access">' +
          (access.read ? 'R' : '') + (access.write ? '/W' : '') + '</span></div>').join('');
      const slots = Object.entries(metadata.slots).map(([slot, definition]) => {
        const values = device.slots[slot] || {};
        const configured = slotIsConfigured(values);
        const slotFields = Object.entries(definition.logicTypes).sort(([a], [b]) => a.localeCompare(b))
          .map(([name, access]) => '<div class="field-row"><span>' + escapeHtml(name) +
            help(slotHelp[name]) + '</span>' +
            logicEditor('data-slot="' + escapeHtml(slot) + '" data-slot-field="' + escapeHtml(name) + '"',
              name, values[name] ?? 0, undefined) + '<span class="access">' +
            (access.read ? 'R' : '') + (access.write ? '/W' : '') + '</span></div>').join('');
        return '<details class="slot-section"' + (configured ? ' open' : '') +
          '><summary><strong>Slot ' + escapeHtml(slot) + ' · ' + escapeHtml(definition.name) +
          '</strong><span>' + (configured ? 'Configured' : 'Empty') +
          '</span></summary><div class="slot-content">' +
          slotItemPicker(slot, definition, values) + slotFields + '</div></details>';
      }).join('');
      const ic = device.ic ? renderIc(device) : '';
      const memorySize = Number(metadata.memory?.size || 0);
      const memory = memorySize > 0
        ? '<h3>Device memory</h3><div class="hint">Initial values for the device memory addressed by IC10 get/put instructions. Only explicitly listed addresses are stored; all other cells start at 0. Valid addresses: 0–' +
          (memorySize - 1) + ' · ' + escapeHtml(metadata.memory.access || '') + '.</div>' +
          '<div class="form"><label>Initial memory cells' +
          help('A sparse map means the scenario only records cells whose initial value you set, instead of writing all ' + memorySize + ' cells.') +
          '</label><textarea id="memory">' +
          escapeHtml(JSON.stringify(device.memory || {}, null, 2)) + '</textarea></div>'
        : '';
      inspector.innerHTML =
        '<div class="device-head">' + image + '<div><h2>' + escapeHtml(device.name || metadata.displayName) +
        '</h2><div class="hint">' + escapeHtml(metadata.prefabName) + ' · ' + metadata.prefabHash +
        '</div><div class="hint">' + escapeHtml(metadata.description || '') +
        '</div></div><button id="delete" class="danger">Delete</button></div>' +
        '<h3>Identity</h3><div class="form">' +
        '<label>Stable ID' + help('A unique, editor-friendly identifier used by pins and debugger configuration. It is not the in-game ReferenceId.') +
        '</label><input id="deviceId" value="' + escapeHtml(device.id) + '">' +
        '<label>Labeller name' + help('The name assigned with the in-game Labeller. IC10 name/hash lookups use this value.') +
        '</label><input id="deviceName" value="' + escapeHtml(device.name || '') + '">' +
        '<label>Reference ID' + help('The numeric runtime identity used by ReferenceId. Leave blank for the simulator to assign one.') +
        '</label><input id="referenceId" type="number" step="1" min="-2147483648" max="2147483647" placeholder="Assigned automatically" value="' +
        escapeHtml(device.referenceId ?? '') + '">' +
        '<label>Runs an IC10 program</label><input class="checkbox" id="isIc" type="checkbox"' +
        (device.ic ? ' checked' : '') + '></div>' +
        '<h3>Connections</h3><div class="form">' + (connections || '<div class="hint">No connections</div>') + '</div>' +
        ic + '<h3>Logic fields</h3><div class="hint">These are initial/test-driver values. IC writes still obey R/W access.</div>' +
        fields + (slots ? '<h3>Inventory slots</h3><div class="hint">Configured slots open automatically; empty slots stay collapsed.</div><div class="slots-grid">' +
          slots + '</div>' : '') + memory;
      inspector.querySelectorAll('[data-connection]').forEach((select) => {
        select.value = device.connections[select.dataset.connection] || '';
        select.addEventListener('change', () => {
          if (select.value) device.connections[select.dataset.connection] = select.value;
          else delete device.connections[select.dataset.connection];
          sanitizeAllPins(); queueSave(); render();
        });
      });
      inspector.querySelectorAll('[data-field]').forEach((input) =>
        input.addEventListener('change', () => {
          device.fields[input.dataset.field] = scalar(input.value); queueSave();
        })
      );
      inspector.querySelectorAll('[data-slot-field]').forEach((input) =>
        input.addEventListener('change', () => {
          const values = device.slots[input.dataset.slot] ??= {};
          values[input.dataset.slotField] = scalar(input.value); queueSave();
        })
      );
      inspector.querySelectorAll('[data-slot-item-query]').forEach((input) => {
        const slot = input.dataset.slotItemQuery;
        const definition = metadata.slots[slot];
        input.addEventListener('focus', () =>
          showSlotItemResults(device, slot, definition, input.value)
        );
        input.addEventListener('input', () =>
          showSlotItemResults(device, slot, definition, input.value)
        );
        input.addEventListener('keydown', (event) => {
          if (event.key === 'Escape') {
            inspector.querySelector('[data-slot-item-results="' + slot + '"]').hidden = true;
            input.blur();
          }
        });
        input.addEventListener('blur', () => {
          setTimeout(() => {
            const results = inspector.querySelector('[data-slot-item-results="' + slot + '"]');
            if (results) results.hidden = true;
          }, 0);
        });
      });
      inspector.querySelectorAll('[data-slot-item-clear]').forEach((button) =>
        button.addEventListener('click', () => {
          const slot = button.dataset.slotItemClear;
          clearSlotItem(device, slot, metadata.slots[slot]);
          queueSave(); render();
        })
      );
      document.getElementById('deviceId').addEventListener('change', (event) => {
        const next = event.target.value.trim();
        if (!next) {
          event.target.setCustomValidity('Enter a device stable ID.');
          event.target.reportValidity();
          return;
        }
        if (scenario.devices.some((candidate) =>
          candidate !== device && String(candidate.id).trim() === next
        )) {
          event.target.setCustomValidity('Device stable IDs must be unique.');
          event.target.reportValidity();
          return;
        }
        event.target.setCustomValidity('');
        const previous = device.id;
        device.id = next;
        scenario.devices.forEach((candidate) => Object.keys(candidate.ic?.pins || {}).forEach((pin) => {
          if (candidate.ic.pins[pin] === previous) candidate.ic.pins[pin] = device.id;
        }));
        queueSave(); render();
      });
      document.getElementById('deviceName').addEventListener('change', (event) => {
        device.name = event.target.value; queueSave(); render();
      });
      document.getElementById('referenceId').addEventListener('change', (event) => {
        const raw = event.target.value.trim();
        if (!raw) {
          delete device.referenceId;
          event.target.setCustomValidity('');
          queueSave(); render();
          return;
        }
        const next = Number(raw);
        if (!Number.isInteger(next) || next < -2147483648 || next > 2147483647) {
          event.target.setCustomValidity('Reference ID must be a 32-bit integer.');
          event.target.reportValidity();
          return;
        }
        if (scenario.devices.some((candidate) =>
          candidate !== device && Number(candidate.referenceId) === next
        )) {
          event.target.setCustomValidity('Device Reference IDs must be unique.');
          event.target.reportValidity();
          return;
        }
        event.target.setCustomValidity('');
        device.referenceId = next;
        queueSave(); render();
      });
      document.getElementById('isIc').addEventListener('change', (event) => {
        if (event.target.checked) device.ic = {
          program: '', enabled: true, pins: {}, registers: {}, stack: {}
        };
        else delete device.ic;
        sanitizeAllPins();
        queueSave(); render();
      });
      bindIc(device);
      if (memorySize > 0) {
        document.getElementById('memory').addEventListener('change', (event) => {
          try { device.memory = JSON.parse(event.target.value || '{}'); queueSave(); }
          catch { event.target.setCustomValidity('Enter a JSON object.'); event.target.reportValidity(); }
        });
      }
      document.getElementById('delete').addEventListener('click', () => {
        scenario.devices.splice(selection.index, 1);
        sanitizeAllPins();
        selection = null; queueSave(); render();
      });
    }

    const registerNames = [
      'r0', 'r1', 'r2', 'r3', 'r4', 'r5', 'r6', 'r7',
      'r8', 'r9', 'r10', 'r11', 'r12', 'r13', 'r14', 'r15',
      'ra', 'sp'
    ];

    function sparseRows(kind, values) {
      const entries = Object.entries(values);
      if (!entries.length) return '<div class="hint">No initial values set.</div>';
      const used = new Set(Object.keys(values));
      return entries.map(([key, value]) => {
        const keyEditor = kind === 'registers'
          ? '<select data-sparse-key data-kind="' + kind + '" data-old-key="' + escapeHtml(key) + '">' +
            (registerNames.includes(key) ? [] : [key]).concat(
              registerNames.filter((name) => name === key || !used.has(name))
            )
              .map((name) => '<option' + (name === key ? ' selected' : '') + '>' +
                escapeHtml(name) + '</option>').join('') + '</select>'
          : '<input data-sparse-key data-kind="' + kind + '" data-old-key="' + escapeHtml(key) +
            '" type="number" min="0" max="511" step="1" value="' + escapeHtml(key) + '">';
        return '<div class="sparse-row">' + keyEditor +
          '<input data-sparse-value data-kind="' + kind + '" data-key="' + escapeHtml(key) +
          '" value="' + escapeHtml(value) + '" aria-label="Initial value">' +
          '<button class="danger" data-sparse-delete data-kind="' + kind + '" data-key="' +
          escapeHtml(key) + '" title="Delete initial value">×</button></div>';
      }).join('');
    }

    function renderIc(device) {
      const ic = device.ic;
      ic.pins ??= {}; ic.registers ??= {}; ic.stack ??= {};
      const pinDevices = validPinDevices(device);
      const networkId = dataNetworkId(device);
      const deviceOptions = '<option value="">Not set</option>' + pinDevices
        .map((candidate) => '<option value="' + escapeHtml(candidate.id) + '">' +
          escapeHtml(candidate.name || candidate.id) + '</option>').join('');
      const pins = Array.from({ length: 6 }, (_, index) =>
        '<label>d' + index + '</label><select data-pin="d' + index + '">' + deviceOptions + '</select>'
      ).join('');
      const programOptions = Array.from(new Set([ic.program, ...programs].filter(Boolean)))
        .map((program) => '<option value="' + escapeHtml(program) + '"' +
          (program === ic.program ? ' selected' : '') + '>' +
          escapeHtml(program) + '</option>').join('');
      const programPlaceholder = ic.program
        ? ''
        : '<option value="" selected disabled>' +
          (programOptions ? 'Select an IC10 program…' : 'No IC10 programs found') +
          '</option>';
      return '<h3>IC10 program</h3><div class="form">' +
        '<label>Program path' + help('Path to the IC10 source file, relative to this simulation file. Choose a workspace file or browse anywhere.') +
        '</label><div class="input-action"><select id="program" aria-label="IC10 program path">' +
        programPlaceholder + programOptions +
        '</select><button id="openProgram" class="secondary" title="Open this IC10 source">Open</button>' +
        '<button id="browseProgram" class="secondary" title="Browse for an IC10 file">Browse…</button></div>' +
        '<label>Enabled</label><input class="checkbox" id="icEnabled" type="checkbox"' +
        (ic.enabled !== false ? ' checked' : '') + '></div>' +
        '<h3>Device pins</h3><div class="hint">' +
        (networkId
          ? 'Only devices with a data-capable connection on cable “' + escapeHtml(networkId) + '” are available.'
          : 'Attach this IC housing to a cable through its Data connection before assigning d0–d5.') +
        '</div><div class="form">' + pins + '</div>' +
        '<div class="section-actions"><h3>Initial registers</h3><button id="addRegister" class="secondary">Add register</button></div>' +
        sparseRows('registers', ic.registers) +
        '<div class="section-actions"><h3>Initial stack</h3><button id="addStack" class="secondary">Add stack entry</button></div>' +
        '<div class="hint">Stack addresses must be unique integers from 0 to 511.</div>' +
        sparseRows('stack', ic.stack);
    }

    function bindIc(device) {
      if (!device.ic) return;
      document.getElementById('program').addEventListener('change', (event) => {
        device.ic.program = event.target.value; queueSave(); render();
      });
      document.getElementById('browseProgram').addEventListener('click', () => {
        vscode.postMessage({ type: 'browseProgram', deviceId: device.id });
      });
      document.getElementById('openProgram').addEventListener('click', () => {
        if (device.ic.program) {
          vscode.postMessage({ type: 'openProgram', program: device.ic.program });
        }
      });
      document.getElementById('icEnabled').addEventListener('change', (event) => {
        device.ic.enabled = event.target.checked; queueSave(); render();
      });
      inspector.querySelectorAll('[data-pin]').forEach((select) => {
        select.value = device.ic.pins[select.dataset.pin] || '';
        select.addEventListener('change', () => {
          if (select.value) device.ic.pins[select.dataset.pin] = select.value;
          else delete device.ic.pins[select.dataset.pin];
          queueSave();
        });
      });
      document.getElementById('addRegister').addEventListener('click', () => {
        const key = registerNames.find((name) => !(name in device.ic.registers));
        if (!key) return;
        device.ic.registers[key] = 0; queueSave(); render();
      });
      document.getElementById('addStack').addEventListener('click', () => {
        let address = 0;
        while (String(address) in device.ic.stack && address < 512) address++;
        if (address >= 512) return;
        device.ic.stack[String(address)] = 0; queueSave(); render();
      });
      inspector.querySelectorAll('[data-sparse-key]').forEach((input) =>
        input.addEventListener('change', () => {
          const values = device.ic[input.dataset.kind];
          const oldKey = input.dataset.oldKey;
          const nextKey = input.dataset.kind === 'stack'
            ? String(Number(input.value))
            : input.value;
          if (input.dataset.kind === 'stack') {
            const numeric = Number(input.value);
            if (!Number.isInteger(numeric) || numeric < 0 || numeric > 511 ||
                (nextKey !== oldKey && nextKey in values)) {
              input.setCustomValidity('Choose a unique whole-number stack address from 0 to 511.');
              input.reportValidity();
              input.value = oldKey;
              return;
            }
          }
          if (nextKey !== oldKey) {
            values[nextKey] = values[oldKey];
            delete values[oldKey];
            queueSave(); render();
          }
        })
      );
      inspector.querySelectorAll('[data-sparse-value]').forEach((input) =>
        input.addEventListener('change', () => {
          device.ic[input.dataset.kind][input.dataset.key] = scalar(input.value);
          queueSave();
        })
      );
      inspector.querySelectorAll('[data-sparse-delete]').forEach((button) =>
        button.addEventListener('click', () => {
          delete device.ic[button.dataset.kind][button.dataset.key];
          queueSave(); render();
        })
      );
    }

    devicePickerButton.addEventListener('click', () => {
      const opening = devicePickerPanel.hidden;
      devicePickerPanel.hidden = !opening;
      devicePickerButton.setAttribute('aria-expanded', String(opening));
      if (opening) {
        deviceFilter.value = '';
        renderDeviceResults();
        deviceFilter.focus();
      }
    });
    devicePicker.addEventListener('click', (event) => event.stopPropagation());
    deviceFilter.addEventListener('input', renderDeviceResults);
    deviceFilter.addEventListener('keydown', (event) => {
      if (event.key === 'Escape') {
        devicePickerPanel.hidden = true;
        devicePickerButton.setAttribute('aria-expanded', 'false');
        devicePickerButton.focus();
      }
    });
    document.addEventListener('click', () => {
      devicePickerPanel.hidden = true;
      devicePickerButton.setAttribute('aria-expanded', 'false');
    });
    function setViewMode(mode) {
      viewMode = mode;
      document.body.classList.toggle('topology-mode', mode === 'topology');
      document.getElementById('inspectorTab').setAttribute('aria-selected', String(mode === 'inspector'));
      document.getElementById('topologyTab').setAttribute('aria-selected', String(mode === 'topology'));
      if (mode === 'topology') renderTopology();
    }
    document.getElementById('inspectorTab').addEventListener('click', () => setViewMode('inspector'));
    document.getElementById('topologyTab').addEventListener('click', () => setViewMode('topology'));
    [topologySearch, topologyKind, topologyPrefab, topologyIcOnly, topologyValidation].forEach((control) =>
      control.addEventListener(control.tagName === 'INPUT' ? 'input' : 'change', renderTopology)
    );
    document.getElementById('topologyDuplicate').addEventListener('click', () => {
      const value = selected();
      if (selection && value) {
        vscode.postMessage({
          type: 'duplicateTopology',
          topologySelection: { kind: selection.type, id: value.id }
        });
      }
    });
    document.getElementById('topologyExport').addEventListener('click', () => {
      const value = selected();
      vscode.postMessage({
        type: 'exportTopology',
        topologySelection: selection && value
          ? { kind: selection.type, id: value.id }
          : undefined
      });
    });
    document.getElementById('topologyImport').addEventListener('click', () =>
      vscode.postMessage({ type: 'importTopology' }));
    document.getElementById('topologyPropose').addEventListener('click', () =>
      vscode.postMessage({ type: 'requestEnvironmentProposal' }));
    document.getElementById('topologyReset').addEventListener('click', () =>
      vscode.postMessage({ type: 'resetTopologyLayout' }));
    [
      ['topologySource', 'source'],
      ['topologyVariables', 'variables'],
      ['topologyWatch', 'watch'],
      ['topologyTrace', 'trace']
    ].forEach(([id, action]) =>
      document.getElementById(id).addEventListener('click', () => {
        const value = selected();
        if (value) vscode.postMessage({
          type: 'topologyDebugAction',
          action,
          targetId: value.id
        });
      })
    );
    document.getElementById('topologyZoomOut').addEventListener('click', () => {
      topologyZoom = Math.max(.1, Math.round((topologyZoom - .1) * 10) / 10);
      persistTopologyLayout();
      renderTopology();
    });
    document.getElementById('topologyZoomIn').addEventListener('click', () => {
      topologyZoom = Math.min(8, Math.round((topologyZoom + .1) * 10) / 10);
      persistTopologyLayout();
      renderTopology();
    });
    const topologyScroll = document.getElementById('topologyScroll');
    topologyScroll.addEventListener('wheel', (event) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      if (!event.deltaY) return;
      const rect = topologyScroll.getBoundingClientRect();
      const mouseX = event.clientX - rect.left;
      const mouseY = event.clientY - rect.top;
      const oldZoom = topologyZoom;
      const delta = event.deltaY < 0 ? .1 : -.1;
      const newZoom = Math.max(.1, Math.min(8, Math.round((topologyZoom + delta) * 10) / 10));
      if (newZoom === oldZoom) return;
      topologyZoom = newZoom;
      renderTopology();
      topologyScroll.scrollLeft = (topologyScroll.scrollLeft + mouseX) * (newZoom / oldZoom) - mouseX;
      topologyScroll.scrollTop = (topologyScroll.scrollTop + mouseY) * (newZoom / oldZoom) - mouseY;
      persistTopologyLayout();
    }, { passive: false });
    document.getElementById('proposalCancel').addEventListener('click', () => {
      environmentProposalPreview = null;
      proposalDialog.close();
    });
    proposalApply.addEventListener('click', () => {
      if (!environmentProposalPreview) return;
      const selectedPrefabs = Object.fromEntries(
        Array.from(proposalBody.querySelectorAll('[data-proposal-key]'))
          .map((select) => [select.dataset.proposalKey, select.value])
      );
      vscode.postMessage({
        type: 'applyEnvironmentProposal',
        selectedPrefabs,
        confirmAssumptions: Boolean(
          document.getElementById('proposalConfirm')?.checked
        )
      });
    });
    document.getElementById('addNetwork').addEventListener('click', () => {
      const id = unique('network', scenario.networks.map((network) => network.id));
      scenario.networks.push({ id, kind: 'cable', cableRole: 'data', channels: {} });
      selection = { type: 'network', index: scenario.networks.length - 1 };
      queueSave(); render();
    });
    document.getElementById('addDevice').addEventListener('click', () => {
      const metadata = catalog[selectedPrefab];
      if (!metadata) return;
      const id = unique(slug(metadata.displayName), scenario.devices.map((device) => device.id));
      const device = {
        id, prefab: metadata.prefabName, name: metadata.displayName,
        connections: {}, fields: {}, slots: {}
      };
      if (Number(metadata.memory?.size || 0) > 0) device.memory = {};
      scenario.devices.push(device);
      selection = { type: 'device', index: scenario.devices.length - 1 };
      queueSave(); render();
    });
    debugSelect.addEventListener('change', () => {
      const index = scenario.devices.findIndex((device) => device.id === debugSelect.value && device.ic);
      selection = index >= 0 ? { type: 'device', index } : null;
      render();
    });
    debugButton.addEventListener('click', () => {
      const ics = scenario.devices.filter((device) =>
        device.ic && device.ic.enabled !== false && device.ic.program
      );
      const icId = ics.length === 1 ? ics[0].id : debugSelect.value;
      if (icId) {
        clearTimeout(saveTimer);
        vscode.postMessage({ type: 'startDebug', icId, scenario });
      }
    });
    vscode.postMessage({ type: 'ready' });
  </script>
</body>
</html>`;
}

function getNonce(): string {
  const possible =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  return Array.from(
    { length: 32 },
    () => possible.charAt(Math.floor(Math.random() * possible.length)),
  ).join("");
}
