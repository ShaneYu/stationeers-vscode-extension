import assert from "node:assert/strict";
import test from "node:test";
import { assertSanitizedEvidence, verifyExtensionManifest } from "./verify-release-hardening.mjs";

test("sanitized evidence rejects credentials and absolute paths", () => {
  assert.throws(() => assertSanitizedEvidence("fixture.json", { sanitization: { tokensRemoved: true }, log: "Bearer abcdefghijklmnop" }), /sensitive-looking/);
  assert.throws(() => assertSanitizedEvidence("fixture.json", { sanitization: { tokensRemoved: true }, path: "C:\\Users\\local\\save.json" }), /sensitive-looking/);
});

test("sanitized evidence requires every declared redaction flag", () => {
  assert.throws(() => assertSanitizedEvidence("fixture.json", { sanitization: { tokensRemoved: true, sourceTextRemoved: false } }), /sanitization.sourceTextRemoved must be true/);
});

test("real-game acceptance cannot be promoted by fixture evidence", () => {
  assert.throws(() => assertSanitizedEvidence("fixture.json", { realGame: { acceptance: { status: "pass" } } }), /real-game acceptance/);
});

test("extension manifest has explicit dependency and package allowlist", () => {
  assert.doesNotThrow(() => verifyExtensionManifest({ files: ["dist/"], extensionDependencies: ["sumneko.lua"], main: "./dist/extension.js" }));
  assert.throws(() => verifyExtensionManifest({ files: ["dist/"], main: "./dist/extension.js" }), /sumneko.lua/);
  assert.throws(() => verifyExtensionManifest({ files: ["dist/"], extensionDependencies: ["sumneko.lua", "OrbitalFoundryModdingCrew.stationeers-lua"], main: "./dist/extension.js" }), /must not depend/);
});
