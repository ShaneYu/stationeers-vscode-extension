import assert from "node:assert/strict";
import { test } from "node:test";
import { BridgeClient, BridgeError } from "./bridge.ts";
import type { BridgeSnapshot } from "./bridge.ts";

const hello = { apiVersion: "1.0", bridgeVersion: "0.1.0", gameVersion: "fixture", instanceId: "i", role: "singlePlayer", world: { loaded: true, name: "Mars - Lua Test", epoch: "epoch-1", revision: "1" }, capabilities: { scopeDiscovery: true, ic10SourceRead: true, ic10SourceWrite: false, multiplayerRelay: false, eventStream: true } };
const snapshot: BridgeSnapshot = { worldEpoch: "epoch-1", revision: "1", scopes: [{ scopeId: "scope-a", name: "Greenhouse", anchorCount: 2, chipIds: ["9007199254740993", "2"] }, { scopeId: "scope-b", name: "Greenhouse", disambiguator: "Area 3", anchorCount: 1, chipIds: ["9007199254740993"] }], chips: [{ chipId: "9007199254740993", housingReferenceId: "12345678901234567", housingName: "Climate Controller", housingPrefab: "StructureCircuitHousing", chipPrefab: "ItemIntegratedCircuit10", language: "ic10", powered: true, source: { readable: true, writable: false, version: "17", sha256: "a" } }, { chipId: "2", housingReferenceId: "3", housingName: "Lua Controller", housingPrefab: "StructureCircuitHousing", chipPrefab: "Lua", language: "lua", powered: false, source: { readable: false, writable: false, version: "1", sha256: "b" } }], warnings: [] };

function fixtureFetch(routes: Record<string, unknown>, status = 200) { return async (url: string, _init: RequestInit): Promise<Response> => new Response(JSON.stringify(routes[url.split("/bridge/v1")[1] ?? ""]), { status, headers: { "content-type": "application/json" } }); }

test("connects using the versioned contract and preserves duplicate scope appearances", async () => {
  const client = new BridgeClient("http://127.0.0.1:3032", "secret", { fetch: fixtureFetch({ "/hello": hello, "/scopes": snapshot }) });
  const result = await client.connect();
  assert.equal(result.scopes.length, 2); assert.deepEqual(result.scopes[1]?.chipIds, ["9007199254740993"]); assert.equal(client.state, "connected");
});

test("rejects non-loopback endpoints and malformed snapshots", async () => {
  assert.throws(() => new BridgeClient("http://example.test:3032", ""), /loopback/);
  const client = new BridgeClient("http://127.0.0.1:3032", "", { fetch: fixtureFetch({ "/hello": hello, "/scopes": { worldEpoch: "epoch-1" } }) });
  await assert.rejects(client.connect(), (error: unknown) => error instanceof BridgeError && error.code === "malformed_response");
});

test("automatically pairs through the loopback bootstrap route", async () => {
  const client = new BridgeClient("http://127.0.0.1:3032", "", { fetch: fixtureFetch({ "/pair": { token: "a".repeat(32) } }) });
  assert.equal(await client.pair(), "a".repeat(32));
});

test("clears the last snapshot when the live bridge disappears", async () => {
  let available = true;
  const client = new BridgeClient("http://127.0.0.1:3032", "secret", { fetch: async (url) => {
    const path = url.split("/bridge/v1")[1] ?? "";
    if (path === "/hello") return new Response(JSON.stringify(hello), { status: 200 });
    if (path === "/scopes" && available) return new Response(JSON.stringify(snapshot), { status: 200 });
    return new Response(JSON.stringify({ error: { code: "transport_unavailable", message: "game closed" } }), { status: 503 });
  } });
  await client.connect();
  available = false;
  await assert.rejects(client.refresh());
  assert.equal(client.snapshot, undefined);
  assert.equal(client.state, "reconnecting");
});

test("cancels the previous request when reconnecting", async () => {
  let aborted = false;
  const client = new BridgeClient("http://127.0.0.1:3032", "", { fetch: async (_url, init) => { init.signal?.addEventListener("abort", () => { aborted = true; }); await new Promise((resolve) => setTimeout(resolve, 100)); throw new Error("cancelled"); } });
  const first = client.connect(); client.disconnect(); await assert.rejects(first); assert.equal(aborted, true);
});
