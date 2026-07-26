import * as path from "node:path";

import * as vscode from "vscode";

export interface TopologyRuntimeWrite {
  sequence: number;
  tick: number;
  sourceId: string;
  sourcePath?: string;
  line?: number;
  cpuId?: string;
  targetId: string;
  targetKind: "device" | "network" | "register" | "stack" | "other";
  field?: string;
  before?: string;
  after?: string;
}

export interface TopologyRuntimeRead {
  sequence: number;
  tick: number;
  sourceId: string;
  sourcePath?: string;
  line?: number;
  cpuId?: string;
  targetId: string;
  targetKind: "device" | "network" | "register" | "stack" | "other";
  field?: string;
  value?: string;
}

export interface TopologyRuntimeState {
  scenarioId: string;
  tick: number;
  devices: Record<
    string,
    {
      behaviour: {
        model: string;
        version: number;
        kind: string;
        modelled: boolean;
      };
      fields: Record<string, string>;
      lastReader?: string;
      lastWriter?: string;
    }
  >;
  networks: Record<
    string,
    {
      channels: Record<string, string>;
      lastReader?: string;
      lastWriter?: string;
    }
  >;
  ics: Record<
    string,
    {
      runState: string;
      sourceId: string;
      sourcePath?: string;
      line?: number;
    }
  >;
}

export type TopologyRuntimeMessage =
  | { type: "snapshot"; state: TopologyRuntimeState }
  | {
      type: "traceBatch";
      scenarioId: string;
      sequence: number;
      dropped: number;
      reads: readonly TopologyRuntimeRead[];
      writes: readonly TopologyRuntimeWrite[];
      ics?: TopologyRuntimeState["ics"];
    }
  | { type: "ended"; scenarioId: string };

export class EnvironmentDebugOverlayService implements vscode.Disposable {
  private readonly subscriptions = new Map<
    string,
    Set<(message: TopologyRuntimeMessage) => void>
  >();
  private readonly snapshots = new Map<string, TopologyRuntimeState>();
  private readonly attaching = new Set<string>();
  private readonly disposables: vscode.Disposable[];

  public constructor() {
    this.disposables = [
      vscode.debug.onDidStartDebugSession((session) => {
        void this.attach(session);
      }),
      vscode.debug.onDidReceiveDebugSessionCustomEvent((event) => {
        if (event.session.type !== "ic10") {
          return;
        }
        if (event.event === "ic10/traceBatch") {
          this.publish(event.body as TopologyRuntimeMessage);
        } else if (event.event === "ic10/stateChanged") {
          void this.attach(event.session, true);
        }
      }),
      vscode.debug.onDidTerminateDebugSession((session) => {
        if (session.type !== "ic10") {
          return;
        }
        const scenarioId = scenarioIdentifier(session.configuration.scenario);
        if (scenarioId) {
          this.snapshots.delete(scenarioId);
          this.publish({ type: "ended", scenarioId });
        }
      }),
    ];
  }

  public subscribe(
    scenario: vscode.Uri,
    listener: (message: TopologyRuntimeMessage) => void,
  ): vscode.Disposable {
    const key = scenarioIdentifier(scenario.toString(true))!;
    const values = this.subscriptions.get(key) ?? new Set();
    values.add(listener);
    this.subscriptions.set(key, values);
    const cached = this.snapshots.get(key);
    if (cached) {
      queueMicrotask(() => listener({ type: "snapshot", state: cached }));
    } else {
      const candidate = vscode.debug.activeDebugSession;
      const session =
        candidate?.type === "ic10" &&
        scenarioIdentifier(candidate.configuration.scenario) === key
          ? candidate
          : undefined;
      if (session) {
        void this.attach(session);
      }
    }
    return new vscode.Disposable(() => {
      values.delete(listener);
      if (values.size === 0) {
        this.subscriptions.delete(key);
      }
    });
  }

  public snapshot(scenario: vscode.Uri): TopologyRuntimeState | undefined {
    return this.snapshots.get(scenarioIdentifier(scenario.toString(true))!);
  }

