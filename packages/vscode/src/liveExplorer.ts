import * as vscode from "vscode";
import { BridgeChip, BridgeClient, BridgeError, BridgeScope, BridgeSnapshot, BridgeSource, BridgeState } from "./bridge";
import { formatChipDescription, getLiveChipContext } from "./liveExplorerModel";
import { liveSourceIdentity, liveSourceKey, liveSourceLabel, LiveSourceSession } from "./liveSourceModel";

export const LIVE_CONTEXT = { connected: "stationeers.bridge.connected", filterActive: "stationeers.bridge.filterActive", read: "stationeers.bridge.canReadIc10", write: "stationeers.bridge.canWriteIc10", language: "stationeers.liveChip.language", stale: "stationeers.liveChip.stale", available: "stationeers.liveChip.available", canRead: "stationeers.liveChip.canRead", canCompare: "stationeers.liveChip.canCompare", canPush: "stationeers.liveChip.canPush", luaDebugEligible: "stationeers.liveChip.luaDebugEligible", lua: "stationeers.stationeersLua.connected" } as const;

const LIVE_SCHEME = "stationeers-live";

class LiveSourceFileSystem implements vscode.FileSystemProvider {
  private readonly contents = new Map<string, Uint8Array>();
  private readonly changed = new vscode.EventEmitter<vscode.FileChangeEvent[]>();
  readonly onDidChangeFile = this.changed.event;
  constructor(private readonly onWrite: (uri: vscode.Uri, bytes: Uint8Array) => Promise<void>) {}
  create(uri: vscode.Uri, source: string): void { this.contents.set(uri.toString(), new TextEncoder().encode(source)); }
  restore(uri: vscode.Uri, source: string): void { this.contents.set(uri.toString(), new TextEncoder().encode(source)); this.changed.fire([{ type: vscode.FileChangeType.Created, uri }]); }
  replace(uri: vscode.Uri, source: string): void { this.create(uri, source); this.changed.fire([{ type: vscode.FileChangeType.Changed, uri }]); }
  markDeleted(uri: vscode.Uri): void { if (this.contents.delete(uri.toString())) this.changed.fire([{ type: vscode.FileChangeType.Deleted, uri }]); }
  watch(): vscode.Disposable { return new vscode.Disposable(() => undefined); }
  stat(uri: vscode.Uri): vscode.FileStat { const bytes = this.contents.get(uri.toString()); if (!bytes) throw vscode.FileSystemError.FileNotFound(uri); return { type: vscode.FileType.File, ctime: 0, mtime: Date.now(), size: bytes.byteLength }; }
  readDirectory(): [string, vscode.FileType][] { return []; }
  createDirectory(): void { throw vscode.FileSystemError.NoPermissions; }
  readFile(uri: vscode.Uri): Uint8Array { const bytes = this.contents.get(uri.toString()); if (!bytes) throw vscode.FileSystemError.FileNotFound(uri); return bytes; }
  async writeFile(uri: vscode.Uri, content: Uint8Array): Promise<void> { await this.onWrite(uri, content); this.contents.set(uri.toString(), content); }
  delete(): void { throw vscode.FileSystemError.NoPermissions; }
  rename(): void { throw vscode.FileSystemError.NoPermissions; }
  dispose(): void { this.changed.dispose(); this.contents.clear(); }
}

