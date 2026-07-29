export type BridgeState = "disabled" | "discovering" | "pairing" | "connected" | "stale" | "reconnecting" | "incompatible" | "denied";

export interface BridgeHello {
  apiVersion: string;
  bridgeVersion: string;
  gameVersion: string;
  instanceId: string;
  role: string;
  world: { loaded: boolean; name: string; epoch: string; revision: string };
  capabilities: { scopeDiscovery: boolean; ic10SourceRead: boolean; ic10SourceWrite: boolean; multiplayerRelay: boolean; eventStream: boolean };
  mods?: { stationeersLua?: { detected: boolean; version: string | null } };
  limits?: { maxSourceBytes?: number; maxRequestsPerSecond?: number };
}

export interface BridgeScope { scopeId: string; name: string; disambiguator?: string; anchorCount: number; chipIds: string[] }
export interface BridgeChip { chipId: string; housingReferenceId: string; housingName: string; housingPrefab: string; chipPrefab: string; language: "ic10" | "lua" | string; identitySource?: "chip" | "housing"; powered: boolean; source: { readable: boolean; writable: boolean; version: string; sha256: string; length?: number; bytes?: number } }
export interface BridgeSnapshot { worldEpoch: string; revision: string; scopes: BridgeScope[]; chips: BridgeChip[]; warnings: BridgeWarning[] }
export interface BridgeWarning { code: string; message: string; anchorReferenceId?: string }
export interface BridgeSource { worldEpoch: string; chipId: string; housingReferenceId: string; language: string; version: string; length: number; sha256: string; source: string }
export interface BridgeSourceWriteRequest { requestId: string; worldEpoch: string; expectedVersion: string; expectedSha256: string; source: string; sourceSha256: string }
export interface BridgeSourceWriteResponse { worldEpoch: string; chipId: string; version: string; sha256: string; applied: true }
export interface BridgeEvent { apiVersion: string; eventId: string; worldEpoch: string; revision: string; type: string; data?: Record<string, unknown> }

export interface BridgeTransport { fetch(input: string, init: RequestInit): Promise<Response> }

const LOOPBACK = /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::\d+)?$/i;

export class BridgeError extends Error {
  readonly code: string; readonly status: number; readonly retryable: boolean; readonly details?: Record<string, unknown>;
  constructor(code: string, status: number, message: string, retryable = false, details?: Record<string, unknown>) { super(message); this.name = "BridgeError"; this.code = code; this.status = status; this.retryable = retryable; this.details = details; }
}

export class BridgeClient {
  private readonly stateListeners = new Set<(state: BridgeState) => void>();
  private readonly abort = new AbortController();
  private requestAbort?: AbortController;
  private helloValue?: BridgeHello;
  private snapshotValue?: BridgeSnapshot;
  private stateValue: BridgeState = "disabled";
  private baseUrl: string;
  private token: string;
  private readonly transport: BridgeTransport;
  onDidChangeState(listener: (state: BridgeState) => void): { dispose(): void } { this.stateListeners.add(listener); return { dispose: () => this.stateListeners.delete(listener) }; }

