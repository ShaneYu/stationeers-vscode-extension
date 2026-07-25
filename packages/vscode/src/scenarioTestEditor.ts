import * as path from "node:path";

import * as vscode from "vscode";

import {
  newScenarioTestFixture,
  scenarioPathForTest,
  validateScenarioTestFixture,
} from "./scenarioTestEditorModel";
import { resolveScenarioProgramPath } from "./scenarioUri";

export class Ic10ScenarioTestEditorProvider
  implements vscode.CustomTextEditorProvider
{
  public static readonly viewType = "ic10.scenarioTest";

  public async resolveCustomTextEditor(
    document: vscode.TextDocument,
    panel: vscode.WebviewPanel,
  ): Promise<void> {
    panel.webview.options = { enableScripts: true };
    panel.webview.html = scenarioTestEditorHtml(panel.webview);

    const update = async (): Promise<void> => {
      try {
        const fixture: unknown = JSON.parse(document.getText());
        await panel.webview.postMessage({
          type: "update",
          fixture,
          scenarios: await findScenarios(document.uri),
          validation: validateScenarioTestFixture(fixture),
        });
      } catch (error) {
        await panel.webview.postMessage({
          type: "parseError",
          message: `The JSON source cannot be parsed: ${String(error)}`,
        });
      }
    };

    const writeFixture = async (fixture: unknown): Promise<boolean> => {
      const validation = validateScenarioTestFixture(fixture);
      await panel.webview.postMessage({ type: "validation", validation });
      if (validation.length > 0) {
        return false;
      }
      const replacement = `${JSON.stringify(fixture, null, 2)}\n`;
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
      if (!(await vscode.workspace.applyEdit(edit))) {
        await panel.webview.postMessage({
          type: "validation",
          validation: ["VS Code could not apply the test-file edit."],
        });
        return false;
      }
      return true;
    };

    const changes = vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.uri.toString() === document.uri.toString()) {
        void update();
      }
    });
    const watcher = vscode.workspace.createFileSystemWatcher(
      "**/*.ic10sim.json",
    );
    const refreshScenarios = (): void => {
      void findScenarios(document.uri).then((scenarios) =>
        panel.webview.postMessage({ type: "scenarios", scenarios }),
      );
    };
    const created = watcher.onDidCreate(refreshScenarios);
    const deleted = watcher.onDidDelete(refreshScenarios);
    panel.onDidDispose(() => {
      changes.dispose();
      created.dispose();
      deleted.dispose();
      watcher.dispose();
    });

    panel.webview.onDidReceiveMessage(async (message: {
      readonly type: string;
      readonly fixture?: unknown;
      readonly scenario?: string;
    }) => {
      switch (message.type) {
        case "ready":
          await update();
          break;
        case "save":
          await writeFixture(message.fixture);
          break;
        case "saveNow":
          if (await writeFixture(message.fixture)) {
            await document.save();
          }
          break;
        case "openJson":
          await vscode.commands.executeCommand(
            "vscode.openWith",
            document.uri,
            "default",
          );
          break;
        case "browseScenario": {
          const selected = (
            await vscode.window.showOpenDialog({
              canSelectFiles: true,
              canSelectFolders: false,
              canSelectMany: false,
              defaultUri: document.uri.with({
                path: path.posix.dirname(document.uri.path),
              }),
              filters: {
                "IC10 simulation environments": ["ic10sim.json"],
              },
              openLabel: "Use Simulation Environment",
            })
          )?.[0];
          if (selected) {
            await panel.webview.postMessage({
              type: "scenarioSelected",
              scenario: scenarioPathForTest(
                document.uri.fsPath,
                selected.fsPath,
              ),
            });
          }
          break;
        }
        case "openScenario":
          if (message.scenario) {
            const scenario = resolveRelative(document.uri, message.scenario);
            await vscode.commands.executeCommand(
              "vscode.openWith",
              scenario,
              "ic10.environment",
            );
          }
          break;
      }
    });
  }
}

