import * as vscode from "vscode";

import { debugType } from "./debug";

interface StateResponse {
  readonly threadId: number;
  readonly tick: number;
  readonly cpu: {
    readonly id: string;
    readonly name: string;
    readonly line?: number;
    readonly state: string;
    readonly error?: string;
  };
  readonly cpus: readonly {
    readonly threadId: number;
    readonly id: string;
    readonly name: string;
    readonly line?: number;
    readonly state: string;
  }[];
  readonly registers: readonly {
    readonly name: string;
    readonly value: string;
  }[];
  readonly stack: readonly string[];
}

export class Ic10StateViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = "ic10.state";

  private view: vscode.WebviewView | undefined;
  private threadId = 1;

  public constructor(private readonly context: vscode.ExtensionContext) {
    context.subscriptions.push(
      vscode.debug.onDidChangeActiveDebugSession(() => void this.refresh()),
      vscode.debug.onDidReceiveDebugSessionCustomEvent((event) => {
        if (
          event.session.type === debugType &&
          event.event === "ic10/stateChanged"
        ) {
          void this.refresh();
        }
      }),
      vscode.debug.onDidTerminateDebugSession((session) => {
        if (session.type === debugType) {
          void this.refresh();
        }
      }),
    );
  }

  public resolveWebviewView(view: vscode.WebviewView): void {
    this.view = view;
    view.webview.options = { enableScripts: true };
    view.webview.html = stateViewHtml(view.webview, this.context.extensionUri);
    view.webview.onDidReceiveMessage(
      (message: {
        type: string;
        threadId?: number;
        name?: string;
        address?: number;
        value?: string;
      }) => {
        switch (message.type) {
          case "ready":
            void this.refresh();
            break;
          case "selectThread":
            if (message.threadId) {
              this.threadId = message.threadId;
              void this.refresh();
            }
            break;
          case "setRegister":
            if (message.name && message.value !== undefined) {
              void this.setState({
                registers: { [message.name]: message.value },
              });
            }
            break;
          case "setStack":
            if (
              message.address !== undefined &&
              message.value !== undefined
            ) {
              void this.setState({
                stack: { [String(message.address)]: message.value },
              });
            }
            break;
          case "stepTick":
            void this.stepWorldTick();
            break;
          case "saveInitialStack":
            void this.saveInitialStack();
            break;
        }
      },
      undefined,
      this.context.subscriptions,
    );
  }

  public async stepWorldTick(): Promise<void> {
    const session = activeIc10Session();
    if (!session) {
      void vscode.window.showInformationMessage(
        "Start an IC10 debug session before stepping the world.",
      );
      return;
    }
    try {
      await session.customRequest("ic10/stepTick", {
        threadId: this.threadId,
      });
    } catch (error) {
      void vscode.window.showErrorMessage(
        `Could not step the IC10 world: ${String(error)}`,
      );
    }
  }

  public async refresh(): Promise<void> {
    if (!this.view) {
      return;
    }
    const session = activeIc10Session();
    if (!session) {
      await this.view.webview.postMessage({ type: "inactive" });
      return;
    }
    try {
      const state = (await session.customRequest("ic10/getState", {
        threadId: this.threadId,
      })) as StateResponse;
      await this.view.webview.postMessage({ type: "state", state });
    } catch (error) {
      await this.view.webview.postMessage({
        type: "error",
        message: String(error),
      });
    }
  }

  private async setState(change: {
    registers?: Record<string, string>;
    stack?: Record<string, string>;
  }): Promise<void> {
    const session = activeIc10Session();
    if (!session) {
      return;
    }
    try {
      await session.customRequest("ic10/setState", {
        threadId: this.threadId,
        ...change,
      });
      await this.refresh();
    } catch (error) {
      void vscode.window.showErrorMessage(
        `Could not edit IC10 state: ${String(error)}`,
      );
      await this.refresh();
    }
  }

  private async saveInitialStack(): Promise<void> {
    const session = activeIc10Session();
    if (!session) {
      return;
    }
    const scenarioPath = session.configuration.scenario;
    if (typeof scenarioPath !== "string" || !scenarioPath) {
      void vscode.window.showErrorMessage(
        "The active IC10 debug session has no simulation environment path.",
      );
      return;
    }
    try {
      const state = (await session.customRequest("ic10/getState", {
        threadId: this.threadId,
      })) as StateResponse;
      const confirmed = await vscode.window.showWarningMessage(
        `Replace the sparse initial stack for ${state.cpu.name} with its current non-zero runtime cells?`,
        { modal: true },
        "Save Stack",
      );
      if (confirmed !== "Save Stack") {
        return;
      }
      const uri = vscode.Uri.file(scenarioPath);
      const document = await vscode.workspace.openTextDocument(uri);
      const scenario = JSON.parse(document.getText()) as {
        devices?: {
          id?: string;
          ic?: {
            stack?: Record<string, number | string>;
          };
        }[];
      };
      const device = scenario.devices?.find(
        (candidate) => candidate.id === state.cpu.id && candidate.ic,
      );
      if (!device?.ic) {
        throw new Error(
          `Simulation environment has no IC housing with stable ID “${state.cpu.id}”.`,
        );
      }
      device.ic.stack = Object.fromEntries(
        state.stack.flatMap((value, address) => {
          if (value === "0") {
            return [];
          }
          return [[String(address), scenarioNumber(value)]];
        }),
      );
      const replacement = `${JSON.stringify(scenario, null, 2)}\n`;
      const edit = new vscode.WorkspaceEdit();
      edit.replace(
        uri,
        new vscode.Range(
          document.positionAt(0),
          document.positionAt(document.getText().length),
        ),
        replacement,
      );
      if (!(await vscode.workspace.applyEdit(edit)) || !(await document.save())) {
        throw new Error("VS Code could not save the simulation environment.");
      }
      void vscode.window.showInformationMessage(
        `Saved ${Object.keys(device.ic.stack).length} non-zero stack cells from ${state.cpu.name} as its initial stack.`,
      );
    } catch (error) {
      void vscode.window.showErrorMessage(
        `Could not save the runtime stack: ${String(error)}`,
      );
    }
  }
}

