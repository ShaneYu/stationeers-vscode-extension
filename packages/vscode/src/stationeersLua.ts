export type StationeersLuaState = "disabled" | "checking" | "available" | "no-scope" | "unavailable" | "incompatible" | "error";
export const SUPPORTED_STATIONEERS_LUA_VERSION = "0.9.5.0";

export interface StationeersLuaTransport { fetch(input: string, init: RequestInit): Promise<Response> }
export interface StationeersLuaStatus { name: string; status: string; version: string; debuggerEnabled: boolean }
export interface StationeersLuaEditor {
  editorOpen: boolean;
  allowNetworkChipAccess: boolean;
  allowNetworkChipAccessOnlyForWirelessBoards: boolean;
  wirelessRemoteAccessOnly: boolean;
  selectedChipRefId: string | null;
  selectedHousingRefId: string | null;
  selectedHousingName: string | null;
  networkId: string | null;
  networkIds: string[];
  networkNames: Record<string, string>;
  accessibleChipCount: number;
  selectedChipDebuggerAvailable: boolean;
  selectedChipDebuggerReason: string;
}
export interface StationeersLuaChip {
  refId: string;
  housingRefId: string;
  isLua: boolean;
  isLibrary: boolean;
  hasError: boolean;
  isSelected: boolean;
  sourceLength: number;
  sourceVersion: string;
  housingName: string;
  housingType: string;
  networkId: string;
  isOn: boolean;
  modules: unknown[];
  loadedLibraries: unknown[];
  [key: string]: unknown;
}
export interface StationeersLuaChipTarget { refId: string | number; housingRefId: string | number }
export interface StationeersLuaWriteResult {
  refId: string;
  mode: "chip" | "editor_then_chip";
  editorSynced: boolean;
  editorSyncPath: string;
  editorSyncReason: string;
  sourceVersion: string;
}

export class StationeersLuaError extends Error {
  readonly code: string;
  readonly status: number;
  readonly retryable: boolean;
  readonly noScope: boolean;
  readonly details?: unknown;
  constructor(code: string, status: number, message: string, retryable = false, noScope = false, details?: unknown) {
    super(message); this.name = "StationeersLuaError"; this.code = code; this.status = status; this.retryable = retryable; this.noScope = noScope; this.details = details;
  }
}

const LOOPBACK = /^https?:\/\/(?:127\.0\.0\.1|localhost|\[::1\])(?::\d+)?$/i;

export class StationeersLuaClient {
  private readonly transport: StationeersLuaTransport;
  private readonly abort = new AbortController();
  private requestAbort?: AbortController;
  private readonly timeoutMs: number;
  private baseUrl: string;
  private stateValue: StationeersLuaState = "disabled";
  private editorValue?: StationeersLuaEditor;
  private chipsValue?: StationeersLuaChip[];
  private readonly stateListeners = new Set<(state: StationeersLuaState) => void>();

  constructor(baseUrl: string, transport: StationeersLuaTransport = { fetch: globalThis.fetch.bind(globalThis) }, timeoutMs = 10_000) {
    this.baseUrl = validateLoopback(baseUrl); this.transport = transport; this.timeoutMs = timeoutMs;
  }
  get state(): StationeersLuaState { return this.stateValue; }
  get editorSnapshot(): StationeersLuaEditor | undefined { return this.editorValue; }
  get chipsSnapshot(): readonly StationeersLuaChip[] | undefined { return this.chipsValue; }
  onDidChangeState(listener: (state: StationeersLuaState) => void): { dispose(): void } { this.stateListeners.add(listener); return { dispose: () => this.stateListeners.delete(listener) }; }
  setEndpoint(baseUrl: string): void {
    this.baseUrl = validateLoopback(baseUrl);
    this.disconnect();
  }

