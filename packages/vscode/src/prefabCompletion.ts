import * as fs from "node:fs";
import * as vscode from "vscode";

interface PrefabEntry {
  prefabName: string;
  prefabHash: number;
  displayName: string;
}

interface CatalogFile {
  resources?: Record<string, PrefabEntry>;
  devices?: Record<string, PrefabEntry>;
}

const triggerCharacters = [
  ..."abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_",
];

export function registerPrefabCompletion(
  context: vscode.ExtensionContext,
): void {
  const entries = loadEntries(context);
  const provider: vscode.CompletionItemProvider = {
    provideCompletionItems(document, position) {
      const match = hashLiteralAt(document, position);
      if (!match) return undefined;

      return entries.map((entry) => {
        const item = new vscode.CompletionItem(
          entry.prefabName,
          vscode.CompletionItemKind.Reference,
        );
        item.detail = `${entry.displayName} · PrefabHash ${entry.prefabHash}`;
        item.documentation = `Insert ${entry.prefabName}; its PrefabHash is ${entry.prefabHash}.`;
        item.filterText = `${entry.prefabName} ${entry.displayName} ${entry.prefabHash}`;
        item.range = match.range;
        item.insertText = entry.prefabName;
        return item;
      });
    },
  };

  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      { language: "ic10", scheme: "file" },
      provider,
      ...triggerCharacters,
    ),
    vscode.languages.registerCompletionItemProvider(
      { language: "ic10", scheme: "untitled" },
      provider,
      ...triggerCharacters,
    ),
  );
}

function loadEntries(context: vscode.ExtensionContext): PrefabEntry[] {
  const result: PrefabEntry[] = [];
  for (const file of ["reference/devices.json", "reference/resources.json"]) {
    try {
      const catalog = JSON.parse(
        fs.readFileSync(context.asAbsolutePath(file), "utf8"),
      ) as CatalogFile;
      result.push(...Object.values(catalog.devices ?? {}));
      result.push(...Object.values(catalog.resources ?? {}));
    } catch {
      // The language server remains the fallback if catalog loading fails.
    }
  }
  return result;
}

function hashLiteralAt(
  document: vscode.TextDocument,
  position: vscode.Position,
): { range: vscode.Range } | undefined {
  const line = document.lineAt(position.line).text;
  const before = line.slice(0, position.character);
  const start = before.lastIndexOf('HASH("');
  if (start < 0) return undefined;

  const contentStart = start + 'HASH("'.length;
  const closingQuote = line.indexOf('"', contentStart);
  const contentEnd = closingQuote < 0 ? line.length : closingQuote;
  if (position.character < contentStart || position.character > contentEnd + 1)
    return undefined;

  return {
    range: new vscode.Range(
      position.line,
      contentStart,
      position.line,
      contentEnd,
    ),
  };
}