  public async action(
    scenario: vscode.Uri,
    action: "source" | "variables" | "watch" | "trace",
    targetId: string,
  ): Promise<void> {
    const state = this.snapshot(scenario);
    if (action === "variables") {
      await vscode.commands.executeCommand(
        "workbench.debug.action.focusVariablesView",
      );
      return;
    }
    if (action === "watch") {
      await vscode.commands.executeCommand(
        "workbench.debug.action.addToWatch",
        `device(${JSON.stringify(targetId)})`,
      );
      return;
    }
    if (action === "trace") {
      await vscode.commands.executeCommand("ic10.filterTrace", {
        scenarioId: scenarioIdentifier(scenario.toString(true)),
        targetId,
      });
      return;
    }
    const source = state?.ics[targetId];
    if (!source?.sourcePath) {
      void vscode.window.showInformationMessage(
        `No paused source location is available for ${targetId}.`,
      );
      return;
    }
    const document = await vscode.workspace.openTextDocument(
      vscode.Uri.file(source.sourcePath),
    );
    const editor = await vscode.window.showTextDocument(document);
    const line = Math.max(0, (source.line ?? 1) - 1);
    editor.selection = new vscode.Selection(line, 0, line, 0);
    editor.revealRange(editor.selection, vscode.TextEditorRevealType.InCenter);
  }

  public dispose(): void {
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.subscriptions.clear();
    this.snapshots.clear();
  }

  private async attach(
    session: vscode.DebugSession,
    force = false,
  ): Promise<void> {
    if (session.type !== "ic10") {
      return;
    }
    const scenarioId = scenarioIdentifier(session.configuration.scenario);
    if (!scenarioId || (!force && this.snapshots.has(scenarioId))) {
      return;
    }
    if (this.attaching.has(scenarioId)) {
      return;
    }
    this.attaching.add(scenarioId);
    try {
      const state = (await session.customRequest(
        "ic10/getTopologyState",
        {},
      )) as TopologyRuntimeState;
      this.snapshots.set(scenarioId, state);
      this.publish({ type: "snapshot", state });
    } catch {
      // Older adapters simply do not provide topology overlays.
    } finally {
      this.attaching.delete(scenarioId);
    }
  }

  private publish(message: TopologyRuntimeMessage): void {
    const rawScenarioId =
      message.type === "snapshot" ? message.state.scenarioId : message.scenarioId;
    const scenarioId = scenarioIdentifier(rawScenarioId) ?? rawScenarioId;
    if (message.type === "snapshot") {
      message.state.scenarioId = scenarioId;
      this.snapshots.set(scenarioId, message.state);
    } else if (message.type === "traceBatch") {
      const current = this.snapshots.get(scenarioId);
      if (current) {
        this.snapshots.set(
          scenarioId,
          applyTopologyRuntimeMessage(current, {
            ...message,
            scenarioId,
          }),
        );
      }
    }
    for (const listener of this.subscriptions.get(scenarioId) ?? []) {
      listener(message);
    }
  }
}

export function applyTopologyRuntimeMessage(
  current: TopologyRuntimeState,
  batch: Extract<TopologyRuntimeMessage, { type: "traceBatch" }>,
): TopologyRuntimeState {
  const devices = Object.fromEntries(
    Object.entries(current.devices).map(([id, device]) => [
      id,
      {
        ...device,
        behaviour: { ...device.behaviour },
        fields: { ...device.fields },
      },
    ]),
  );
  const networks = Object.fromEntries(
    Object.entries(current.networks).map(([id, network]) => [
      id,
      {
        ...network,
        channels: { ...network.channels },
      },
    ]),
  );
  let tick = current.tick;
  for (const write of batch.writes) {
    tick = Math.max(tick, write.tick);
    const target =
      write.targetKind === "network"
        ? networks[write.targetId]
        : write.targetKind === "device"
          ? devices[write.targetId]
          : undefined;
    if (!target) {
      continue;
    }
    target.lastWriter = write.cpuId ?? write.sourceId;
    if (write.field && write.after !== undefined) {
      if ("channels" in target) {
        target.channels[write.field] = write.after;
      } else {
        target.fields[write.field] = write.after;
      }
    }
  }
  for (const read of batch.reads) {
    tick = Math.max(tick, read.tick);
    const target =
      read.targetKind === "network"
        ? networks[read.targetId]
        : read.targetKind === "device"
          ? devices[read.targetId]
          : undefined;
    if (target) {
      target.lastReader = read.cpuId ?? read.sourceId;
    }
  }
  return {
    scenarioId: batch.scenarioId,
    tick,
    devices,
    networks,
    ics: batch.ics ? { ...batch.ics } : { ...current.ics },
  };
}

export function scenarioIdentifier(value: unknown): string | undefined {
  if (typeof value !== "string" || value.trim() === "") {
    return undefined;
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(value)) {
    return vscode.Uri.parse(value, true).toString(true);
  }
  return vscode.Uri.file(path.resolve(value)).toString(true);
}
