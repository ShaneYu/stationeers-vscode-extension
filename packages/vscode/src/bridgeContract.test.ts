import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import test from "node:test";

const fixtures = path.resolve(process.cwd(), "../../docs/live-integration/bridge/v1/fixtures");
const read = (name: string): Record<string, unknown> => JSON.parse(fs.readFileSync(path.join(fixtures, name), "utf8")) as Record<string, unknown>;

test("bridge fixtures are deterministic and use string identity fields", () => {
  const hello = read("hello.json");
  const scopes = read("scopes.json");
  const source = read("source.json");
  assert.equal(hello.apiVersion, "1.0");
  assert.equal((scopes.scopes as Array<Record<string, unknown>>)[0]?.scopeId, "scope-opaque");
  assert.equal(typeof (scopes.chips as Array<Record<string, unknown>>)[0]?.chipId, "string");
  assert.equal((scopes.chips as Array<Record<string, unknown>>)[0]?.identitySource, "chip");
  assert.equal(typeof source.chipId, "string");
  assert.equal((hello.capabilities as Record<string, unknown>).ic10SourceWrite, false);
  assert.equal((read("error-stale-world.json").error as Record<string, unknown>).retryable, true);
  assert.equal(read("event.json").type, "snapshot.invalidated");
});

test("the checked-in contract keeps read routes loopback-only and exposes conditional writes", () => {
  const contract = JSON.parse(fs.readFileSync(path.resolve(process.cwd(), "../../docs/live-integration/bridge/v1/openapi.json"), "utf8")) as { servers: Array<{ url: string }>; paths: Record<string, { get?: unknown; post?: unknown; put?: unknown; delete?: unknown }> };
  assert.equal(contract.servers[0]?.url, "http://127.0.0.1:3032/bridge/v1");
  for (const [pathName, route] of Object.entries(contract.paths)) {
    assert.ok(route.get);
    assert.equal(route.post, undefined);
    if (pathName === "/chips/{chipId}/source") assert.ok(route.put);
    else assert.equal(route.put, undefined);
    assert.equal(route.delete, undefined);
  }
});

test("fixture-backed reload and incremental discovery keeps epoch and revision explicit", () => {
  const initial = read("scopes.reload.initial.json");
  const after = read("scopes.reload.after.json");
  assert.equal(initial.worldEpoch, "epoch-reload-1");
  assert.equal(after.worldEpoch, initial.worldEpoch);
  assert.equal(Number(after.revision), Number(initial.revision) + 1);
  assert.equal((initial.scopes as unknown[]).length, 1);
  assert.equal((after.scopes as unknown[]).length, 2);
  assert.deepEqual((after.scopes as Array<Record<string, unknown>>).map((scope) => scope.scopeId), ["scope:network-a:Lab", "scope:network-b:Lab"]);
});

test("authenticated bridge and stale-push fixtures define the retry boundary", () => {
  const auth = read("authenticated-bridge.json");
  assert.equal(auth.authorization, "Bearer test-token");
  assert.equal(auth.unauthorizedStatus, 401);
  assert.equal(auth.pairDoesNotRequireAuthorization, true);
  const sequence = read("source-write.stale-sequence.json").sequence as Array<Record<string, unknown>>;
  const push = sequence.find((step) => step.step === "push")!;
  const response = push.response as Record<string, unknown>;
  assert.equal(response.status, 410);
  assert.equal(response.retryable, true);
  assert.equal((sequence[0] as Record<string, unknown>).worldEpoch, "epoch-reload-1");
  assert.equal((sequence[2] as Record<string, unknown>).result, "discard-old-target-and-reconnect");
});