export async function createScenarioTest(): Promise<void> {
  const active = vscode.window.activeTextEditor?.document;
  let selectedScenario: vscode.Uri | undefined;
  if (active?.uri.fsPath.endsWith(".ic10sim.json")) {
    selectedScenario = active.uri;
  } else {
    const choice = await chooseScenario();
    if (choice === undefined) {
      return;
    }
    selectedScenario = choice ?? undefined;
  }
  const workspaceFolder =
    (selectedScenario &&
      vscode.workspace.getWorkspaceFolder(selectedScenario)) ??
    (active && vscode.workspace.getWorkspaceFolder(active.uri)) ??
    vscode.workspace.workspaceFolders?.[0];
  const defaultName = selectedScenario
    ? `${path.basename(selectedScenario.fsPath, ".ic10sim.json")}.ic10test.json`
    : "scenario.ic10test.json";
  const destination = await vscode.window.showSaveDialog({
    defaultUri: selectedScenario
      ? vscode.Uri.joinPath(selectedScenario, "..", defaultName)
      : workspaceFolder
      ? vscode.Uri.joinPath(workspaceFolder.uri, defaultName)
      : undefined,
    filters: { "IC10 scenario tests": ["ic10test.json"] },
    saveLabel: "Create Scenario Test",
  });
  if (!destination) {
    return;
  }
  const scenario = selectedScenario
    ? scenarioPathForTest(destination.fsPath, selectedScenario.fsPath)
    : "";
  await vscode.workspace.fs.writeFile(
    destination,
    Buffer.from(
      `${JSON.stringify(newScenarioTestFixture(scenario), null, 2)}\n`,
      "utf8",
    ),
  );
  await vscode.commands.executeCommand(
    "vscode.openWith",
    destination,
    Ic10ScenarioTestEditorProvider.viewType,
  );
}

async function chooseScenario(): Promise<vscode.Uri | null | undefined> {
  const scenarios = await vscode.workspace.findFiles(
    "**/*.ic10sim.json",
    "**/{node_modules,target,dist}/**",
    500,
  );
  if (scenarios.length === 0) {
    return null;
  }
  const choices = scenarios
    .map((uri) => ({
      label:
        vscode.workspace.asRelativePath(uri, false) || path.basename(uri.fsPath),
      description: vscode.workspace.getWorkspaceFolder(uri)?.name,
      uri,
    }))
    .sort((left, right) => left.label.localeCompare(right.label));
  return (
    await vscode.window.showQuickPick(choices, {
      placeHolder: "Choose the simulation environment to test",
      title: "Create IC10 Scenario Test",
    })
  )?.uri;
}

async function findScenarios(test: vscode.Uri): Promise<string[]> {
  const scenarios = await vscode.workspace.findFiles(
    "**/*.ic10sim.json",
    "**/{node_modules,target,dist}/**",
    500,
  );
  return scenarios
    .map((scenario) => scenarioPathForTest(test.fsPath, scenario.fsPath))
    .sort((left, right) => left.localeCompare(right));
}

function resolveRelative(base: vscode.Uri, value: string): vscode.Uri {
  if (/^[a-z][a-z0-9+.-]*:/i.test(value)) {
    return vscode.Uri.parse(value, true);
  }
  const resolved = resolveScenarioProgramPath(base, value);
  return base.with({ path: resolved.path });
}

