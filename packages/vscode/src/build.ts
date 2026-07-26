import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";

import { resolveBuildDirectory } from "./buildPath";

export type OptimizationLevel = "none" | "readable" | "compact";

export interface BuildOptions {
  optimization: OptimizationLevel;
  gameVersion?: string;
  sourcePath?: string;
  environment?: string;
}

export interface SourceMapEntry {
  generatedLine: number;
  sourceLine: number;
}

export interface BuildOutput {
  code: string;
  sourceMap: SourceMapEntry[];
  metadata: {
    sourceSha256: string;
    toolVersion: string;
    gameDataVersion: string;
    options: BuildOptions;
  };
  report: {
    sourceLines: number;
    generatedLines: number;
    sourceBytes: number;
    generatedBytes: number;
    savedLines: number;
    savedBytes: number;
    adjustedRelativeBranches: number;
    adjustedAbsoluteBranches: number;
    substitutedDefines: number;
    replacedLabels: number;
    shortenedAliases: number;
    limits: {
      name: string;
      value?: number;
      unit: string;
      source: string;
      gameDataVersion: string;
    }[];
  };
}

export interface BuildFiles {
  code: vscode.Uri;
  sourceMap: vscode.Uri;
  metadata: vscode.Uri;
  report: vscode.Uri;
}

export async function requestBuild(
  client: LanguageClient,
  document: vscode.TextDocument,
  options: BuildOptions,
): Promise<BuildOutput> {
  return client.sendRequest<BuildOutput>("ic10/build", {
    uri: document.uri.toString(),
    options,
  });
}

export function configuredBuildOptions(
  document: vscode.TextDocument,
): BuildOptions {
  const configuration = vscode.workspace.getConfiguration("ic10.build", document.uri);
  return {
    optimization: configuration.get<OptimizationLevel>(
      "optimization",
      "readable",
    ),
    gameVersion:
      configuration.get<string>("gameVersion", "").trim() || undefined,
    sourcePath:
      document.uri.scheme === "file" ? document.uri.fsPath : undefined,
  };
}

export async function writeBuildFiles(
  document: vscode.TextDocument,
  output: BuildOutput,
): Promise<BuildFiles> {
  if (document.uri.scheme !== "file") {
    throw new Error("Save the IC10 source before writing a build artefact.");
  }
  const configuration = vscode.workspace.getConfiguration(
    "ic10.build",
    document.uri,
  );
  const configuredDirectory = configuration.get<string>(
    "outputDirectory",
    "build",
  );
  const directory = resolveBuildDirectory(
    document.uri.fsPath,
    configuredDirectory,
  );
  const basename = path.basename(document.uri.fsPath);
  await fs.promises.mkdir(directory, { recursive: true });

  const files: BuildFiles = {
    code: vscode.Uri.file(path.join(directory, basename)),
    sourceMap: vscode.Uri.file(path.join(directory, `${basename}.map.json`)),
    metadata: vscode.Uri.file(
      path.join(directory, `${basename}.metadata.json`),
    ),
    report: vscode.Uri.file(path.join(directory, `${basename}.report.json`)),
  };
  if (
    path.resolve(files.code.fsPath).toLocaleLowerCase() ===
    path.resolve(document.uri.fsPath).toLocaleLowerCase()
  ) {
    throw new Error(
      "The build output directory resolves to the source folder and would overwrite the IC10 source.",
    );
  }
  await Promise.all([
    fs.promises.writeFile(files.code.fsPath, output.code, "utf8"),
    writeJson(files.sourceMap.fsPath, output.sourceMap),
    writeJson(files.metadata.fsPath, output.metadata),
    writeJson(files.report.fsPath, output.report),
  ]);
  return files;
}

export function optimizationReport(output: BuildOutput): string {
  const unknown = output.report.limits
    .filter((limit) => limit.value === undefined)
    .map((limit) => limit.name)
    .join(", ");
  return [
    `IC10 build ${output.metadata.sourceSha256.slice(0, 12)}`,
    `${output.report.sourceLines} → ${output.report.generatedLines} lines (${output.report.savedLines} saved)`,
    `${output.report.sourceBytes} → ${output.report.generatedBytes} bytes (${output.report.savedBytes} saved)`,
    `${output.report.adjustedRelativeBranches} relative branch offsets adjusted`,
    `${output.report.adjustedAbsoluteBranches} absolute branch targets adjusted`,
    `${output.report.substitutedDefines} defines substituted, ${output.report.replacedLabels} labels replaced, ${output.report.shortenedAliases} private alias references shortened`,
    unknown.length > 0 ? `Official limits unknown: ${unknown}` : "",
  ]
    .filter(Boolean)
    .join("\n");
}

async function writeJson(file: string, value: unknown): Promise<void> {
  await fs.promises.writeFile(file, `${JSON.stringify(value, undefined, 2)}\n`);
}