class ReadOnlyCompareFileSystem implements vscode.FileSystemProvider {
  private readonly contents = new Map<string, Uint8Array>();
  private readonly changed = new vscode.EventEmitter<vscode.FileChangeEvent[]>();
  readonly onDidChangeFile = this.changed.event;
  create(uri: vscode.Uri, source: string): void { this.contents.set(uri.toString(), new TextEncoder().encode(source)); }
  watch(): vscode.Disposable { return new vscode.Disposable(() => undefined); }
  stat(uri: vscode.Uri): vscode.FileStat { const bytes = this.contents.get(uri.toString()); if (!bytes) throw vscode.FileSystemError.FileNotFound(uri); return { type: vscode.FileType.File, ctime: 0, mtime: Date.now(), size: bytes.byteLength }; }
  readDirectory(): [string, vscode.FileType][] { return []; }
  createDirectory(): void { throw vscode.FileSystemError.NoPermissions; }
  readFile(uri: vscode.Uri): Uint8Array { const bytes = this.contents.get(uri.toString()); if (!bytes) throw vscode.FileSystemError.FileNotFound(uri); return bytes; }
  writeFile(): void { throw vscode.FileSystemError.NoPermissions("Comparison documents are read-only."); }
  delete(): void { throw vscode.FileSystemError.NoPermissions; }
  rename(): void { throw vscode.FileSystemError.NoPermissions; }
  dispose(): void { this.changed.dispose(); this.contents.clear(); }
}

type Node = StatusNode | ScopeNode | ChipNode | WarningNode;
class StatusNode extends vscode.TreeItem { readonly nodeKind = "status"; constructor(label: string, contextValue = "bridgeStatus", collapsible = vscode.TreeItemCollapsibleState.None) { super(label, collapsible); this.contextValue = contextValue; this.iconPath = new vscode.ThemeIcon("plug"); } }
class ScopeNode extends vscode.TreeItem { readonly nodeKind = "scope"; constructor(readonly scope: BridgeScope) { super(scope.disambiguator ? `${scope.name} · ${scope.disambiguator}` : scope.name, vscode.TreeItemCollapsibleState.Collapsed); this.id = `scope:${scope.scopeId}`; this.contextValue = "liveScope"; this.iconPath = new vscode.ThemeIcon("broadcast"); } }
export class ChipNode extends vscode.TreeItem { readonly nodeKind = "chip"; constructor(readonly chip: BridgeChip, readonly scopeId: string, readonly networkName: string, readonly stale: boolean) { super(chip.housingName, vscode.TreeItemCollapsibleState.None); this.id = `chip:${scopeId}:${chip.chipId}`; this.description = formatChipDescription(chip); this.tooltip = `${chip.housingName}\n${chip.housingPrefab}\nReference ${chip.housingReferenceId}`; this.contextValue = chip.language === "ic10" ? "liveIc10Chip" : "liveLuaChip"; this.iconPath = new vscode.ThemeIcon(chip.powered ? "circuit-board" : "circle-slash"); if (chip.language === "ic10") this.command = { command: "stationeers.live.open", title: "Open live IC10 source", arguments: [this] }; }
}
class WarningNode extends vscode.TreeItem { readonly nodeKind = "warning"; constructor(readonly warning: { message: string; anchorReferenceId?: string }) { super(warning.message, vscode.TreeItemCollapsibleState.None); this.description = warning.anchorReferenceId ? `anchor ${warning.anchorReferenceId}` : undefined; this.contextValue = "bridgeWarning"; this.iconPath = new vscode.ThemeIcon("warning"); } }

