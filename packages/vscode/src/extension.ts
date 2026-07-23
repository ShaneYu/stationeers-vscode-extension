import * as fs from "node:fs";
import * as path from "node:path";

import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let outputChannel: vscode.LogOutputChannel | undefined;

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  outputChannel = vscode.window.createOutputChannel("Stationeers IC10", {
    log: true,
  });
  context.subscriptions.push(outputChannel);
  context.subscriptions.push(
    vscode.commands.registerCommand("ic10.restartServer", async () => {
      await stopClient();
      await startClient(context);
      void vscode.window.showInformationMessage(
        "IC10 language server restarted.",
      );
    }),
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
    synchronize: { fileEvents: fileWatcher },
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
    },
  };

  client = new LanguageClient(
    "ic10",
    "Stationeers IC10",
    serverOptions,
    clientOptions,
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
