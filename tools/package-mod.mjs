import { mkdir, readFile, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const packageJson = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
const source = path.join(root, "dist", "stationeers-toolkit");
const archive = path.join(root, "dist", `StationeersToolkit-v${packageJson.version}.zip`);

await mkdir(path.dirname(archive), { recursive: true });

const command = process.platform === "win32" ? "powershell" : "pwsh";
const script = [
  "$ErrorActionPreference = 'Stop'",
  `Compress-Archive -Path '${source.replaceAll("'", "''")}\\*' -DestinationPath '${archive.replaceAll("'", "''")}' -Force`,
].join("; ");
const result = spawnSync(command, ["-NoProfile", "-NonInteractive", "-Command", script], { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);
console.log(`Packaged StationeersToolkit ${packageJson.version} at ${archive}`);