  async status(signal?: AbortSignal): Promise<StationeersLuaStatus> {
    this.setState("checking");
    try {
      const value = validateStatus(await this.requestJson("/api/status", signal));
      if (value.name !== "StationeersLua") {
        throw new StationeersLuaError("incompatible", 426, `The service identified itself as ${value.name}, not StationeersLua.`);
      }
      if (value.version !== SUPPORTED_STATIONEERS_LUA_VERSION) {
        throw new StationeersLuaError("incompatible", 426, `StationeersLua ${value.version} is not supported by the validated ${SUPPORTED_STATIONEERS_LUA_VERSION} contract.`);
      }
      if (value.status !== "ok") {
        throw new StationeersLuaError("service_unavailable", 503, `StationeersLua reported service status ${value.status}.`, true);
      }
      this.setState("available");
      return value;
    } catch (error) {
      this.applyErrorState(error);
      throw error;
    }
  }
  async editor(signal?: AbortSignal): Promise<StationeersLuaEditor> {
    this.setState("checking");
    try { const value = validateEditor(await this.requestJson("/api/editor", signal)); this.editorValue = value; if (!hasScope(value)) this.setState("no-scope"); else this.setState("available"); return value; }
    catch (error) { this.applyErrorState(error); throw error; }
  }
  async chips(signal?: AbortSignal): Promise<StationeersLuaChip[]> {
    try { const value = validateChips(await this.requestJson("/api/chips", signal)); this.chipsValue = value; this.setState("available"); return value; }
    catch (error) { this.applyErrorState(error); throw error; }
  }
  isAccessible(refId: string | number): boolean | undefined {
    const id = normalizeRefId(refId); return this.chipsValue?.some((chip) => chip.refId === id);
  }
  async pull(target: StationeersLuaChipTarget, signal?: AbortSignal): Promise<string> {
    const id = normalizeRefId(target.refId);
    const housingId = normalizeRefId(target.housingRefId);
    const chips = this.chipsValue ?? await this.chips(signal);
    if (!chips.some((chip) => chip.refId === id && chip.housingRefId === housingId && chip.isLua)) {
      throw new StationeersLuaError("inaccessible", 403, `Lua chip ${id} is not accessible through the current editor or wireless scope.`);
    }
    try {
      return validateSourceResponse(
        await this.requestJson(`/api/chips/${encodeURIComponent(id)}/code?mode=chip`, signal),
        id,
      );
    }
    catch (error) { this.applyErrorState(error); throw error; }
  }
  async push(target: StationeersLuaChipTarget, source: string, signal?: AbortSignal): Promise<StationeersLuaWriteResult> {
    const id = normalizeRefId(target.refId); const housingId = normalizeRefId(target.housingRefId);
    const editor = this.editorValue ?? await this.editor(signal);
    const selected = editor.editorOpen
      && editor.selectedChipRefId === id
      && editor.selectedHousingRefId === housingId;
    if (!selected) {
      const chips = this.chipsValue ?? await this.chips(signal);
      if (!chips.some((chip) => chip.refId === id && chip.housingRefId === housingId)) throw new StationeersLuaError("inaccessible", 403, `Lua chip ${id} is not accessible through the current editor or wireless scope.`);
    }
    const mode = selected ? "editor_then_chip" : "chip";
    try {
      return validateWriteResponse(
        await this.requestJson(`/api/chips/${encodeURIComponent(id)}/code?mode=${mode}`, signal, "PUT", source),
        id,
        mode,
      );
    }
    catch (error) { this.applyErrorState(error); throw error; }
  }
  disconnect(): void { this.requestAbort?.abort(); this.editorValue = undefined; this.chipsValue = undefined; this.setState("disabled"); }
  dispose(): void { this.disconnect(); this.abort.abort(); this.stateListeners.clear(); }

