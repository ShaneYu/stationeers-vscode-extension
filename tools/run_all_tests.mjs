import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

const steps = [
  {
    name: "Release Hardening Unit Tests",
    command: "node",
    args: ["--test", "tools/test_release_hardening.mjs"],
  },
  {
    name: "Simulator Conformance Matrix",
    command: "python",
    args: ["tools/conformance/generate.py", "--check"],
  },
  {
    name: "Lua API Profile Generation",
    command: "python",
    args: ["tools/lua_api_profile.py", "--check"],
  },
  {
    name: "Template Manifest Schema",
    command: "node",
    args: ["--test", "templates/manifest.test.mjs"],
  },
  {
    name: "Cargo Workspace Unit Tests",
    command: "cargo",
    args: ["test", "--workspace", "--locked", "--", "--nocapture"],
  },
  {
    name: "Stationpedia Unit Tests",
    command: "python",
    args: ["-m", "unittest", "discover", "-s", "tools/stationpedia/tests", "-v"],
  },
  {
    name: "Build Rust Binaries (Debug)",
    command: "cargo",
    args: ["build", "--locked", "-p", "ic10-lsp", "-p", "ic10-dap", "-p", "ic10-runner"],
  },
  {
    name: "Canonical Template Integration Fixtures",
    command: "node",
    args: ["tools/test_templates.mjs"],
  },
  {
    name: "Native LSP Transport Smoke Test",
    command: "python",
    args: ["tools/smoke_lsp.py"],
  },
  {
    name: "Native DAP Transport Smoke Test",
    command: "python",
    args: ["tools/smoke_dap.py"],
  },
  {
    name: "VS Code Extension Package Tests",
    command: "npm",
    args: ["run", "test", "--workspace", "packages/vscode"],
  },
];

console.log(`[CI RUNNER] Starting test suite (${steps.length} steps)...`);

for (let i = 0; i < steps.length; i++) {
  const step = steps[i];
  console.log(`\n==================================================`);
  console.log(`[CI RUNNER] [Step ${i + 1}/${steps.length}] ${step.name}`);
  console.log(`[CI RUNNER] Command: ${step.command} ${step.args.join(" ")}`);
  console.log(`==================================================\n`);

  const startTime = Date.now();
  const result = spawnSync(step.command, step.args, {
    cwd: repositoryRoot,
    stdio: "inherit",
    shell: process.platform === "win32",
  });

  const durationSec = ((Date.now() - startTime) / 1000).toFixed(2);

  if (result.status !== 0) {
    console.error(`\n==================================================`);
    console.error(`[CI RUNNER] ❌ STEP FAILED AT STEP ${i + 1}/${steps.length}: ${step.name}`);
    console.error(`[CI RUNNER] Command failed: ${step.command} ${step.args.join(" ")}`);
    console.error(`[CI RUNNER] Exit code: ${result.status}`);
    console.error(`[CI RUNNER] Duration: ${durationSec}s`);
    console.error(`==================================================\n`);
    process.exit(result.status ?? 1);
  }

  console.log(`[CI RUNNER] ✅ Step ${i + 1} passed in ${durationSec}s`);
}

console.log(`\n==================================================`);
console.log(`[CI RUNNER] 🎉 ALL TEST STEPS PASSED SUCCESSFULLY!`);
console.log(`==================================================\n`);