function scenarioTestEditorHtml(webview: vscode.Webview): string {
  const nonce = createNonce();
  return /* html */ `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>IC10 Scenario Test</title>
  <style>
    * { box-sizing: border-box; }
    body { margin: 0; color: var(--vscode-foreground); background: var(--vscode-editor-background); font-family: var(--vscode-font-family); }
    button, input, select, textarea { font: inherit; color: var(--vscode-input-foreground); background: var(--vscode-input-background); border: 1px solid var(--vscode-input-border, transparent); }
    button { padding: 5px 10px; color: var(--vscode-button-foreground); background: var(--vscode-button-background); cursor: pointer; }
    button:hover { background: var(--vscode-button-hoverBackground); }
    button.secondary { color: var(--vscode-foreground); background: var(--vscode-button-secondaryBackground); }
    button.danger { color: var(--vscode-errorForeground); background: transparent; border-color: var(--vscode-errorForeground); }
    button:disabled { cursor: default; opacity: .55; }
    input, select { min-height: 28px; padding: 4px 6px; }
    textarea { min-height: 54px; padding: 5px 7px; resize: vertical; font-family: var(--vscode-editor-font-family); }
    .toolbar { position: sticky; z-index: 20; top: 0; display: flex; gap: 7px; align-items: center; padding: 9px 12px; border-bottom: 1px solid var(--vscode-panel-border); background: var(--vscode-sideBar-background); }
    .toolbar strong { margin-right: auto; }
    .layout { display: grid; grid-template-columns: 250px minmax(480px, 1fr); min-height: calc(100vh - 48px); }
    .sidebar { padding: 12px; border-right: 1px solid var(--vscode-panel-border); background: var(--vscode-sideBar-background); }
    .sidebar-actions { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; margin-bottom: 10px; }
    .case-item { width: 100%; display: flex; justify-content: space-between; gap: 8px; margin: 3px 0; padding: 7px 8px; text-align: left; color: var(--vscode-foreground); background: transparent; border: 1px solid transparent; }
    .case-item:hover, .case-item.active { color: var(--vscode-list-activeSelectionForeground); background: var(--vscode-list-activeSelectionBackground); }
    .case-item span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .main { padding: 16px 22px 70px; overflow: auto; }
    .fixture { display: grid; grid-template-columns: 150px minmax(240px, 1fr) auto auto; gap: 7px 9px; align-items: center; max-width: 980px; margin-bottom: 18px; }
    .fixture label, .field label { color: var(--vscode-descriptionForeground); }
    .fixture input, .fixture select { width: 100%; }
    h2 { margin: 0 0 14px; font-size: 18px; }
    h3 { margin: 22px 0 9px; padding-bottom: 5px; border-bottom: 1px solid var(--vscode-panel-border); font-size: 12px; letter-spacing: .06em; text-transform: uppercase; }
    .fields { display: grid; grid-template-columns: repeat(2, minmax(190px, 1fr)); gap: 9px 14px; max-width: 980px; }
    .field { display: grid; gap: 4px; }
    .field.wide { grid-column: 1 / -1; }
    .hint { margin: 4px 0 10px; color: var(--vscode-descriptionForeground); font-size: 12px; }
    .section-head { display: flex; justify-content: space-between; align-items: center; max-width: 980px; margin-top: 20px; border-bottom: 1px solid var(--vscode-panel-border); }
    .section-head h3 { margin: 0; border: 0; }
    .card { max-width: 980px; margin: 8px 0; padding: 10px; border: 1px solid var(--vscode-panel-border); background: var(--vscode-editorWidget-background); }
    .card-head { display: flex; justify-content: space-between; align-items: center; gap: 8px; margin-bottom: 8px; }
    .card-grid { display: grid; grid-template-columns: repeat(4, minmax(120px, 1fr)); gap: 7px; }
    .pair { display: grid; grid-template-columns: minmax(180px, 1.2fr) minmax(120px, 1fr) auto; gap: 6px; margin: 5px 0; }
    .pair input { width: 100%; }
    .empty { max-width: 980px; padding: 12px; color: var(--vscode-descriptionForeground); border: 1px dashed var(--vscode-panel-border); }
    .validation { margin: 0; padding: 8px 12px; color: var(--vscode-errorForeground); background: var(--vscode-inputValidation-errorBackground); border-bottom: 1px solid var(--vscode-inputValidation-errorBorder); }
    .validation[hidden] { display: none; }
    .validation ul { margin: 4px 0; padding-left: 20px; }
    .parse-error { padding: 24px; color: var(--vscode-errorForeground); }
    .check { display: flex; align-items: center; gap: 7px; min-height: 28px; }
    .check input { min-height: auto; }
    @media (max-width: 780px) {
      .layout { grid-template-columns: 1fr; }
      .sidebar { border-right: 0; border-bottom: 1px solid var(--vscode-panel-border); }
      .fields, .card-grid { grid-template-columns: 1fr; }
      .fixture { grid-template-columns: 1fr auto auto; }
      .fixture label { grid-column: 1 / -1; }
      .pair { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <div class="toolbar">
    <strong>IC10 Scenario Test</strong>
    <span id="saveState" class="hint">Loading…</span>
    <button id="saveNow">Save</button>
    <button id="openJson" class="secondary">Open JSON</button>
  </div>
  <div id="validation" class="validation" hidden></div>
  <div id="app"><div class="empty">Loading test fixture…</div></div>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const app = document.getElementById('app');
    const validationElement = document.getElementById('validation');
    const saveState = document.getElementById('saveState');
    let fixture;
    let scenarios = [];
    let selectedCase = 0;
    let saveTimer;
    const escapeHtml = (value) => String(value ?? '')
      .replaceAll('&', '&amp;').replaceAll('<', '&lt;')
      .replaceAll('>', '&gt;').replaceAll('"', '&quot;');
    const scalarText = (value) => typeof value === 'string' ? value : String(value ?? '');
    const parseScalar = (value) => {
      const text = String(value).trim();
      if (['NaN', 'Infinity', '-Infinity', '-0'].includes(text)) return text;
      if (text !== '' && Number.isFinite(Number(text))) return Number(text);
      return text;
    };
    const parseParameter = (value) => {
      const text = String(value).trim();
      if (text === 'true') return true;
      if (text === 'false') return false;
      return parseScalar(text);
    };
    const ensure = (value, fallback) => value ?? fallback;
    const uniqueCaseName = (base) => {
      const names = fixture.cases.map((testCase) => testCase.name);
      let name = base; let suffix = 2;
      while (names.includes(name)) name = base + ' ' + suffix++;
      return name;
    };
    function showValidation(errors) {
      validationElement.hidden = !errors?.length;
      validationElement.innerHTML = errors?.length
        ? '<strong>Resolve these guard-rail checks before the JSON is saved:</strong><ul>' +
          errors.map((error) => '<li>' + escapeHtml(error) + '</li>').join('') + '</ul>'
        : '';
      saveState.textContent = errors?.length ? 'Needs attention' : 'Ready';
    }
    function queueSave() {
      clearTimeout(saveTimer);
      saveState.textContent = 'Checking…';
      saveTimer = setTimeout(() => {
        vscode.postMessage({ type: 'save', fixture });
      }, 350);
    }
    function pairRows(values, group, parent = '') {
      const entries = Object.entries(values || {});
      if (!entries.length) return '<div class="hint">No values configured.</div>';
      return entries.map(([key, value], index) =>
        '<div class="pair"><input data-pair-key data-group="' + group +
        '" data-parent="' + parent + '" data-index="' + index + '" value="' +
        escapeHtml(key) + '" aria-label="Target or parameter name">' +
        '<input data-pair-value data-group="' + group + '" data-parent="' + parent +
        '" data-index="' + index + '" value="' + escapeHtml(scalarText(value)) +
        '" aria-label="Value"><button class="danger" data-pair-delete data-group="' +
        group + '" data-parent="' + parent + '" data-index="' + index +
        '" title="Delete value">×</button></div>'
      ).join('');
    }
    function assertionType(assertion) {
      if (assertion.eventually !== undefined) return 'eventually';
      if (assertion.always !== undefined) return 'always';
      return 'expression';
    }
    function renderAssertions(testCase) {
      const assertions = ensure(testCase.expect, []);
      return assertions.map((assertion, index) => {
        const type = assertionType(assertion);
        const expression = assertion[type] ?? '';
        return '<div class="card"><div class="card-head"><strong>Assertion ' + (index + 1) +
          '</strong><button class="danger" data-delete-assertion="' + index + '">Delete</button></div>' +
          '<div class="card-grid"><div class="field"><label>Kind</label><select data-assertion-type="' +
          index + '">' + ['expression', 'eventually', 'always'].map((candidate) =>
            '<option' + (candidate === type ? ' selected' : '') + '>' + candidate +
            '</option>').join('') + '</select></div>' +
          '<div class="field" style="grid-column:span 3"><label>Expression</label><input data-assertion-expression="' +
          index + '" value="' + escapeHtml(expression) + '"></div>' +
          (type === 'expression'
            ? '<div class="field"><label>Expected (optional)</label><input data-assertion-expected="' +
              index + '" value="' + escapeHtml(scalarText(assertion.expected)) + '"></div>' +
              '<div class="field"><label>At tick (optional)</label><input type="number" min="0" data-assertion-at="' +
              index + '" value="' + escapeHtml(assertion.atTick ?? '') + '"></div>'
            : '') +
          (type === 'eventually'
            ? '<div class="field"><label>Within ticks</label><input type="number" min="0" data-assertion-within="' +
              index + '" value="' + escapeHtml(assertion.withinTicks ?? '') + '"></div>'
            : '') +
          '<div class="field"><label>Absolute tolerance</label><input type="number" min="0" step="any" data-assertion-absolute="' +
          index + '" value="' + escapeHtml(assertion.tolerance?.absolute ?? '') + '"></div>' +
          '<div class="field"><label>Relative tolerance</label><input type="number" min="0" step="any" data-assertion-relative="' +
          index + '" value="' + escapeHtml(assertion.tolerance?.relative ?? '') + '"></div></div></div>';
      }).join('') || '<div class="empty">No assertions yet. Add an exact, eventual, or invariant check.</div>';
    }
    function renderTimeline(testCase) {
      const timeline = ensure(testCase.timeline, []);
      return timeline.map((entry, index) =>
        '<div class="card"><div class="card-head"><strong>Timeline entry ' + (index + 1) +
        '</strong><button class="danger" data-delete-timeline="' + index + '">Delete</button></div>' +
        '<div class="field"><label>Tick</label><input type="number" min="0" data-timeline-tick="' +
        index + '" value="' + escapeHtml(entry.tick ?? 0) + '"></div>' +
        '<div class="card-head"><strong>Set state</strong><button class="secondary" data-add-pair="timelineSet" data-parent="' +
        index + '">Add value</button></div>' + pairRows(entry.set, 'timelineSet', String(index)) +
        '<div class="card-head"><strong>Events</strong><button class="secondary" data-add-event="' +
        index + '">Add event</button></div>' +
        ensure(entry.events, []).map((event, eventIndex) =>
          '<div class="pair"><input data-event-target="' + index + ':' + eventIndex +
          '" value="' + escapeHtml(event.target) + '" placeholder="target">' +
          '<input data-event-value="' + index + ':' + eventIndex + '" value="' +
          escapeHtml(scalarText(event.value)) + '" placeholder="value"><button class="danger" data-delete-event="' +
          index + ':' + eventIndex + '">×</button></div>').join('') +
        '</div>'
      ).join('') || '<div class="empty">No scheduled stimuli. Add a timeline entry to change state at a tick.</div>';
    }
    function renderParameters(testCase) {
      const parameters = ensure(testCase.parameters, []);
      return parameters.map((parameter, index) => {
        const values = Object.fromEntries(Object.entries(parameter).filter(([key]) => key !== 'name'));
        return '<div class="card"><div class="card-head"><strong>Parameter set ' + (index + 1) +
          '</strong><button class="danger" data-delete-parameter="' + index + '">Delete</button></div>' +
          '<div class="field"><label>Display name (optional)</label><input data-parameter-name="' +
          index + '" value="' + escapeHtml(parameter.name ?? '') + '"></div>' +
          '<div class="card-head"><strong>Values</strong><button class="secondary" data-add-pair="parameter" data-parent="' +
          index + '">Add value</button></div>' + pairRows(values, 'parameter', String(index)) + '</div>';
      }).join('') || '<div class="empty">No parameter table. Add a set to run this case with substituted values.</div>';
    }
    function render() {
      if (!fixture) return;
      fixture.cases ??= [];
      if (selectedCase >= fixture.cases.length) selectedCase = Math.max(0, fixture.cases.length - 1);
      const scenarioOptions = Array.from(new Set([fixture.scenario, ...scenarios].filter(Boolean)))
        .map((scenario) => '<option value="' + escapeHtml(scenario) + '"' +
          (scenario === fixture.scenario ? ' selected' : '') + '>' +
          escapeHtml(scenario) + '</option>').join('');
      const sidebar = '<aside class="sidebar"><div class="sidebar-actions"><button id="addCase">Add case</button>' +
        '<button id="duplicateCase" class="secondary"' + (!fixture.cases.length ? ' disabled' : '') +
        '>Duplicate</button></div><div id="caseList">' + fixture.cases.map((testCase, index) =>
          '<button class="case-item' + (index === selectedCase ? ' active' : '') +
          '" data-case="' + index + '"><span>' + escapeHtml(testCase.name || 'Unnamed case') +
          '</span><small>' + escapeHtml(ensure(testCase.maxTicks, 100)) + ' ticks</small></button>'
        ).join('') + '</div></aside>';
      const testCase = fixture.cases[selectedCase];
      const fixtureForm = '<div class="fixture"><label>Simulation environment</label><select id="scenario">' +
        (scenarioOptions || '<option value="">Choose a scenario…</option>') +
        '</select><button id="browseScenario" class="secondary">Browse…</button><button id="openScenario" class="secondary">Open</button>' +
        '<label>Deterministic seed</label><input id="seed" type="number" min="0" step="1" value="' +
        escapeHtml(ensure(fixture.seed, 0)) + '"><span></span><span></span></div>';
      if (!testCase) {
        app.innerHTML = '<div class="layout">' + sidebar + '<main class="main">' +
          fixtureForm + '<div class="empty">Add a test case to begin.</div></main></div>';
        bind();
        return;
      }
      testCase.initial ??= {}; testCase.timeline ??= []; testCase.expect ??= [];
      testCase.parameters ??= [];
      const main = '<main class="main">' + fixtureForm +
        '<div class="section-head"><h2>Case ' + (selectedCase + 1) +
        '</h2><button id="deleteCase" class="danger">Delete case</button></div>' +
        '<div class="fields"><div class="field wide"><label>Name</label><input id="caseName" value="' +
        escapeHtml(testCase.name) + '"></div><div class="field"><label>Focus IC (optional)</label><input id="focusIc" value="' +
        escapeHtml(testCase.focusIc ?? '') + '" placeholder="housing ID"></div>' +
        '<div class="field"><label>Maximum ticks</label><input id="maxTicks" type="number" min="1" value="' +
        escapeHtml(ensure(testCase.maxTicks, 100)) + '"></div><div class="field"><label>Maximum operations</label><input id="maxOperations" type="number" min="1" value="' +
        escapeHtml(ensure(testCase.maxOperations, 100000)) + '"></div></div>' +
        '<div class="section-head"><h3>Initial state</h3><button class="secondary" data-add-pair="initial">Add value</button></div>' +
        '<p class="hint">Use targets such as r0, stack[3], device("sensor").Setting, or network("data").Channel0.</p>' +
        pairRows(testCase.initial, 'initial') +
        '<div class="section-head"><h3>Assertions</h3><button id="addAssertion" class="secondary">Add assertion</button></div>' +
        renderAssertions(testCase) +
        '<div class="section-head"><h3>Timeline</h3><button id="addTimeline" class="secondary">Add entry</button></div>' +
        renderTimeline(testCase) +
        '<div class="section-head"><h3>Parameters</h3><button id="addParameter" class="secondary">Add set</button></div>' +
        renderParameters(testCase) +
        '<div class="section-head"><h3>Expected error</h3></div><div class="card"><label class="check"><input id="expectErrorEnabled" type="checkbox"' +
        (testCase.expectError ? ' checked' : '') + '> This case should fail</label>' +
        (testCase.expectError ? '<div class="card-grid"><div class="field"><label>Kind</label><select id="errorKind"><option' +
          (testCase.expectError.kind === 'compile' ? ' selected' : '') + '>compile</option><option' +
          (testCase.expectError.kind === 'runtime' ? ' selected' : '') + '>runtime</option></select></div>' +
          '<div class="field" style="grid-column:span 3"><label>Message contains (optional)</label><input id="errorMessage" value="' +
          escapeHtml(testCase.expectError.messageContains ?? '') + '"></div></div>' : '') + '</div>' +
        '<div class="section-head"><h3>Final snapshot</h3><button class="secondary" data-add-pair="snapshot">Add value</button></div>' +
        '<p class="hint">Snapshot values produce a compact diff when the final state changes.</p>' +
        pairRows(testCase.snapshot?.values, 'snapshot') + '</main>';
      app.innerHTML = '<div class="layout">' + sidebar + main + '</div>';
      bind();
    }
    function renderSafely() {
      try {
        render();
      } catch (error) {
        app.innerHTML = '<div class="parse-error"><h2>Open the JSON source to repair this file</h2><p>' +
          escapeHtml(error instanceof Error ? error.message : String(error)) +
          '</p><button id="repairJson">Open JSON</button></div>';
        document.getElementById('repairJson').addEventListener('click', () =>
          vscode.postMessage({ type: 'openJson' }));
      }
    }
    function mapFor(group, parent) {
      const testCase = fixture.cases[selectedCase];
      if (group === 'initial') return testCase.initial ??= {};
      if (group === 'snapshot') return (testCase.snapshot ??= { values: {} }).values;
      if (group === 'timelineSet') return (testCase.timeline[Number(parent)].set ??= {});
      if (group === 'parameter') {
        const parameter = testCase.parameters[Number(parent)];
        return Object.fromEntries(Object.entries(parameter).filter(([key]) => key !== 'name'));
      }
    }
    function writeMap(group, parent, map) {
      const testCase = fixture.cases[selectedCase];
      if (group === 'initial') testCase.initial = map;
      else if (group === 'snapshot') (testCase.snapshot ??= { values: {} }).values = map;
      else if (group === 'timelineSet') testCase.timeline[Number(parent)].set = map;
      else if (group === 'parameter') {
        const parameter = testCase.parameters[Number(parent)];
        const name = parameter.name;
        testCase.parameters[Number(parent)] = { ...(name ? { name } : {}), ...map };
      }
    }
    function updatePair(input, keyChange) {
      const group = input.dataset.group; const parent = input.dataset.parent || '';
      const map = mapFor(group, parent); const entries = Object.entries(map);
      const index = Number(input.dataset.index); const [oldKey, oldValue] = entries[index] || ['', ''];
      if (keyChange) {
        const next = input.value.trim();
        if (!next || (next !== oldKey && Object.hasOwn(map, next))) {
          input.setCustomValidity('Use a non-empty unique name.'); input.reportValidity(); return;
        }
        entries[index] = [next, oldValue];
      } else {
        entries[index] = [oldKey, group === 'parameter' ? parseParameter(input.value) : parseScalar(input.value)];
      }
      writeMap(group, parent, Object.fromEntries(entries)); queueSave();
    }
    function bind() {
      document.querySelectorAll('[data-case]').forEach((button) => button.addEventListener('click', () => {
        selectedCase = Number(button.dataset.case); render();
      }));
      document.getElementById('addCase')?.addEventListener('click', () => {
        fixture.cases.push({ name: uniqueCaseName('new test'), maxTicks: 100, maxOperations: 100000, initial: {}, timeline: [], expect: [], parameters: [] });
        selectedCase = fixture.cases.length - 1; queueSave(); render();
      });
      document.getElementById('duplicateCase')?.addEventListener('click', () => {
        const clone = structuredClone(fixture.cases[selectedCase]);
        clone.name = uniqueCaseName((clone.name || 'test') + ' copy');
        fixture.cases.splice(selectedCase + 1, 0, clone); selectedCase++; queueSave(); render();
      });
      document.getElementById('deleteCase')?.addEventListener('click', () => {
        fixture.cases.splice(selectedCase, 1); selectedCase = Math.max(0, selectedCase - 1); queueSave(); render();
      });
      const scenario = document.getElementById('scenario');
      scenario?.addEventListener('change', () => { fixture.scenario = scenario.value; queueSave(); });
      document.getElementById('browseScenario')?.addEventListener('click', () => vscode.postMessage({ type: 'browseScenario' }));
      document.getElementById('openScenario')?.addEventListener('click', () => vscode.postMessage({ type: 'openScenario', scenario: fixture.scenario }));
      const seed = document.getElementById('seed');
      seed?.addEventListener('change', () => { fixture.seed = Number(seed.value); queueSave(); });
      const testCase = fixture.cases[selectedCase];
      if (!testCase) return;
      for (const [id, key, numeric] of [['caseName','name',false], ['focusIc','focusIc',false], ['maxTicks','maxTicks',true], ['maxOperations','maxOperations',true]]) {
        const input = document.getElementById(id);
        input?.addEventListener('input', () => {
          if (numeric) testCase[key] = Number(input.value);
          else if (input.value) testCase[key] = input.value;
          else if (key === 'name') testCase[key] = '';
          else delete testCase[key];
          queueSave();
        });
      }
      document.querySelectorAll('[data-pair-key]').forEach((input) => input.addEventListener('change', () => updatePair(input, true)));
      document.querySelectorAll('[data-pair-value]').forEach((input) => input.addEventListener('change', () => updatePair(input, false)));
      document.querySelectorAll('[data-pair-delete]').forEach((button) => button.addEventListener('click', () => {
        const map = mapFor(button.dataset.group, button.dataset.parent || '');
        const entries = Object.entries(map); entries.splice(Number(button.dataset.index), 1);
        writeMap(button.dataset.group, button.dataset.parent || '', Object.fromEntries(entries)); queueSave(); render();
      }));
      document.querySelectorAll('[data-add-pair]').forEach((button) => button.addEventListener('click', () => {
        const map = mapFor(button.dataset.addPair, button.dataset.parent || '');
        let key = button.dataset.addPair === 'parameter' ? 'value' : 'r0'; let suffix = 2;
        while (Object.hasOwn(map, key)) key = (button.dataset.addPair === 'parameter' ? 'value' : 'target') + suffix++;
        map[key] = 0; writeMap(button.dataset.addPair, button.dataset.parent || '', map); queueSave(); render();
      }));
      document.getElementById('addAssertion')?.addEventListener('click', () => {
        testCase.expect.push({ expression: 'r0', expected: 0 }); queueSave(); render();
      });
      document.querySelectorAll('[data-delete-assertion]').forEach((button) => button.addEventListener('click', () => {
        testCase.expect.splice(Number(button.dataset.deleteAssertion), 1); queueSave(); render();
      }));
      document.querySelectorAll('[data-assertion-type]').forEach((select) => select.addEventListener('change', () => {
        const assertion = testCase.expect[Number(select.dataset.assertionType)];
        const old = assertion[assertionType(assertion)] || '';
        delete assertion.expression; delete assertion.eventually; delete assertion.always;
        delete assertion.atTick; delete assertion.withinTicks;
        assertion[select.value] = old; queueSave(); render();
      }));
      const bindAssertion = (selector, dataKey, apply) => document.querySelectorAll(selector).forEach((input) =>
        input.addEventListener('input', () => { apply(testCase.expect[Number(input.dataset[dataKey])], input.value); queueSave(); }));
      bindAssertion('[data-assertion-expression]', 'assertionExpression', (assertion, value) => { assertion[assertionType(assertion)] = value; });
      bindAssertion('[data-assertion-expected]', 'assertionExpected', (assertion, value) => { if (value === '') delete assertion.expected; else assertion.expected = parseScalar(value); });
      bindAssertion('[data-assertion-at]', 'assertionAt', (assertion, value) => { if (value === '') delete assertion.atTick; else assertion.atTick = Number(value); });
      bindAssertion('[data-assertion-within]', 'assertionWithin', (assertion, value) => { if (value === '') delete assertion.withinTicks; else assertion.withinTicks = Number(value); });
      bindAssertion('[data-assertion-absolute]', 'assertionAbsolute', (assertion, value) => { assertion.tolerance ??= {}; if (value === '') delete assertion.tolerance.absolute; else assertion.tolerance.absolute = Number(value); });
      bindAssertion('[data-assertion-relative]', 'assertionRelative', (assertion, value) => { assertion.tolerance ??= {}; if (value === '') delete assertion.tolerance.relative; else assertion.tolerance.relative = Number(value); });
      document.getElementById('addTimeline')?.addEventListener('click', () => {
        testCase.timeline.push({ tick: 0, set: {}, events: [] }); queueSave(); render();
      });
      document.querySelectorAll('[data-delete-timeline]').forEach((button) => button.addEventListener('click', () => {
        testCase.timeline.splice(Number(button.dataset.deleteTimeline), 1); queueSave(); render();
      }));
      document.querySelectorAll('[data-timeline-tick]').forEach((input) => input.addEventListener('input', () => {
        testCase.timeline[Number(input.dataset.timelineTick)].tick = Number(input.value); queueSave();
      }));
      document.querySelectorAll('[data-add-event]').forEach((button) => button.addEventListener('click', () => {
        (testCase.timeline[Number(button.dataset.addEvent)].events ??= []).push({ target: 'r0', value: 0 }); queueSave(); render();
      }));
      document.querySelectorAll('[data-delete-event]').forEach((button) => button.addEventListener('click', () => {
        const [timeline, event] = button.dataset.deleteEvent.split(':').map(Number);
        testCase.timeline[timeline].events.splice(event, 1); queueSave(); render();
      }));
      for (const attribute of ['eventTarget', 'eventValue']) document.querySelectorAll('[data-' + attribute.replace(/[A-Z]/g, (letter) => '-' + letter.toLowerCase()) + ']').forEach((input) =>
        input.addEventListener('change', () => {
          const [timeline, event] = input.dataset[attribute].split(':').map(Number);
          if (attribute === 'eventTarget') testCase.timeline[timeline].events[event].target = input.value;
          else testCase.timeline[timeline].events[event].value = parseScalar(input.value);
          queueSave();
        }));
      document.getElementById('addParameter')?.addEventListener('click', () => {
        testCase.parameters.push({ name: 'set ' + (testCase.parameters.length + 1), value: 0 }); queueSave(); render();
      });
      document.querySelectorAll('[data-delete-parameter]').forEach((button) => button.addEventListener('click', () => {
        testCase.parameters.splice(Number(button.dataset.deleteParameter), 1); queueSave(); render();
      }));
      document.querySelectorAll('[data-parameter-name]').forEach((input) => input.addEventListener('input', () => {
        const parameter = testCase.parameters[Number(input.dataset.parameterName)];
        if (input.value) parameter.name = input.value; else delete parameter.name; queueSave();
      }));
      document.getElementById('expectErrorEnabled')?.addEventListener('change', (event) => {
        if (event.target.checked) testCase.expectError = { kind: 'runtime' };
        else delete testCase.expectError; queueSave(); render();
      });
      document.getElementById('errorKind')?.addEventListener('change', (event) => {
        testCase.expectError.kind = event.target.value; queueSave();
      });
      document.getElementById('errorMessage')?.addEventListener('input', (event) => {
        if (event.target.value) testCase.expectError.messageContains = event.target.value;
        else delete testCase.expectError.messageContains; queueSave();
      });
    }
    document.getElementById('saveNow').addEventListener('click', () => {
      clearTimeout(saveTimer);
      vscode.postMessage({ type: 'saveNow', fixture });
    });
    document.getElementById('openJson').addEventListener('click', () => vscode.postMessage({ type: 'openJson' }));
    window.addEventListener('message', (event) => {
      const message = event.data;
      if (message.type === 'update') {
        const same = fixture && JSON.stringify(fixture) === JSON.stringify(message.fixture);
        fixture = message.fixture; scenarios = message.scenarios || scenarios;
        showValidation(message.validation);
        if (!same) renderSafely();
      } else if (message.type === 'validation') {
        showValidation(message.validation);
      } else if (message.type === 'scenarios') {
        scenarios = message.scenarios; renderSafely();
      } else if (message.type === 'scenarioSelected') {
        fixture.scenario = message.scenario; queueSave(); renderSafely();
      } else if (message.type === 'parseError') {
        validationElement.hidden = true;
        app.innerHTML = '<div class="parse-error"><h2>Open the JSON source to repair this file</h2><p>' +
          escapeHtml(message.message) + '</p><button id="repairJson">Open JSON</button></div>';
        document.getElementById('repairJson').addEventListener('click', () => vscode.postMessage({ type: 'openJson' }));
      }
    });
    vscode.postMessage({ type: 'ready' });
  </script>
</body>
</html>`;
}

function createNonce(): string {
  const alphabet =
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  return Array.from(
    { length: 32 },
    () => alphabet[Math.floor(Math.random() * alphabet.length)],
  ).join("");
}