  private async requestJson(path: string, signal?: AbortSignal, method = "GET", body?: string): Promise<unknown> {
    const response = await this.requestResponse(path, signal, method, body);
    try { return await response.json(); } catch { throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned malformed JSON."); }
  }
  private async requestResponse(path: string, signal?: AbortSignal, method = "GET", body?: string): Promise<Response> {
    this.requestAbort?.abort();
    const requestAbort = new AbortController();
    this.requestAbort = requestAbort;
    const timer = this.timeoutMs > 0 ? setTimeout(() => requestAbort.abort(), this.timeoutMs) : undefined;
    const merged = signal ? AbortSignal.any([requestAbort.signal, signal, this.abort.signal]) : AbortSignal.any([requestAbort.signal, this.abort.signal]);
    let response: Response;
    try { response = await this.transport.fetch(`${this.baseUrl}${path}`, { method, headers: body === undefined ? { Accept: "application/json" } : { Accept: "text/plain, application/json", "Content-Type": "text/plain; charset=utf-8" }, body, signal: merged }); }
    catch (error) { throw new StationeersLuaError(merged.aborted ? "cancelled" : "transport", 503, error instanceof Error ? error.message : "StationeersLua request failed.", true); }
    finally { if (timer !== undefined) clearTimeout(timer); }
    if (!response.ok) throw await httpError(response);
    return response;
  }
  private applyErrorState(error: unknown): void {
    if (!(error instanceof StationeersLuaError) || error.code === "cancelled") return;
    if (error.noScope) this.setState("no-scope");
    else if (error.code === "incompatible") this.setState("incompatible");
    else if (error.code === "transport" || error.code === "service_unavailable") this.setState("unavailable");
    else this.setState("error");
  }
  private setState(state: StationeersLuaState): void { this.stateValue = state; for (const listener of this.stateListeners) listener(state); }
}

async function httpError(response: Response): Promise<StationeersLuaError> {
  const text = await response.text(); let message = text || `StationeersLua request failed (${response.status}).`; let details: unknown;
  try { const json: unknown = JSON.parse(text); details = json; if (isRecord(json) && typeof json.error === "string") message = json.error; } catch { /* plain-text error */ }
  const noScope = response.status === 400 && /no IC editor|wireless|scope|network/i.test(message);
  return new StationeersLuaError(noScope ? "no_scope" : "http_error", response.status, message, response.status >= 500 || response.status === 429, noScope, details);
}

function validateStatus(value: unknown): StationeersLuaStatus { if (!isRecord(value) || typeof value.name !== "string" || typeof value.status !== "string" || typeof value.version !== "string" || typeof value.debugger_enabled !== "boolean") throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed status response."); return { name: value.name, status: value.status, version: value.version, debuggerEnabled: value.debugger_enabled }; }
function validateSourceResponse(value: unknown, expectedRefId: string): string {
  if (
    !isRecord(value)
    || !isRef(value.ref_id)
    || normalizeRefId(value.ref_id) !== expectedRefId
    || typeof value.source !== "string"
    || value.is_lua !== true
    || typeof value.is_library !== "boolean"
  ) {
    throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed source response.");
  }
  return value.source;
}
function validateWriteResponse(
  value: unknown,
  expectedRefId: string,
  expectedMode: "chip" | "editor_then_chip",
): StationeersLuaWriteResult {
  if (
    !isRecord(value)
    || value.success !== true
    || !isRef(value.ref_id)
    || normalizeRefId(value.ref_id) !== expectedRefId
    || value.mode !== expectedMode
    || typeof value.editor_synced !== "boolean"
    || typeof value.editor_sync_path !== "string"
    || typeof value.editor_sync_reason !== "string"
    || !isRef(value.source_version)
  ) {
    throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed source-write response.");
  }
  return {
    refId: expectedRefId,
    mode: expectedMode,
    editorSynced: value.editor_synced,
    editorSyncPath: value.editor_sync_path,
    editorSyncReason: value.editor_sync_reason,
    sourceVersion: normalizeRefId(value.source_version),
  };
}
function validateEditor(value: unknown): StationeersLuaEditor {
  if (
    !isRecord(value)
    || typeof value.editor_open !== "boolean"
    || typeof value.allow_network_chip_access !== "boolean"
    || typeof value.allow_network_chip_access_only_for_wireless_boards !== "boolean"
  ) {
    throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed editor response.");
  }
  const wirelessRemoteAccessOnly = value.wireless_remote_access_only ?? false;
  const selectedChipRefId = value.selected_chip_ref_id ?? null;
  const selectedHousingRefId = value.selected_housing_ref_id ?? null;
  const selectedHousingName = value.selected_housing_name ?? null;
  const networkId = value.network_id ?? null;
  const networkIds = value.network_ids ?? [];
  const networkNames = value.network_names ?? {};
  const accessibleChipCount = value.accessible_chip_count ?? 0;
  const selectedChipDebuggerAvailable = value.selected_chip_debugger_available ?? false;
  const selectedChipDebuggerReason = value.selected_chip_debugger_reason ?? "";
  if (
    typeof wirelessRemoteAccessOnly !== "boolean"
    || !isNullableRef(selectedChipRefId)
    || !isNullableRef(selectedHousingRefId)
    || !isNullableString(selectedHousingName)
    || !isNullableRef(networkId)
    || !Array.isArray(networkIds)
    || !networkIds.every(isRef)
    || !isRecord(networkNames)
    || !Object.values(networkNames).every((name) => typeof name === "string")
    || typeof accessibleChipCount !== "number"
    || !Number.isSafeInteger(accessibleChipCount)
    || accessibleChipCount < 0
    || typeof selectedChipDebuggerAvailable !== "boolean"
    || typeof selectedChipDebuggerReason !== "string"
  ) {
    throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed editor response.");
  }
  return {
    editorOpen: value.editor_open,
    allowNetworkChipAccess: value.allow_network_chip_access,
    allowNetworkChipAccessOnlyForWirelessBoards: value.allow_network_chip_access_only_for_wireless_boards,
    wirelessRemoteAccessOnly,
    selectedChipRefId: selectedChipRefId === null ? null : normalizeRefId(selectedChipRefId),
    selectedHousingRefId: selectedHousingRefId === null ? null : normalizeRefId(selectedHousingRefId),
    selectedHousingName,
    networkId: networkId === null ? null : normalizeRefId(networkId),
    networkIds: networkIds.map(normalizeRefId),
    networkNames: networkNames as Record<string, string>,
    accessibleChipCount,
    selectedChipDebuggerAvailable,
    selectedChipDebuggerReason,
  };
}
function validateChips(value: unknown): StationeersLuaChip[] { if (!Array.isArray(value)) throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed chip response."); return value.map((item) => { if (!isRecord(item) || !isRef(item.ref_id) || !isRef(item.housing_ref_id) || typeof item.is_lua !== "boolean" || typeof item.is_library !== "boolean" || typeof item.has_error !== "boolean" || typeof item.is_selected !== "boolean" || typeof item.source_length !== "number" || !Number.isSafeInteger(item.source_length) || item.source_length < 0 || !isRef(item.source_version) || typeof item.housing_name !== "string" || typeof item.housing_type !== "string" || !isRef(item.network_id) || typeof item.is_on !== "boolean" || !Array.isArray(item.modules) || !Array.isArray(item.loaded_libraries)) throw new StationeersLuaError("malformed_response", 502, "StationeersLua returned a malformed chip response."); return { ...item, refId: normalizeRefId(item.ref_id), housingRefId: normalizeRefId(item.housing_ref_id), sourceVersion: normalizeRefId(item.source_version), housingName: item.housing_name, housingType: item.housing_type, networkId: normalizeRefId(item.network_id), isLua: item.is_lua, isLibrary: item.is_library, hasError: item.has_error, isSelected: item.is_selected, sourceLength: item.source_length, isOn: item.is_on, modules: item.modules, loadedLibraries: item.loaded_libraries }; }); }
function hasScope(editor: StationeersLuaEditor): boolean {
  const exactEditorTarget = editor.editorOpen
    && editor.selectedChipRefId !== null
    && editor.selectedHousingRefId !== null;
  const networkScope = editor.allowNetworkChipAccess
    && (editor.editorOpen || editor.wirelessRemoteAccessOnly)
    && editor.networkIds.length > 0;
  return exactEditorTarget || networkScope;
}
function validateLoopback(value: string): string { const normalized = value.trim().replace(/\/$/, ""); if (!LOOPBACK.test(normalized)) throw new Error("StationeersLua URL must use a local loopback endpoint."); return normalized; }
function normalizeRefId(value: string | number): string { if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) return String(value); if (typeof value === "string" && /^(?:0|[1-9]\d*)$/.test(value) && Number.isSafeInteger(Number(value))) return String(Number(value)); throw new StationeersLuaError("invalid_ref_id", 400, "StationeersLua reference IDs must be non-negative safe integers."); }
function isRef(value: unknown): value is string | number { try { normalizeRefId(value as string | number); return true; } catch { return false; } }
function isNullableRef(value: unknown): value is string | number | null { return value === null || isRef(value); }
function isNullableString(value: unknown): value is string | null { return value === null || typeof value === "string"; }
function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
