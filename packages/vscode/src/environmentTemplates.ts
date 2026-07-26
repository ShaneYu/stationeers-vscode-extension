import * as path from "node:path";

import * as vscode from "vscode";

import { validateTemplateRelativePaths } from "./environmentTemplateModel";

export interface EnvironmentTemplateManifest {
  schemaVersion: 1;
  id: string;
  title: string;
  targetGameVersion: string;
  entryFiles: {
    scenario: string;
    tests: string;
    programs: string[];
  };
  tests: string[];
  knownDeviations: string[];
}

export interface TemplateFilePlan {
  relativePath: string;
  source: vscode.Uri;
  destination: vscode.Uri;
}

export function templateDestinationPlan(
  sourceRoot: vscode.Uri,
  destinationRoot: vscode.Uri,
  relativeFiles: readonly string[],
): readonly TemplateFilePlan[] {
  return validateTemplateRelativePaths(relativeFiles).map((normalized) => {
    return {
      relativePath: normalized,
      source: vscode.Uri.joinPath(sourceRoot, ...normalized.split("/")),
      destination: vscode.Uri.joinPath(
        destinationRoot,
        ...normalized.split("/"),
      ),
    };
  });
}

export async function createEnvironmentFromTemplate(
  context: vscode.ExtensionContext,
): Promise<void> {
  if (!vscode.workspace.isTrusted) {
    void vscode.window.showWarningMessage(
      "Trust this workspace before creating executable IC10 template files.",
    );
    return;
  }
  const templatesRoot = vscode.Uri.joinPath(context.extensionUri, "templates");
  const manifests = await loadTemplateManifests(templatesRoot);
  const selected = await vscode.window.showQuickPick(
    manifests.map(({ manifest, root }) => ({
      label: manifest.title,
      description: manifest.id,
      detail: `Stationeers ${manifest.targetGameVersion} · ${manifest.tests.length} tested scenario${manifest.tests.length === 1 ? "" : "s"}`,
      manifest,
      root,
    })),
    {
      title: "Create IC10 environment from template",
      placeHolder: "Choose a tested starting point",
      matchOnDescription: true,
      matchOnDetail: true,
    },
  );
  if (!selected) {
    return;
  }
  const parent = await vscode.window.showOpenDialog({
    canSelectFiles: false,
    canSelectFolders: true,
    canSelectMany: false,
    defaultUri: vscode.workspace.workspaceFolders?.[0]?.uri,
    openLabel: "Choose destination parent",
    title: `Create ${selected.manifest.title}`,
  });
  if (!parent?.[0]) {
    return;
  }
  const destination = vscode.Uri.joinPath(parent[0], selected.manifest.id);
  const files = await collectTemplateFiles(selected.root);
  const plan = templateDestinationPlan(selected.root, destination, files);
  const existing = await existingDestinations(plan);
  if (existing.length > 0) {
    void vscode.window.showErrorMessage(
      `Template creation refused: ${existing.length} destination file${existing.length === 1 ? "" : "s"} already exist. No files were overwritten.`,
    );
    return;
  }
  const preview = [
    `Create ${plan.length} files in`,
    destination.fsPath,
    "",
    `Scenario: ${selected.manifest.entryFiles.scenario}`,
    `Tests: ${selected.manifest.entryFiles.tests}`,
    `Programs: ${selected.manifest.entryFiles.programs.join(", ")}`,
  ].join("\n");
  const confirmation = await vscode.window.showInformationMessage(
    preview,
    { modal: true },
    "Create Template",
  );
  if (confirmation !== "Create Template") {
    return;
  }
  for (const file of plan) {
    const bytes = await vscode.workspace.fs.readFile(file.source);
    await vscode.workspace.fs.createDirectory(
      file.destination.with({
        path: path.posix.dirname(file.destination.path),
      }),
    );
    await vscode.workspace.fs.writeFile(file.destination, bytes);
  }
  const scenario = vscode.Uri.joinPath(
    destination,
    ...selected.manifest.entryFiles.scenario.split("/"),
  );
  await vscode.commands.executeCommand(
    "vscode.openWith",
    scenario,
    "ic10.environment",
  );
}

async function loadTemplateManifests(
  root: vscode.Uri,
): Promise<
  { manifest: EnvironmentTemplateManifest; root: vscode.Uri }[]
> {
  const entries = await vscode.workspace.fs.readDirectory(root);
  const manifests: {
    manifest: EnvironmentTemplateManifest;
    root: vscode.Uri;
  }[] = [];
  for (const [name, type] of entries) {
    if ((type & vscode.FileType.Directory) === 0) {
      continue;
    }
    const templateRoot = vscode.Uri.joinPath(root, name);
    try {
      const source = await vscode.workspace.fs.readFile(
        vscode.Uri.joinPath(templateRoot, "manifest.json"),
      );
      const manifest = JSON.parse(
        Buffer.from(source).toString("utf8"),
      ) as EnvironmentTemplateManifest;
      if (
        manifest.schemaVersion === 1 &&
        manifest.id === name &&
        manifest.title
      ) {
        manifests.push({ manifest, root: templateRoot });
      }
    } catch {
      // A malformed bundled template is skipped; packaging tests identify it.
    }
  }
  return manifests.sort((left, right) =>
    left.manifest.title.localeCompare(right.manifest.title),
  );
}

async function collectTemplateFiles(
  root: vscode.Uri,
  relative = "",
): Promise<string[]> {
  const directory = relative
    ? vscode.Uri.joinPath(root, ...relative.split("/"))
    : root;
  const result: string[] = [];
  for (const [name, type] of await vscode.workspace.fs.readDirectory(directory)) {
    const child = relative ? `${relative}/${name}` : name;
    if ((type & vscode.FileType.Directory) !== 0) {
      result.push(...(await collectTemplateFiles(root, child)));
    } else if ((type & vscode.FileType.File) !== 0) {
      result.push(child);
    }
  }
  return result.sort();
}

async function existingDestinations(
  plan: readonly TemplateFilePlan[],
): Promise<TemplateFilePlan[]> {
  const existing: TemplateFilePlan[] = [];
  for (const file of plan) {
    try {
      await vscode.workspace.fs.stat(file.destination);
      existing.push(file);
    } catch {
      // Missing is the required state.
    }
  }
  return existing;
}