export class LiveNetworkTree implements vscode.TreeDataProvider<Node>, vscode.Disposable {
  private readonly changed = new vscode.EventEmitter<Node | undefined | null | void>(); private snapshot?: BridgeSnapshot; private state: BridgeState = "disabled"; private filterText = "";
  readonly onDidChangeTreeData = this.changed.event;
  constructor(readonly client: BridgeClient) { client.onDidChangeState((state) => { const wasConnected = this.state === "connected"; this.state = state; this.snapshot = client.snapshot; if (wasConnected !== (state === "connected") || state === "connected") this.changed.fire(); }); }
  setSnapshot(snapshot: BridgeSnapshot): void { this.snapshot = snapshot; this.changed.fire(); }
  setFilter(value: string): void { this.filterText = value.trim().toLocaleLowerCase(); this.changed.fire(); }
  get filter(): string { return this.filterText; }
  getTreeItem(element: Node): vscode.TreeItem { return element; }
  getChildren(element?: Node): Node[] {
    if (!element) { if (this.state !== "connected") return [new StatusNode("Not connected to game", "bridgeDisconnected")]; const worldName = this.client.hello?.world.name?.trim() || "world"; const scopes = (this.snapshot?.scopes ?? []).filter((scope) => this.matchesScope(scope)); return [new StatusNode(`Connected · ${worldName}${this.filterText ? ` · filter: ${this.filterText}` : ""}`), ...scopes.slice().sort(scopeSort).map((scope) => new ScopeNode(scope)), ...((this.snapshot?.warnings.length ?? 0) > 0 ? [new StatusNode("Unlabeled Remote Networks (not shown)", "bridgeWarnings", vscode.TreeItemCollapsibleState.Collapsed)] : [])]; }
    if (element instanceof ScopeNode) { const chips = this.snapshot?.chips ?? []; return element.scope.chipIds.map((id) => chips.find((chip) => chip.chipId === id)).filter((chip): chip is BridgeChip => Boolean(chip) && this.matchesChip(chip)).sort(chipSort).map((chip) => new ChipNode(chip, element.scope.scopeId, element.scope.name, this.state !== "connected")); }
    if (element.contextValue === "bridgeWarnings") return (this.snapshot?.warnings ?? []).map((warning) => new WarningNode(warning));
    return [];
  }
  dispose(): void { this.changed.dispose(); }
  private matchesScope(scope: BridgeScope): boolean { if (!this.filterText) return true; if (`${scope.name} ${scope.disambiguator ?? ""}`.toLocaleLowerCase().includes(this.filterText)) return true; return scope.chipIds.some((id) => this.matchesChip(this.snapshot?.chips.find((chip) => chip.chipId === id))); }
  private matchesChip(chip: BridgeChip | undefined): boolean { if (!chip) return false; if (!this.filterText) return true; return `${chip.housingName} ${chip.housingPrefab} ${chip.chipPrefab} ${chip.language} ${chip.housingReferenceId}`.toLocaleLowerCase().includes(this.filterText); }
}

