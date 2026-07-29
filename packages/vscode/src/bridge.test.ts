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

test("sends bearer authentication on protected requests but not pairing", async () => {
  const calls: Array<{ path: string; authorization?: string }> = [];
  const client = new BridgeClient("http://127.0.0.1:3032", "test-token", { fetch: async (url, init) => {
    calls.push({ path: url.split("/bridge/v1")[1] ?? "", authorization: (init.headers as Record<string, string>).Authorization });
    const path = url.split("/bridge/v1")[1] ?? "";
    if (path === "/hello") return new Response(JSON.stringify(hello));
    return new Response(JSON.stringify(snapshot));
  } });
  await client.connect();
  assert.deepEqual(calls.map((call) => call.authorization), ["Bearer test-token", "Bearer test-token"]);
  const pairing = new BridgeClient("http://127.0.0.1:3032", "", { fetch: async (_url, init) => {
    assert.equal((init.headers as Record<string, string>).Authorization, undefined);
    return new Response(JSON.stringify({ token: "a".repeat(32) }));
  } });
  await pairing.pair();
});

test("surfaces stale push and requires a fresh discovery epoch", async () => {
  let refreshed = false;
  const writableHello = { ...hello, world: { ...hello.world, epoch: "epoch-reload-1" }, capabilities: { ...hello.capabilities, ic10SourceWrite: true } };
  const writableChip = { ...snapshot.chips[0]!, source: { ...snapshot.chips[0]!.source, writable: true } };
  const writableSnapshot = { ...snapshot, worldEpoch: "epoch-reload-1", chips: [writableChip] };
  const client = new BridgeClient("http://127.0.0.1:3032", "secret", { fetch: async (url) => {
    const path = url.split("/bridge/v1")[1] ?? "";
    if (path === "/hello") return new Response(JSON.stringify(writableHello));
    if (path === "/scopes") {
      if (!refreshed) return new Response(JSON.stringify(writableSnapshot));
      return new Response(JSON.stringify({ ...writableSnapshot, worldEpoch: "epoch-reload-2", revision: "1" }));
    }
    return new Response(JSON.stringify({ error: { code: "stale_world", message: "refresh discovery", retryable: true } }), { status: 410 });
  } });
  await client.connect();
  await assert.rejects(client.push(writableChip, { worldEpoch: "epoch-reload-1", version: "1", sha256: "a".repeat(64) }, "move x 1", "b".repeat(64)), (error: unknown) => error instanceof BridgeError && error.code === "stale_world" && error.status === 410);
  refreshed = true;
  await assert.rejects(client.refresh(), (error: unknown) => error instanceof BridgeError && error.code === "stale_world");
  assert.equal(client.snapshot, undefined);
});

test("reads documented IC10 source and rejects malformed payloads", async () => {
  const source = { worldEpoch: "epoch-1", chipId: "9007199254740993", housingReferenceId: "12345678901234567", language: "ic10", version: "17", length: 17, sha256: "a".repeat(64), source: "alias Sensor d0\n" };
  const client = new BridgeClient("http://127.0.0.1:3032", "secret", { fetch: async (url) => {
    const path = url.split("/bridge/v1")[1] ?? "";
    if (path === "/hello") return new Response(JSON.stringify(hello));
    if (path === "/scopes") return new Response(JSON.stringify(snapshot));
    if (path.startsWith("/chips/9007199254740993/source")) return new Response(JSON.stringify(source));
    return new Response("{}", { status: 404 });
  } });
  await client.connect();
  assert.deepEqual(await client.source(snapshot.chips[0]!), source);

  const malformed = new BridgeClient("http://127.0.0.1:3032", "secret", { fetch: async (url) => {
    const path = url.split("/bridge/v1")[1] ?? "";
    if (path === "/hello") return new Response(JSON.stringify(hello));
    if (path === "/scopes") return new Response(JSON.stringify(snapshot));
    return new Response(JSON.stringify({ ...source, source: 42 }));
  } });
  await malformed.connect();
  await assert.rejects(malformed.source(snapshot.chips[0]!), (error: unknown) => error instanceof BridgeError && error.code === "malformed_response");
});

test("marks the client stale when source targets a different world or housing", async () => {
  const client = new BridgeClient("http://127.0.0.1:3032", "secret", { fetch: async (url) => {
    const path = url.split("/bridge/v1")[1] ?? "";
    if (path === "/hello") return new Response(JSON.stringify(hello));
    if (path === "/scopes") return new Response(JSON.stringify(snapshot));
    return new Response(JSON.stringify({ worldEpoch: "epoch-1", chipId: "9007199254740993", housingReferenceId: "changed", language: "ic10", version: "17", length: 0, sha256: "a".repeat(64), source: "" }));
  } });
  await client.connect();
  await assert.rejects(client.source(snapshot.chips[0]!), (error: unknown) => error instanceof BridgeError && error.code === "stale_target");
  assert.equal(client.state, "stale");
  assert.equal(client.snapshot, undefined);
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