function activeIc10Session(): vscode.DebugSession | undefined {
  const session = vscode.debug.activeDebugSession;
  return session?.type === debugType ? session : undefined;
}

function scenarioNumber(value: string): number | string {
  if (["NaN", "Infinity", "-Infinity", "-0"].includes(value)) {
    return value;
  }
  const numeric = Number(value);
  return Number.isNaN(numeric) ? value : numeric;
}

function stateViewHtml(
  webview: vscode.Webview,
  _extensionUri: vscode.Uri,
): string {
  const nonce = getNonce();
  return /* html */ `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>IC10 State</title>
  <style>
    body { padding: 0 10px 12px; color: var(--vscode-foreground); font-family: var(--vscode-font-family); }
    .toolbar { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 6px; position: sticky; top: 0; padding: 8px 0; background: var(--vscode-sideBar-background); z-index: 2; }
    select, input, button { color: var(--vscode-input-foreground); background: var(--vscode-input-background); border: 1px solid var(--vscode-input-border, transparent); font: inherit; }
    select, button { min-height: 26px; }
    button { padding: 2px 8px; color: var(--vscode-button-foreground); background: var(--vscode-button-background); cursor: pointer; }
    button:hover { background: var(--vscode-button-hoverBackground); }
    .summary { color: var(--vscode-descriptionForeground); margin: 2px 0 10px; }
    h3 { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; margin: 12px 0 6px; }
    .registers { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 3px; }
    .cell { display: grid; grid-template-columns: 34px 1fr; align-items: center; min-width: 0; }
    .cell label { color: var(--vscode-symbolIcon-variableForeground); font-family: var(--vscode-editor-font-family); }
    .cell input { width: 100%; min-width: 0; height: 24px; box-sizing: border-box; padding: 2px 5px; font-family: var(--vscode-editor-font-family); }
    .stack { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 3px; max-height: 390px; overflow: auto; }
    .muted { color: var(--vscode-descriptionForeground); padding: 12px 0; }
    .error { color: var(--vscode-errorForeground); }
    @media (min-width: 430px) {
      .registers { grid-template-columns: repeat(3, minmax(0, 1fr)); }
      .stack { grid-template-columns: repeat(3, minmax(0, 1fr)); }
    }
  </style>
</head>
<body>
  <div id="app" class="muted">Start an IC10 debug session to inspect state.</div>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const app = document.getElementById('app');
    const escapeHtml = (value) => String(value)
      .replaceAll('&', '&amp;').replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;').replaceAll('"', '&quot;');

    window.addEventListener('message', (event) => {
      const message = event.data;
      if (message.type === 'inactive') {
        app.className = 'muted';
        app.textContent = 'Start an IC10 debug session to inspect state.';
        return;
      }
      if (message.type === 'error') {
        app.className = 'error';
        app.textContent = message.message;
        return;
      }
      if (message.type !== 'state') return;
      const state = message.state;
      app.className = '';
      const options = state.cpus.map((cpu) =>
        '<option value="' + cpu.threadId + '"' +
        (cpu.threadId === state.threadId ? ' selected' : '') + '>' +
        escapeHtml(cpu.name) + ' — ' + escapeHtml(cpu.state) + '</option>'
      ).join('');
      const registers = state.registers.map((register) =>
        '<div class="cell"><label>' + escapeHtml(register.name) + '</label>' +
        '<input data-register="' + escapeHtml(register.name) + '" value="' +
        escapeHtml(register.value) + '"></div>'
      ).join('');
      const stack = state.stack.map((value, address) =>
        '<div class="cell"><label>' + address + '</label>' +
        '<input data-stack="' + address + '" value="' + escapeHtml(value) + '"></div>'
      ).join('');
      app.innerHTML =
        '<div class="toolbar"><select id="cpu">' + options + '</select>' +
        '<button id="saveInitialStack" title="Replace this IC housing’s sparse initial stack in the simulation environment with its current runtime stack">Save stack</button>' +
        '<button id="stepTick" title="Run every IC for one 0.5 second tick">Step tick</button></div>' +
        '<div class="summary">Tick ' + state.tick + ' · line ' +
        (state.cpu.line ?? '—') + (state.cpu.error ? ' · ' + escapeHtml(state.cpu.error) : '') + '</div>' +
        '<h3>Registers</h3><div class="registers">' + registers + '</div>' +
        '<h3>Stack</h3><div class="stack">' + stack + '</div>';
      document.getElementById('cpu').addEventListener('change', (event) =>
        vscode.postMessage({ type: 'selectThread', threadId: Number(event.target.value) })
      );
      document.getElementById('stepTick').addEventListener('click', () =>
        vscode.postMessage({ type: 'stepTick' })
      );
      document.getElementById('saveInitialStack').addEventListener('click', () =>
        vscode.postMessage({ type: 'saveInitialStack' })
      );
      app.querySelectorAll('[data-register]').forEach((input) =>
        input.addEventListener('change', () => vscode.postMessage({
          type: 'setRegister', name: input.dataset.register, value: input.value
        }))
      );
      app.querySelectorAll('[data-stack]').forEach((input) =>
        input.addEventListener('change', () => vscode.postMessage({
          type: 'setStack', address: Number(input.dataset.stack), value: input.value
        }))
      );
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
