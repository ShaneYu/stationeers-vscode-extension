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

const result = spawnSync(
  process.execPath,
  ["--experimental-strip-types", "--test", ...testFiles],
  {
    stdio: "inherit",
    cwd: path.resolve(__dirname, ".."),
  },
);

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