export class LiveExplorer implements vscode.Disposable {
  private readonly tree: LiveNetworkTree; private readonly view: vscode.TreeView<Node>; private readonly status: vscode.StatusBarItem; private readonly disposables: vscode.Disposable[] = [];
  private selectedChip?: ChipNode; private lastState: BridgeState = "disabled"; private outageNotified = false; private autoConnectInFlight = false; private refreshInFlight = false;
  private readonly liveSources = new Map<string, LiveSourceSession>();
  private readonly files: LiveSourceFileSystem;
  private readonly compareFiles = new ReadOnlyCompareFileSystem();
  private readonly manualSaves = new Set<string>();
  constructor(private readonly context: vscode.ExtensionContext, private readonly client: BridgeClient) {
    this.files = new LiveSourceFileSystem((uri, bytes) => this.saveVirtual(uri, new TextDecoder().decode(bytes)));
    this.tree = new LiveNetworkTree(client); this.view = vscode.window.createTreeView("stationeers.liveNetworks", { treeDataProvider: this.tree, showCollapseAll: true, canSelectMany: false, dragAndDropController: new LiveDropController(client, this.tree) });
    this.status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 80); this.status.command = "stationeers.live.connect"; this.status.show();
    this.disposables.push(this.tree, this.view, this.status, this.files, this.compareFiles, vscode.workspace.registerFileSystemProvider(LIVE_SCHEME, this.files, { isCaseSensitive: true }), vscode.workspace.registerFileSystemProvider("stationeers-compare", this.compareFiles, { isCaseSensitive: true }), vscode.workspace.onWillSaveTextDocument((event) => {
      if (event.document.uri.scheme !== LIVE_SCHEME || event.reason !== vscode.TextDocumentSaveReason.Manual) return;
      const session = [...this.liveSources.values()].find((value) => value.uri === event.document.uri.toString());
      const chip = session ? this.chipForSession(session) : undefined;
      if (!session || !chip) return;
      this.manualSaves.add(event.document.uri.toString());
      event.waitUntil(this.pushDocument(event.document, session, chip).then(() => []).catch((error) => { this.showBridgeError("save", error); throw error; }));
    }), client.onDidChangeState((state) => { if (this.lastState === "connected" && state !== "connected") { for (const session of this.liveSources.values()) this.files.markDeleted(vscode.Uri.parse(session.uri)); if (!this.outageNotified) { this.outageNotified = true; void vscode.window.showWarningMessage("Stationeers bridge connection lost. Live chip tabs remain open and are marked deleted until the game reconnects."); } } if (state === "connected") { this.outageNotified = false; } this.lastState = state; void this.updateContext(state); }), this.view.onDidChangeSelection((event) => { this.selectedChip = event.selection.find((item): item is ChipNode => item instanceof ChipNode); void this.updateSelectedContext(); }), vscode.workspace.onDidChangeConfiguration((event) => { if (event.affectsConfiguration("stationeers.bridge.url")) void this.reconnect(); }), { dispose: () => clearInterval(this.pollTimer) });
    this.pollTimer = setInterval(() => void this.poll(), 3000);
    void this.updateContext(client.state);
  }
  private readonly pollTimer: ReturnType<typeof setInterval>;
  private async saveVirtual(uri: vscode.Uri, source: string): Promise<void> {
    this.files.create(uri, source);
    this.manualSaves.delete(uri.toString());
  }
  private liveUri(source: BridgeSource, node: ChipNode): vscode.Uri {
    const safe = `${node.networkName} — ${node.chip.housingName}`.replace(/[\\/:*?"<>|]+/g, "_").trim() || node.chip.chipId;
    return vscode.Uri.from({ scheme: LIVE_SCHEME, path: `/${safe}.ic10`, query: `chipId=${encodeURIComponent(source.chipId)}&worldEpoch=${encodeURIComponent(source.worldEpoch)}` });
  }
  private sessionFor(node?: ChipNode): LiveSourceSession | undefined {
    if (node) return [...this.liveSources.values()].find((session) => session.identity.chipId === node.chip.chipId && session.networkName === node.networkName);
    const uri = vscode.window.activeTextEditor?.document.uri.toString();
    return uri ? [...this.liveSources.values()].find((session) => session.uri === uri) : undefined;
  }
  private chipForSession(session: LiveSourceSession): BridgeChip | undefined {
    return this.client.snapshot?.chips.find((chip) => chip.chipId === session.identity.chipId && chip.housingReferenceId === session.identity.housingReferenceId);
  }
  private async pushDocument(document: vscode.TextDocument, session: LiveSourceSession, chip: BridgeChip): Promise<void> {
    const source = document.getText();
    const sourceSha256 = await sha256(source);
    const response = await this.client.push(chip, { worldEpoch: session.identity.worldEpoch, version: session.version, sha256: session.sha256 }, source, sourceSha256);
    this.liveSources.set(liveSourceKey(session.identity), { ...session, version: response.version, length: new TextEncoder().encode(source).byteLength, sha256: response.sha256, source });
    this.files.create(document.uri, source);
  }
  private showBridgeError(action: string, error: unknown): void {
    if (error instanceof BridgeError && error.code === "source_conflict") {
      void vscode.window.showErrorMessage(`Could not ${action} source: the chip changed in-game. Use Merge or Compare before trying again.`);
      return;
    }
    void vscode.window.showErrorMessage(`Could not ${action} source: ${error instanceof Error ? error.message : String(error)}`);
  }
  async connect(): Promise<void> { if (vscode.env.remoteName) { void vscode.window.showWarningMessage("Live Stationeers discovery is local-only. Use a local VS Code window or explicitly forward the bridge port; this extension will not contact a remote machine."); return; } try { let token = await this.context.secrets.get("stationeers.bridge.token"); if (!token) { token = await this.client.pair(); await this.context.secrets.store("stationeers.bridge.token", token); } this.client.setEndpoint(this.bridgeUrl(), token); await this.client.connect(); void vscode.window.showInformationMessage("Connected to the Stationeers bridge."); } catch { void vscode.window.showWarningMessage("Could not connect to the Stationeers game. Start the game and try again."); } }
  async autoConnect(): Promise<void> { if (vscode.env.remoteName || this.autoConnectInFlight || this.client.state === "connected" || this.client.state === "discovering" || this.client.state === "pairing") return; this.autoConnectInFlight = true; try { const token = await this.context.secrets.get("stationeers.bridge.token") ?? await this.client.pair(); await this.context.secrets.store("stationeers.bridge.token", token); this.client.setEndpoint(this.bridgeUrl(), token); await this.client.connect(); } catch { /* The game may simply not be running yet. */ } finally { this.autoConnectInFlight = false; } }
  async pair(): Promise<void> { try { const token = await this.client.pair(); await this.context.secrets.store("stationeers.bridge.token", token); await this.reconnect(); } catch { const token = await vscode.window.showInputBox({ prompt: "Paste the Stationeers bridge pairing token (automatic pairing was unavailable)", password: true, ignoreFocusOut: true }); if (!token?.trim()) return; await this.context.secrets.store("stationeers.bridge.token", token.trim()); await this.reconnect(); } }
  async refresh(): Promise<void> { await vscode.window.withProgress({ location: vscode.ProgressLocation.Window, title: "Refreshing Stationeers networks…" }, async () => { try { this.tree.setSnapshot(await this.client.refresh()); void vscode.window.showInformationMessage("Stationeers networks refreshed."); } catch (error) { void vscode.window.showErrorMessage(`Could not refresh live networks: ${error instanceof Error ? error.message : String(error)}`); } }); }
  async disconnect(): Promise<void> { this.client.disconnect(); }
  async open(node: ChipNode): Promise<void> { if (node.chip.language !== "ic10") return; try { const source = await this.client.source(node.chip); const uri = this.liveUri(source, node); const key = liveSourceKey(liveSourceIdentity(source)); const existed = this.liveSources.has(key); if (existed) this.files.restore(uri, source.source); else this.files.create(uri, source.source); this.liveSources.set(key, { identity: liveSourceIdentity(source), version: source.version, length: source.length, sha256: source.sha256, source: source.source, networkName: node.networkName, chipName: node.chip.housingName, uri: uri.toString() }); const document = await vscode.workspace.openTextDocument(uri); await vscode.window.showTextDocument(document); } catch (error) { this.showBridgeError("open", error); } }
  async pull(node: ChipNode): Promise<void> { if (node.chip.language !== "ic10") return; try { const source = await this.client.source(node.chip); const key = liveSourceKey(liveSourceIdentity(source)); const session = this.liveSources.get(key); if (!session) { await this.open(node); return; } this.files.replace(vscode.Uri.parse(session.uri), source.source); this.liveSources.set(key, { ...session, version: source.version, length: source.length, sha256: source.sha256, source: source.source }); void vscode.window.showInformationMessage(`Pulled ${node.chip.housingName} from Stationeers.`); } catch (error) { this.showBridgeError("pull", error); } }
  async push(node?: ChipNode): Promise<void> { const session = this.sessionFor(node); if (!session) { void vscode.window.showInformationMessage("Open an IC10 live editor before pushing source."); return; } const chip = node?.chip ?? this.chipForSession(session); if (!chip) { void vscode.window.showWarningMessage("The live chip is no longer present in the current discovery snapshot."); return; } const editor = vscode.window.visibleTextEditors.find((candidate) => candidate.document.uri.toString() === session.uri); if (!editor) { void vscode.window.showInformationMessage("Open the live IC10 editor before pushing source."); return; } try { await this.pushDocument(editor.document, session, chip); void vscode.window.showInformationMessage(`Pushed ${session.chipName} to Stationeers.`); } catch (error) { this.showBridgeError("push", error); } }
  async compare(node: ChipNode, uri?: vscode.Uri): Promise<void> { try { const source = await this.client.source(node.chip); const session = this.sessionFor(node); const local = session ? vscode.Uri.parse(session.uri) : uri ?? vscode.window.activeTextEditor?.document.uri; if (!local) { void vscode.window.showInformationMessage("Open or select an IC10 file to compare."); return; } const localDocument = await vscode.workspace.openTextDocument(local); const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`; const localUri = vscode.Uri.from({ scheme: "stationeers-compare", path: `/${id}-editor.ic10` }); const remoteUri = vscode.Uri.from({ scheme: "stationeers-compare", path: `/${id}-game.ic10` }); this.compareFiles.create(localUri, localDocument.getText()); this.compareFiles.create(remoteUri, source.source); await vscode.commands.executeCommand("vscode.diff", localUri, remoteUri, `${node.chip.housingName} · bridge compare`); } catch (error) { void vscode.window.showErrorMessage(`Could not compare source: ${error instanceof Error ? error.message : String(error)}`); } }
  async copyReference(node: ChipNode): Promise<void> { await vscode.env.clipboard.writeText(node.chip.housingReferenceId); void vscode.window.showInformationMessage(`Copied housing reference ${node.chip.housingReferenceId}.`); }
  async search(): Promise<void> { if (this.client.state !== "connected") return; const value = await vscode.window.showInputBox({ prompt: "Filter live networks and chips", placeHolder: "Name, prefab, language, or reference; leave blank to clear", value: this.tree.filter, ignoreFocusOut: true }); if (value !== undefined) { this.tree.setFilter(value); await this.updateContext(this.client.state); } }
  async clearFilter(): Promise<void> { if (!this.tree.filter) return; this.tree.setFilter(""); await this.updateContext(this.client.state); }
  dispose(): void { this.disposables.forEach((item) => item.dispose()); }
  private async poll(): Promise<void> { if (vscode.env.remoteName || this.refreshInFlight) return; if (this.client.state === "connected") { this.refreshInFlight = true; try { this.tree.setSnapshot(await this.client.refresh()); } catch { /* State and snapshot are updated by BridgeClient. */ } finally { this.refreshInFlight = false; } } else { await this.autoConnect(); } }
  private bridgeUrl(): string { return vscode.workspace.getConfiguration("stationeers.bridge").get<string>("url", "http://127.0.0.1:3032"); }
  private async reconnect(): Promise<void> { const token = await this.context.secrets.get("stationeers.bridge.token") ?? ""; try { this.client.setEndpoint(this.bridgeUrl(), token); } catch (error) { void vscode.window.showErrorMessage(String(error)); return; } await this.connect(); }
  private async updateContext(state: BridgeState): Promise<void> { const connected = state === "connected"; const hello = this.client.hello; await Promise.all([vscode.commands.executeCommand("setContext", LIVE_CONTEXT.connected, connected), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.filterActive, connected && Boolean(this.tree.filter)), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.read, connected && Boolean(hello?.capabilities.ic10SourceRead)), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.write, connected && Boolean(hello?.capabilities.ic10SourceWrite)), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.lua, Boolean(hello?.mods?.stationeersLua?.detected)), this.updateSelectedContext()]); this.status.text = connected ? "$(plug) Stationeers: Connected" : "$(plug) Stationeers: Disconnected"; this.status.command = connected ? "stationeers.live.disconnect" : "stationeers.live.connect"; this.status.tooltip = connected ? "Connected to the Stationeers game bridge. Click to disconnect." : "Not connected to Stationeers. Click to connect."; }
  private async updateSelectedContext(): Promise<void> { const selected = this.selectedChip ? getLiveChipContext(this.selectedChip.chip, this.client.state, this.client.hello) : undefined; await Promise.all([vscode.commands.executeCommand("setContext", LIVE_CONTEXT.language, selected?.language ?? ""), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.stale, selected?.stale ?? false), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.available, selected?.available ?? false), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.canRead, selected?.canRead ?? false), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.canCompare, selected?.canCompare ?? false), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.canPush, Boolean(selected?.canRead && this.client.hello?.capabilities.ic10SourceWrite)), vscode.commands.executeCommand("setContext", LIVE_CONTEXT.luaDebugEligible, selected?.luaDebugEligible ?? false)]); }
}

class LiveDropController implements vscode.TreeDragAndDropController<ChipNode> {
  readonly dropMimeTypes = ["text/uri-list", "files"]; readonly dragMimeTypes = ["text/uri-list"];
  constructor(private readonly client: BridgeClient, private readonly tree: LiveNetworkTree) {}
  async handleDrop(target: ChipNode | undefined, dataTransfer: vscode.DataTransfer): Promise<void> { if (!target) return; const item = dataTransfer.get("text/uri-list") ?? dataTransfer.get("files"); const value = await item?.asString(); const uri = value?.split(/\r?\n/).find((line) => line && !line.startsWith("#")); if (!uri) { void vscode.window.showWarningMessage("Drop an IC10 or Lua file onto a live chip."); return; } let file: vscode.Uri; try { file = vscode.Uri.parse(uri); } catch { void vscode.window.showWarningMessage("The dropped item is not a valid VS Code URI."); return; } const document = await vscode.workspace.openTextDocument(file); const compatible = (target.chip.language === "ic10" && document.languageId === "ic10") || (target.chip.language === "lua" && document.languageId === "lua"); if (!compatible) { void vscode.window.showWarningMessage(`This ${document.languageId} file is not compatible with the ${target.chip.language} chip.`); return; } if (target.chip.language !== "ic10") { void vscode.window.showInformationMessage("Lua deployment requires StationeersLua and is not part of this bridge item."); return; } await vscode.window.showInformationMessage("Drop preflight passed. Source was not changed; deployment requires P3.06.", "Compare").then((choice) => { if (choice) void vscode.commands.executeCommand("stationeers.live.compare", target, file); }); }
}

function scopeSort(a: BridgeScope, b: BridgeScope): number { return a.name.localeCompare(b.name) || (a.disambiguator ?? "").localeCompare(b.disambiguator ?? "") || a.scopeId.localeCompare(b.scopeId); }
function chipSort(a: BridgeChip, b: BridgeChip): number { return a.housingName.localeCompare(b.housingName) || a.housingReferenceId.localeCompare(b.housingReferenceId); }

export function registerLiveExplorer(context: vscode.ExtensionContext): LiveExplorer {
  const url = vscode.workspace.getConfiguration("stationeers.bridge").get<string>("url", "http://127.0.0.1:3032"); const token = "";
  const client = new BridgeClient(url, token); const explorer = new LiveExplorer(context, client); context.subscriptions.push(client, explorer); void explorer.autoConnect();
  context.subscriptions.push(vscode.commands.registerCommand("stationeers.live.connect", () => explorer.connect()), vscode.commands.registerCommand("stationeers.live.pair", () => explorer.pair()), vscode.commands.registerCommand("stationeers.live.refresh", () => explorer.refresh()), vscode.commands.registerCommand("stationeers.live.disconnect", () => explorer.disconnect()), vscode.commands.registerCommand("stationeers.live.search", () => explorer.search()), vscode.commands.registerCommand("stationeers.live.clearFilter", () => explorer.clearFilter()), vscode.commands.registerCommand("stationeers.live.open", (node: ChipNode) => explorer.open(node)), vscode.commands.registerCommand("stationeers.live.pull", (node: ChipNode) => explorer.pull(node)), vscode.commands.registerCommand("stationeers.live.push", (node?: ChipNode) => explorer.push(node)), vscode.commands.registerCommand("stationeers.live.compare", (node: ChipNode, uri?: vscode.Uri) => explorer.compare(node, uri)), vscode.commands.registerCommand("stationeers.live.copyReference", (node: ChipNode) => explorer.copyReference(node)));
  return explorer;
}

async function sha256(source: string): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(source));
  return [...new Uint8Array(digest)].map((value) => value.toString(16).padStart(2, "0")).join("");
}
