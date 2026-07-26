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

interface TraceResponse {
  readonly history: {
    readonly cursor: number;
    readonly retainedFrom: number;
    readonly retainedTo: number;
    readonly retainedEvents: number;
    readonly eventLimit: number;
    readonly retainedTicks: number;
    readonly droppedEvents: number;
  };
  readonly records: readonly {
    readonly sequence: number;
    readonly tick: number;
    readonly cpu: number;
    readonly line: number;
    readonly source: string;
    readonly eventTypes: readonly string[];
    readonly writes: readonly {
      readonly target: string;
      readonly before: string;
      readonly after: string;
    }[];
  }[];
  readonly coverage: Readonly<Record<string, readonly number[]>>;
  readonly profile: unknown;
}

export class Ic10StateViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = "ic10.state";

  private view: vscode.WebviewView | undefined;
  private threadId = 1;
  private traceFilter: string | undefined;

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
        direction?: string;
        target?: string;
        eventType?: string;
        from?: number;
        to?: number;
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
          case "navigateHistory":
            void this.navigateHistory(
              message.direction ?? "previous",
              message.target,
              message.eventType,
            );
            break;
          case "selectTraceFilter":
            this.traceFilter = message.target || undefined;
            break;
          case "stateDiff":
            if (message.from !== undefined && message.to !== undefined) {
              void this.showStateDiff(message.from, message.to);
            }
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

  public async filterTrace(targetId: string): Promise<void> {
    this.traceFilter = targetId;
    await vscode.commands.executeCommand(`${Ic10StateViewProvider.viewType}.focus`);
    await this.refresh();
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
      let trace: TraceResponse | undefined;
      try {
        trace = (await session.customRequest(
          "ic10/getTrace",
          { tail: 60 },
        )) as TraceResponse;
      } catch {
        trace = undefined;
      }
      await this.view.webview.postMessage({
        type: "state",
        state,
        trace,
        traceFilter: this.traceFilter,
      });
    } catch (error) {
      await this.view.webview.postMessage({
        type: "error",
        message: String(error),
      });
    }
  }

  private async navigateHistory(
    direction: string,
    target?: string,
    eventType?: string,
  ): Promise<void> {
    const session = activeIc10Session();
    if (!session) return;
    try {
      await session.customRequest("ic10/navigateHistory", {
        direction,
        ...(target ? { target } : {}),
        ...(eventType ? { eventType } : {}),
      });
      await this.refresh();
    } catch (error) {
      void vscode.window.showInformationMessage(String(error));
    }
  }

  private async showStateDiff(from: number, to: number): Promise<void> {
    const session = activeIc10Session();
    if (!session) return;
    try {
      const diff = await session.customRequest("ic10/stateDiff", { from, to });
      const document = await vscode.workspace.openTextDocument({
        language: "json",
        content: `${JSON.stringify(diff, null, 2)}\n`,
      });
      await vscode.window.showTextDocument(document, {
        preview: true,
        viewColumn: vscode.ViewColumn.Beside,
      });
    } catch (error) {
      void vscode.window.showErrorMessage(`Could not compare states: ${String(error)}`);
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
    .history-controls { display: grid; grid-template-columns: 1fr 1fr; gap: 4px; }
    .history-actions { display: grid; grid-template-columns: repeat(3, 1fr); gap: 4px; margin-top: 4px; }
    .timeline { max-height: 260px; overflow: auto; margin-top: 6px; font-family: var(--vscode-editor-font-family); font-size: 11px; }
    .trace-event { padding: 3px 4px; border-bottom: 1px solid var(--vscode-panel-border); }
    .trace-event.current { background: var(--vscode-list-activeSelectionBackground); }
    .writes { color: var(--vscode-descriptionForeground); overflow-wrap: anywhere; }
    .history-summary { color: var(--vscode-descriptionForeground); font-size: 11px; margin: 4px 0; }
    .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
    .value-chart { margin: 8px 0 4px; color: var(--vscode-foreground); }
    .value-chart svg { display: block; width: 100%; min-height: 112px; color: var(--vscode-charts-blue, var(--vscode-textLink-foreground)); background: var(--vscode-editor-background); border: 1px solid var(--vscode-panel-border); }
    .value-chart .chart-grid { stroke: var(--vscode-panel-border); stroke-width: 1; }
    .value-chart .chart-line { fill: none; stroke: currentColor; stroke-width: 2.5; stroke-linecap: round; stroke-linejoin: round; vector-effect: non-scaling-stroke; }
    .value-chart .chart-point { fill: var(--vscode-editor-background); stroke: currentColor; stroke-width: 2; vector-effect: non-scaling-stroke; }
    .value-chart text { fill: var(--vscode-foreground); font: 10px var(--vscode-editor-font-family); }
    details { margin: 10px 0 6px; }
    details > summary { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .06em; margin: 8px 0 6px; cursor: pointer; user-select: none; color: var(--vscode-foreground); outline: none; }
    details > summary:hover { color: var(--vscode-textLink-foreground); }
    @media (forced-colors: active) {
      .value-chart svg { color: CanvasText; border-color: CanvasText; forced-color-adjust: none; }
      .value-chart .chart-grid { stroke: GrayText; }
    }
    @media (prefers-reduced-motion: reduce) {
      .value-chart *, .value-chart *::before, .value-chart *::after { animation: none !important; transition: none !important; }
    }
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
      const trace = message.trace;
      const openStates = {
        registers: document.getElementById('detailsRegisters')?.open ?? false,
        stack: document.getElementById('detailsStack')?.open ?? false,
        history: document.getElementById('detailsHistory')?.open ?? false,
      };
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
      const traceRecords = trace ? trace.records.slice(-60).reverse() : [];
      const traceTargets = trace ? [...new Set(trace.records.flatMap((record) =>
        record.writes.map((write) => write.target)
      ))].sort() : [];
      const history = trace ? (() => {
        const targetOptions = traceTargets.map((target) =>
          '<option value="' + escapeHtml(target) + '">' + escapeHtml(target) + '</option>'
        ).join('');
        const events = traceRecords.map((record) =>
          '<div class="trace-event' + (record.sequence === trace.history.cursor ? ' current' : '') +
          '" data-sequence="' + record.sequence + '">' +
          '#' + record.sequence + ' · tick ' + record.tick + ' · IC ' + (record.cpu + 1) +
          ' · line ' + record.line + ' · ' + escapeHtml(record.eventTypes.join(', ')) +
          (record.writes.length ? '<div class="writes">' + record.writes.map((write) =>
            escapeHtml(write.target + ': ' + write.before + ' → ' + write.after)
          ).join('<br>') + '</div>' : '') + '</div>'
        ).join('');
        return '<div class="history-summary">' + trace.history.retainedEvents + ' / ' +
          trace.history.eventLimit + ' events · ' + trace.history.retainedTicks +
          ' ticks retained · ' + trace.history.droppedEvents + ' dropped</div>' +
          '<div class="history-controls"><label class="sr-only" for="historyEvent">Filter history by event type</label><select id="historyEvent" aria-label="Filter history by event type">' +
          '<option value="">All events</option><option>tick</option><option>yield</option>' +
          '<option>sleep</option><option>breakpoint</option><option>error</option>' +
          '<option>assertion</option></select><label class="sr-only" for="historyTarget">Filter history and chart by value</label><select id="historyTarget" aria-label="Filter history and chart by value">' +
          '<option value="">Select value…</option>' + targetOptions + '</select></div>' +
          '<div class="history-actions"><button id="previousHistory">◀ Previous</button>' +
          '<button id="nextHistory">Next ▶</button><button id="compareHistory">Compare…</button></div>' +
          '<div id="valueChart" class="value-chart" aria-live="polite">Choose a numeric value to see its write history.</div>' +
          '<div id="timeline" class="timeline">' + events + '</div>';
      })() : '<div class="muted">History is disabled for this launch.</div>';
      app.innerHTML =
        '<div class="toolbar"><select id="cpu">' + options + '</select>' +
        '<button id="saveInitialStack" title="Replace this IC housing’s sparse initial stack in the simulation environment with its current runtime stack">Save stack</button>' +
        '<button id="stepTick" title="Run every IC for one 0.5 second tick">Step tick</button></div>' +
        '<div class="summary">Tick ' + state.tick + ' · line ' +
        (state.cpu.line ?? '—') + (state.cpu.error ? ' · ' + escapeHtml(state.cpu.error) : '') + '</div>' +
        '<details id="detailsRegisters"' + (openStates.registers ? ' open' : '') + '><summary>Registers</summary><div class="registers">' + registers + '</div></details>' +
        '<details id="detailsStack"' + (openStates.stack ? ' open' : '') + '><summary>Stack</summary><div class="stack">' + stack + '</div></details>' +
        '<details id="detailsHistory"' + (openStates.history ? ' open' : '') + '><summary>History &amp; analysis</summary>' + history + '</details>';
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
      if (trace) {
        const eventFilter = document.getElementById('historyEvent');
        const targetFilter = document.getElementById('historyTarget');
        if (message.traceFilter) {
          targetFilter.value = traceTargets.find((target) =>
            target.includes(message.traceFilter)
          ) || '';
        }
        const updateTimeline = () => {
          const kind = eventFilter.value;
          const target = targetFilter.value;
          app.querySelectorAll('.trace-event').forEach((row, index) => {
            const record = traceRecords[index];
            row.hidden =
              (Boolean(kind) && !record.eventTypes.includes(kind)) ||
              (Boolean(target) && !record.writes.some((write) => write.target === target));
          });
          const points = target ? trace.records.flatMap((record) =>
            record.writes.filter((write) => write.target === target)
              .map((write) => ({ sequence: record.sequence, value: Number(write.after) }))
              .filter((point) => Number.isFinite(point.value))
          ) : [];
          const chart = document.getElementById('valueChart');
          if (!target) {
            chart.textContent = 'Choose a numeric value to see its write history.';
          } else if (!points.length) {
            chart.textContent = target + ' has no retained numeric writes.';
          } else {
            const visible = points.slice(-24);
            const values = visible.map((point) => point.value);
            const minimum = Math.min(...values);
            const maximum = Math.max(...values);
            const span = maximum - minimum || 1;
            const x = (index) => visible.length === 1 ? 150 :
              34 + index * 256 / (visible.length - 1);
            const y = (value) => 16 + (maximum - value) * 70 / span;
            const coordinates = visible.map((point, index) =>
              x(index).toFixed(1) + ',' + y(point.value).toFixed(1)
            ).join(' ');
            const circles = visible.map((point, index) =>
              '<circle class="chart-point" cx="' + x(index).toFixed(1) +
              '" cy="' + y(point.value).toFixed(1) + '" r="2.5"><title>Event ' +
              point.sequence + ': ' + escapeHtml(point.value) + '</title></circle>'
            ).join('');
            const label = target + ' value history, ' + visible.length +
              ' retained numeric writes, minimum ' + minimum + ', maximum ' + maximum + '.';
            chart.innerHTML = '<svg viewBox="0 0 320 112" role="img" aria-label="' +
              escapeHtml(label) + '" preserveAspectRatio="none">' +
              '<line class="chart-grid" x1="34" y1="16" x2="290" y2="16"></line>' +
              '<line class="chart-grid" x1="34" y1="86" x2="290" y2="86"></line>' +
              '<text x="2" y="20">' + escapeHtml(maximum) + '</text>' +
              '<text x="2" y="90">' + escapeHtml(minimum) + '</text>' +
              '<text x="34" y="105">#' + visible[0].sequence + '</text>' +
              '<text x="254" y="105">#' + visible[visible.length - 1].sequence + '</text>' +
              '<polyline class="chart-line" points="' + coordinates + '"></polyline>' +
              circles + '</svg>';
          }
        };
        eventFilter.addEventListener('change', updateTimeline);
        targetFilter.addEventListener('change', () => {
          vscode.postMessage({
            type: 'selectTraceFilter',
            target: targetFilter.value || undefined
          });
          updateTimeline();
        });
        updateTimeline();
        document.getElementById('previousHistory').addEventListener('click', () =>
          vscode.postMessage({ type: 'navigateHistory', direction: 'previous',
            target: targetFilter.value || undefined, eventType: eventFilter.value || undefined })
        );
        document.getElementById('nextHistory').addEventListener('click', () =>
          vscode.postMessage({ type: 'navigateHistory', direction: 'next',
            target: targetFilter.value || undefined, eventType: eventFilter.value || undefined })
        );
        document.getElementById('compareHistory').addEventListener('click', () => {
          const from = Number(prompt('First retained event', String(trace.history.retainedFrom)));
          const to = Number(prompt('Second retained event', String(trace.history.cursor)));
          if (Number.isFinite(from) && Number.isFinite(to)) {
            vscode.postMessage({ type: 'stateDiff', from, to });
          }
        });
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
