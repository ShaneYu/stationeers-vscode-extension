import { readdir, readFile } from "node:fs/promises";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const read = (file) => readFile(path.join(root, file), "utf8");
const required = [
  ["backlog/p3-10-integration-hardening.md", "real-game acceptance"],
  ["docs/live-integration/p310-integration-release-checklist.md", "server companion required"],
  ["docs/live-integration/p310-integration-release-checklist.md", "world reload and chip replacement rejection"],
  ["docs/live-integration/releases/README.md", "not-run"],
  ["docs/live-integration/releases/p310-release-report.template.md", "Real-game sequence results"],
];

const openapi = JSON.parse(await read("docs/live-integration/bridge/v1/openapi.json"));
if (openapi.openapi !== "3.0.3" || !openapi.components?.securitySchemes?.bearerAuth) {
  throw new Error("bridge OpenAPI contract is missing bearer authentication metadata");
}

const fixtureDirectory = path.join(root, "docs", "live-integration", "bridge", "v1", "fixtures");
for (const file of await readdir(fixtureDirectory)) {
  if (file.endsWith(".json")) {
    JSON.parse(await read(path.join("docs/live-integration/bridge/v1/fixtures", file)));
  }
}
if (!openapi.paths?.["/chips/{chipId}/source"]?.put?.responses?.["409"] ||
    !openapi.paths?.["/chips/{chipId}/source"]?.put?.responses?.["410"]) {
  throw new Error("source write contract must expose conflict and stale-world responses");
}

const evidence = JSON.parse(await read("docs/live-integration/evidence/p310-release-evidence.template.json"));
if (evidence.realGame?.acceptance?.status !== "not-run" ||
    evidence.realGame?.sequences?.unmoddedServerFailClosed?.status !== "not-run") {
  throw new Error("P3.10 evidence template must start with real-game gates not-run");
}
if (evidence.sanitization?.tokensRemoved !== false) {
  throw new Error("P3.10 evidence template must require sanitization review");
}

for (const [file, phrase] of required) {
  const contents = await read(file);
  if (!contents.toLowerCase().includes(phrase.toLowerCase())) {
    throw new Error(`${file} is missing required phrase: ${phrase}`);
  }
}

console.log("P3.10 documentation, contract, and evidence gates are present; runtime acceptance remains not-run until real-game evidence is supplied.");