  constructor(baseUrl: string, token: string, transport: BridgeTransport = { fetch: globalThis.fetch.bind(globalThis) }) {
    this.baseUrl = validateLoopback(baseUrl);
    this.token = token;
    this.transport = transport;
  }
  get state(): BridgeState { return this.stateValue; }
  get hello(): BridgeHello | undefined { return this.helloValue; }
  get snapshot(): BridgeSnapshot | undefined { return this.snapshotValue; }
  setEndpoint(baseUrl: string, token: string): void { this.baseUrl = validateLoopback(baseUrl); this.token = token; this.cancel(); this.stateValue = "disabled"; this.fireState(); }
  async connect(signal?: AbortSignal): Promise<BridgeSnapshot> {
    this.cancel(); this.stateValue = "discovering"; this.fireState();
    try {
      const hello = await this.get<BridgeHello>("/hello", signal);
      if (hello.apiVersion.split(".")[0] !== "1") { this.stateValue = "incompatible"; throw new BridgeError("incompatible_version", 409, `Unsupported bridge API ${hello.apiVersion}.`); }
      this.helloValue = hello;
      if (!hello.world.loaded || !hello.capabilities.scopeDiscovery) { this.stateValue = "stale"; throw new BridgeError("unavailable", 423, "The bridge is connected but the world is not ready.", true); }
      const snapshot = await this.get<BridgeSnapshot>("/scopes", signal);
      this.snapshotValue = validateSnapshot(snapshot); this.stateValue = "connected"; this.fireState(); return this.snapshotValue;
    } catch (error) {
      if (error instanceof BridgeError && error.code === "incompatible_version") { this.fireState(); throw error; }
      if (error instanceof BridgeError && error.status === 401) this.stateValue = "denied";
      else if (error instanceof BridgeError && error.status === 410) this.stateValue = "stale";
      else if (this.stateValue !== "stale") this.stateValue = "reconnecting";
      this.fireState(); throw error;
    }
  }
  async refresh(signal?: AbortSignal): Promise<BridgeSnapshot> {
    if (!this.helloValue) return this.connect(signal);
    try {
      const snapshot = validateSnapshot(await this.get<BridgeSnapshot>("/scopes", signal));
      if (snapshot.worldEpoch !== this.helloValue.world.epoch) { this.stateValue = "stale"; this.snapshotValue = undefined; this.fireState(); throw new BridgeError("stale_world", 410, "The world changed; refresh connection before retrying.", true); }
      this.snapshotValue = snapshot; this.stateValue = "connected"; this.fireState(); return snapshot;
    } catch (error) {
      this.snapshotValue = undefined;
      if (error instanceof BridgeError && error.status === 410) this.stateValue = "stale";
      else if (error instanceof BridgeError && error.status === 401) this.stateValue = "denied";
      else this.stateValue = "reconnecting";
      this.fireState();
      throw error;
    }
  }
  async pair(signal?: AbortSignal): Promise<string> {
    const value = await this.request<{ token?: unknown }>("/pair", false, signal);
    if (!isRecord(value) || typeof value.token !== "string" || value.token.length < 32) throw new BridgeError("malformed_pairing", 502, "The bridge returned an invalid pairing response.");
    this.token = value.token;
    return value.token;
  }
  async source(chip: BridgeChip, signal?: AbortSignal): Promise<BridgeSource> {
    if (chip.language !== "ic10" || !chip.source.readable) throw new BridgeError("unsupported_capability", 403, "Only readable IC10 source is supported by this bridge client.");
    if (!this.helloValue) throw new BridgeError("not_connected", 503, "Connect to the bridge first.");
    const value = validateSource(await this.get<unknown>(`/chips/${encodeURIComponent(chip.chipId)}/source?worldEpoch=${encodeURIComponent(this.helloValue.world.epoch)}`, signal));
    if (value.worldEpoch !== this.helloValue.world.epoch || value.chipId !== chip.chipId || value.housingReferenceId !== chip.housingReferenceId) {
      this.stateValue = "stale"; this.snapshotValue = undefined; this.fireState();
      throw new BridgeError("stale_target", 410, "The selected chip is no longer current.", true);
    }
    if (value.language !== "ic10") throw new BridgeError("malformed_response", 502, "The bridge returned non-IC10 source for an IC10 chip.");
    return value;
  }
  async push(chip: BridgeChip, base: Pick<BridgeSource, "worldEpoch" | "version" | "sha256">, source: string, sourceSha256: string, signal?: AbortSignal): Promise<BridgeSourceWriteResponse> {
    if (chip.language !== "ic10" || !chip.source.writable || !this.helloValue?.capabilities.ic10SourceWrite) throw new BridgeError("unsupported_capability", 403, "IC10 source writes are not enabled by this bridge.");
    if (!this.helloValue) throw new BridgeError("not_connected", 503, "Connect to the bridge first.");
    const value = await this.request<unknown>(`/chips/${encodeURIComponent(chip.chipId)}/source`, true, signal, "PUT", { requestId: randomRequestId(), worldEpoch: base.worldEpoch, expectedVersion: base.version, expectedSha256: base.sha256, source, sourceSha256 });
    if (!isRecord(value) || value.worldEpoch !== this.helloValue.world.epoch || value.chipId !== chip.chipId || typeof value.version !== "string" || typeof value.sha256 !== "string" || !/^[0-9a-f]{64}$/i.test(value.sha256) || value.applied !== true) throw new BridgeError("malformed_response", 502, "The bridge returned a malformed source-write response.");
    return value as unknown as BridgeSourceWriteResponse;
  }
  disconnect(): void { this.cancel(); this.helloValue = undefined; this.snapshotValue = undefined; this.stateValue = "disabled"; this.fireState(); }
  dispose(): void { this.disconnect(); this.abort.abort(); this.stateListeners.clear(); }
  private fireState(): void { for (const listener of this.stateListeners) listener(this.stateValue); }
  private cancel(): void { this.requestAbort?.abort(); this.requestAbort = new AbortController(); }
  private async get<T>(path: string, signal?: AbortSignal): Promise<T> {
    return this.request<T>(path, true, signal);
  }
  private async request<T>(path: string, authorized: boolean, signal?: AbortSignal, method = "GET", payload?: unknown): Promise<T> {
    const controller = this.requestAbort ?? new AbortController();
    const merged = signal ? AbortSignal.any([controller.signal, signal, this.abort.signal]) : AbortSignal.any([controller.signal, this.abort.signal]);
    let response: Response;
    const headers: Record<string, string> = { Accept: "application/json" };
    if (payload !== undefined) headers["Content-Type"] = "application/json";
    if (authorized && this.token) headers.Authorization = `Bearer ${this.token}`;
    try { response = await this.transport.fetch(`${this.baseUrl}/bridge/v1${path}`, { method, headers, body: payload === undefined ? undefined : JSON.stringify(payload), signal: merged }); }
    catch (error) { throw new BridgeError("transport", 503, error instanceof Error ? error.message : "Bridge request failed.", true); }
    const body: unknown = await response.json().catch(() => undefined);
    if (!response.ok) { const error = isRecord(body) && isRecord(body.error) ? body.error : {}; const details = isRecord(error.details) ? error.details : undefined; throw new BridgeError(typeof error.code === "string" ? error.code : "http_error", response.status, typeof error.message === "string" ? error.message : `Bridge request failed (${response.status}).`, response.status >= 500 || response.status === 429, details); }
    return body as T;
  }
}

