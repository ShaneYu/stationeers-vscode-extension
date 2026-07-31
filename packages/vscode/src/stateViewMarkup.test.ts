import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const source = fs.readFileSync(
  path.resolve(process.cwd(), "src", "stateView.ts"),
  "utf8",
);
const manifest = JSON.parse(
  fs.readFileSync(path.resolve(process.cwd(), "package.json"), "utf8"),
) as {
  contributes: {
    configuration: {
      properties: Record<string, { default: unknown }>;
    };
  };
};
const readme = fs.readFileSync(
  path.resolve(process.cwd(), "README.md"),
  "utf8",
);

test("exposes reversible history through the IC10 State view", () => {
  for (const marker of [
    '"ic10/getTrace"',
    '"ic10/navigateHistory"',
    '"ic10/stateDiff"',
    "historyEvent",
    "historyTarget",
    "valueChart",
    "retainedTicks",
    "droppedEvents",
    "detailsRegisters",
    "detailsStack",
    "worldViewType",
    "mode === 'world'",
    "detailsLua",
    "detailsHistory",
    "refreshEpoch",
    "minmax(90px",
    "state.runtimes",
    "const mode = ${JSON.stringify(mode)};",
    "invocations",
    "onDidStartDebugSession",
    "onDidChangeActiveStackItem",
    "activeStackItem",
    "knownIc10Sessions",
    "rememberIc10Session(event.session, true)",
    "isIc10Session",
    "isUnavailableDebugSessionError",
    "setInterval(() => void this.refresh(), 1000)",
    "No IC10 simulation session is available.",
    "knownIc10Sessions",
  ]) {
    assert.match(source, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});

test("keeps World State independent from the selected runtime", () => {
  assert.match(source, /if \(this\.mode === "world"\)/);
  assert.match(source, /"ic10\/getTopologyState"/);
  assert.match(source, /tick: topology\.tick \?\? 0/);
});

test("injects the host view mode into the embedded script", () => {
  assert.match(source, /const mode = \$\{JSON\.stringify\(mode\)\};/);
  assert.match(
    source,
    /const mode = \$\{JSON\.stringify\(mode\)\};[\s\S]*if \(mode === 'world'\)/,
  );
});

test("renders successful IC and World State messages", () => {
  const state = {
    threadId: 1,
    tick: 4,
    cpu: { id: "requester", name: "Item Requester", state: "Running" },
    cpus: [{
      threadId: 1,
      id: "requester",
      name: "Item Requester",
      language: "ic10",
      state: "Running",
    }],
    registers: [{ name: "r0", value: "7" }],
    stack: [],
    runtime: { id: "requester", state: "Running", line: 17 },
  };
  const topology = {
    devices: {
      button: { fields: { Activate: "0" } },
      sensor: { fields: { Setting: "2" } },
    },
    networks: { data: { channels: { Channel0: "1" } } },
  };

  const ic = runStateViewScript("ic", { type: "state", state, topology });
  assert.match(ic.app.innerHTML, /<select id="cpu">/);
  assert.match(ic.app.innerHTML, /Item Requester/);

  const world = runStateViewScript("world", { type: "state", state, topology });
  assert.match(world.app.innerHTML, /button · Activate/);
  assert.doesNotMatch(world.app.innerHTML, /<select id="cpu">/);
});

test("updates World State controls in place without stealing focus", () => {
  const state = {
    threadId: 1,
    tick: 4,
    cpus: [],
    registers: [],
    stack: [],
  };
  const topology = {
    devices: {
      button: { fields: { Activate: "0" } },
      sensor: { fields: { Setting: "2" } },
    },
    networks: { data: { channels: { Channel0: "1" } } },
  };
  const view = runStateViewScript("world", { type: "state", state, topology });
  const initialRenders = view.app.renderCount;
  view.controls.deviceInput.value = "42";
  view.document.activeElement = view.controls.deviceInput;

  view.receive({
    data: {
      type: "state",
      state: { ...state, tick: 5 },
      topology: {
        devices: {
          button: { fields: { Activate: "1" } },
          sensor: { fields: { Setting: "9" } },
        },
        networks: { data: { channels: { Channel0: "3" } } },
      },
    },
  });

  assert.equal(view.app.renderCount, initialRenders, "stable topology is not rerendered");
  assert.equal(view.controls.deviceInput.value, "42", "focused input is preserved");
  assert.equal(view.controls.networkInput.value, "3", "unfocused input is updated");
  assert.equal(view.controls.button.textContent, "Release");
  view.controls.button.dispatch("click");
  assert.deepEqual(view.posted.at(-1), {
    type: "setWorldField",
    deviceId: "button",
    field: "Activate",
    value: "0",
  });

  view.document.activeElement = null;
  view.receive({
    data: {
      type: "state",
      state: { ...state, tick: 6 },
      topology: {
        devices: {
          button: { fields: { Activate: "1" } },
          sensor: { fields: { Setting: "9" } },
        },
        networks: { data: { channels: { Channel0: "3" } } },
      },
    },
  });
  assert.equal(view.controls.deviceInput.value, "9");
});

test("acknowledges each World State Activate toggle independently", () => {
  const state = { threadId: 1, tick: 4, cpus: [], registers: [], stack: [] };
  const topology = {
    devices: {
      "iron-button": { fields: { Activate: "0" } },
      "gold-button": { fields: { Activate: "0" } },
      "steel-button": { fields: { Activate: "0" } },
    },
    networks: {},
  };
  const view = runStateViewScript("world", { type: "state", state, topology });

  view.controls.steelButton.dispatch("click");
  assert.equal(view.controls.steelButton.disabled, true);
  assert.equal(view.controls.steelButton.textContent, "Pressing…");
  assert.deepEqual(view.posted.at(-1), {
    type: "setWorldField",
    deviceId: "steel-button",
    field: "Activate",
    value: "1",
  });

  view.receive({
    data: {
      type: "worldFieldResult",
      deviceId: "steel-button",
      field: "Activate",
      value: "1",
      success: true,
    },
  });
  assert.equal(view.controls.steelButton.disabled, false);
  assert.equal(view.controls.steelButton.textContent, "Release");
  assert.equal(view.controls.goldButton.textContent, "Press");

  view.controls.goldButton.dispatch("click");
  assert.deepEqual(view.posted.at(-1), {
    type: "setWorldField",
    deviceId: "gold-button",
    field: "Activate",
    value: "1",
  });
});

test("keeps IC State selectors and disclosure sections stable while polling", () => {
  const state = {
    threadId: 1,
    tick: 4,
    cpu: { id: "requester", name: "Item Requester", state: "Running", line: 17 },
    cpus: [
      { threadId: 1, id: "requester", name: "Item Requester", language: "ic10", state: "Running" },
      { threadId: 2, id: "supplier", name: "Item Supplier", language: "ic10", state: "Paused" },
    ],
    registers: [{ name: "r0", value: "7" }],
    stack: ["3"],
    runtime: { id: "requester", state: "Running", line: 17 },
  };
  const view = runStateViewScript("ic", { type: "state", state });
  const initialRenders = view.app.renderCount;
  view.controls.detailsRegisters.open = true;
  view.controls.detailsStack.open = true;
  view.controls.detailsHistory.open = true;
  view.controls.cpu.value = "1";
  view.document.activeElement = view.controls.cpu;

  view.receive({
    data: {
      type: "state",
      state: {
        ...state,
        threadId: 2,
        tick: 5,
        cpu: { id: "supplier", name: "Item Supplier", state: "Paused", line: 24 },
        runtime: { id: "supplier", state: "Paused", line: 24 },
        registers: [{ name: "r0", value: "9" }],
        stack: ["4"],
      },
    },
  });

  assert.equal(view.app.renderCount, initialRenders, "stable IC structure is not rerendered");
  assert.equal(view.controls.cpu.value, "1", "focused runtime selector is preserved");
  assert.equal(view.controls.detailsRegisters.open, true);
  assert.equal(view.controls.detailsStack.open, true);
  assert.equal(view.controls.detailsHistory.open, true);
  assert.match(view.controls.summary.innerHTML, /Tick 5 · line 24 · Status Paused/);
  assert.doesNotMatch(view.app.innerHTML, /— Running<\/option>/);

  view.document.activeElement = null;
  view.receive({
    data: {
      type: "state",
      state: {
        ...state,
        threadId: 2,
        tick: 6,
        cpu: { id: "supplier", name: "Item Supplier", state: "Running", line: 25 },
        runtime: { id: "supplier", state: "Running", line: 25 },
      },
    },
  });
  assert.equal(view.controls.cpu.value, "2");
  view.app.dispatch("change", view.controls.cpu);
  assert.deepEqual(view.posted.at(-1), { type: "selectThread", threadId: 2 });
});

test("filters the timeline with the current target selector", () => {
  assert.match(source, /const target = targetFilter\.value;/);
  assert.match(
    source,
    /record\.writes\.some\(\(write\) => write\.target === target\)/,
  );
  assert.match(source, /type: 'selectTraceFilter'/);
  assert.match(source, /this\.traceFilter = message\.target \|\| undefined/);
  assert.doesNotMatch(source, /const requestedTarget = message\.traceFilter/);
});

test("renders an accessible SVG chart for retained numeric writes", () => {
  for (const marker of [
    "<svg viewBox=",
    'role="img"',
    "aria-label=",
    "Number.isFinite(point.value)",
    "prefers-reduced-motion: reduce",
    "forced-colors: active",
    "chart-line",
    "chart-point",
  ]) {
    assert.match(source, new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});

test("keeps the embedded State view script syntactically valid", () => {
  const marker = '<script nonce="${nonce}">';
  const start = source.indexOf(marker);
  const end = source.indexOf("</script>", start);
  assert(start >= 0 && end > start);
  const raw = source.slice(start + marker.length, end);
  const cooked = Function(`return \`${raw.replace("${JSON.stringify(mode)}", '"ic"')}\`;`)() as string;
  assert.doesNotThrow(() => new Function(cooked));
});

function runStateViewScript(
  mode: "ic" | "world",
  message: unknown,
): StateViewHarness {
  const marker = '<script nonce="${nonce}">';
  const start = source.indexOf(marker);
  const end = source.indexOf("</script>", start);
  assert(start >= 0 && end > start);
  const raw = source.slice(start + marker.length, end);
  const cooked = Function(
    `return \`${raw.replace("${JSON.stringify(mode)}", JSON.stringify(mode))}\`;`,
  )() as string;
  const controls = {
    button: fakeControl({
      worldDevice: "button",
      worldField: "Activate",
      worldValue: "1",
    }),
    ironButton: fakeControl({
      worldDevice: "iron-button",
      worldField: "Activate",
      worldValue: "1",
    }),
    goldButton: fakeControl({
      worldDevice: "gold-button",
      worldField: "Activate",
      worldValue: "1",
    }),
    steelButton: fakeControl({
      worldDevice: "steel-button",
      worldField: "Activate",
      worldValue: "1",
    }),
    deviceInput: fakeControl({
      worldDevice: "sensor",
      worldField: "Setting",
    }, "2"),
    networkInput: fakeControl({
      worldNetwork: "data",
      worldChannel: "Channel0",
    }, "1"),
    cpu: fakeControl({}, "1", "cpu"),
    registerInput: fakeControl({ register: "r0" }, "7"),
    stackInput: fakeControl({ stack: "0" }, "3"),
    summary: fakeControl({}, "", "icSummary"),
    detailsRegisters: fakeControl({}, "", "detailsRegisters"),
    detailsStack: fakeControl({}, "", "detailsStack"),
    detailsHistory: fakeControl({}, "", "detailsHistory"),
    historyContent: fakeControl({}, "", "historyContent"),
  };
  const appListeners = new Map<string, (event: { target: FakeControl }) => void>();
  const app = {
    className: "muted",
    textContent: "No IC10 simulation session is available.",
    renderedHtml: "No IC10 simulation session is available.",
    renderCount: 0,
    get innerHTML(): string { return this.renderedHtml; },
    set innerHTML(value: string) {
      this.renderedHtml = value;
      this.renderCount++;
    },
    querySelectorAll: (selector: string) => {
      if (app.renderedHtml.includes('id="worldState"')) {
        if (selector === 'button[data-world-device]') return [
          controls.button,
          controls.ironButton,
          controls.goldButton,
          controls.steelButton,
        ];
        if (selector === 'input[data-world-device]') return [controls.deviceInput];
        if (selector === 'input[data-world-network]') return [controls.networkInput];
      }
      if (app.renderedHtml.includes('id="icState"')) {
        if (selector === 'input[data-register]') return [controls.registerInput];
        if (selector === 'input[data-stack]') return [controls.stackInput];
      }
      return [];
    },
    addEventListener: (
      type: string,
      listener: (event: { target: FakeControl }) => void,
    ) => appListeners.set(type, listener),
    dispatch: (type: string, target: FakeControl) =>
      appListeners.get(type)?.({ target }),
  };
  let receive: ((event: { data: unknown }) => void) | undefined;
  const document = {
    activeElement: null as FakeControl | null,
    getElementById: (id: string) => {
      if (id === "app") return app;
      if (id === "worldState" && app.renderedHtml.includes('id="worldState"')) {
        return {};
      }
      if (id === "worldTick" && app.renderedHtml.includes('id="worldTick"')) {
        return { textContent: "" };
      }
      if (app.renderedHtml.includes('id="icState"')) {
        if (id === "icState") return {};
        if (id === "cpu") return controls.cpu;
        if (id === "icSummary") return controls.summary;
        if (id === "detailsRegisters") return controls.detailsRegisters;
        if (id === "detailsStack") return controls.detailsStack;
        if (id === "detailsHistory") return controls.detailsHistory;
        if (id === "historyContent") return controls.historyContent;
      }
      return null;
    },
  };
  const posted: unknown[] = [];
  const window = {
    addEventListener: (
      type: string,
      listener: (event: { data: unknown }) => void,
    ) => {
      if (type === "message") receive = listener;
    },
  };
  const execute = new Function(
    "acquireVsCodeApi",
    "document",
    "window",
    "prompt",
    cooked,
  );
  execute(() => ({ postMessage: (value: unknown) => posted.push(value) }), document, window, () => null);
  assert(receive, "State view registers its message listener");
  receive({ data: message });
  return { app, controls, document, posted, receive };
}

interface FakeControl {
  id: string;
  dataset: Record<string, string>;
  value: string;
  textContent: string;
  innerHTML: string;
  open: boolean;
  disabled: boolean;
  closest(selector: string): FakeControl | null;
  addEventListener(type: string, listener: () => void): void;
  dispatch(type: string): void;
}

interface StateViewHarness {
  app: {
    className: string;
    textContent: string;
    innerHTML: string;
    renderCount: number;
    dispatch(type: string, target: FakeControl): void;
  };
  controls: {
    button: FakeControl;
    ironButton: FakeControl;
    goldButton: FakeControl;
    steelButton: FakeControl;
    deviceInput: FakeControl;
    networkInput: FakeControl;
    cpu: FakeControl;
    registerInput: FakeControl;
    stackInput: FakeControl;
    summary: FakeControl;
    detailsRegisters: FakeControl;
    detailsStack: FakeControl;
    detailsHistory: FakeControl;
    historyContent: FakeControl;
  };
  document: { activeElement: FakeControl | null };
  posted: unknown[];
  receive(event: { data: unknown }): void;
}

function fakeControl(
  dataset: Record<string, string>,
  value = "",
  id = "",
): FakeControl {
  const listeners = new Map<string, () => void>();
  return {
    id,
    dataset,
    value,
    textContent: "Press",
    innerHTML: "",
    open: false,
    disabled: false,
    closest: () => null,
    addEventListener: (type, listener) => listeners.set(type, listener),
    dispatch: (type) => listeners.get(type)?.(),
  };
}

test("documents the contributed history defaults", () => {
  const properties = manifest.contributes.configuration.properties;
  assert.equal(properties["ic10.debug.history.enabled"]?.default, false);
  assert.equal(properties["ic10.debug.history.events"]?.default, 20_000);
  assert.equal(
    properties["ic10.debug.history.checkpointInterval"]?.default,
    10_000,
  );
  assert.equal(properties["ic10.debug.history.memoryMiB"]?.default, 64);
  for (const row of [
    "| `ic10.debug.history.enabled` | `false` |",
    "| `ic10.debug.history.events` | `20000` |",
    "| `ic10.debug.history.checkpointInterval` | `10000` |",
    "| `ic10.debug.history.memoryMiB` | `64` |",
  ]) {
    assert(readme.includes(row), `README is missing ${row}`);
  }
});
