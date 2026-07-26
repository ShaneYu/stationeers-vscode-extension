import { readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const srcDir = path.resolve(__dirname, "..", "src");

const testFiles = readdirSync(srcDir)
  .filter((file) => file.endsWith(".test.ts"))
  .map((file) => path.join(srcDir, file))
  .sort();

console.log(`[VSCODE TEST RUNNER] Found ${testFiles.length} test files:`);
for (const file of testFiles) {
  console.log(`  - ${path.relative(srcDir, file)}`);
}

const result = spawnSync(
  process.execPath,
  ["--experimental-strip-types", "--test", "--test-reporter", "spec", ...testFiles],
  {
    stdio: "inherit",
    cwd: path.resolve(__dirname, ".."),
  },
);

if (result.status !== 0) {
  console.error(`[VSCODE TEST RUNNER] ❌ Tests failed with exit code ${result.status}`);
  process.exit(result.status ?? 1);
}

console.log(`[VSCODE TEST RUNNER] ✅ All ${testFiles.length} test files passed.`);