function randomRequestId(): string { return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`; }

function validateLoopback(value: string): string { const normalized = value.trim().replace(/\/$/, ""); if (!LOOPBACK.test(normalized)) throw new Error("Stationeers bridge URL must use the configured local loopback endpoint."); return normalized; }
function validateSnapshot(value: BridgeSnapshot): BridgeSnapshot { if (!isRecord(value) || typeof value.worldEpoch !== "string" || typeof value.revision !== "string" || !Array.isArray(value.scopes) || !Array.isArray(value.chips) || !Array.isArray(value.warnings)) throw new BridgeError("malformed_response", 502, "The bridge returned a malformed discovery snapshot."); return value; }
function validateSource(value: unknown): BridgeSource {
  if (!isRecord(value) || typeof value.worldEpoch !== "string" || typeof value.chipId !== "string" || typeof value.housingReferenceId !== "string" || typeof value.language !== "string" || typeof value.version !== "string" || typeof value.length !== "number" || !Number.isSafeInteger(value.length) || value.length < 0 || typeof value.sha256 !== "string" || !/^[0-9a-f]{64}$/i.test(value.sha256) || typeof value.source !== "string") {
    throw new BridgeError("malformed_response", 502, "The bridge returned a malformed source response.");
  }
  return value as unknown as BridgeSource;
}
function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
