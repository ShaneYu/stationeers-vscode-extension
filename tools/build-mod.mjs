import { cp, mkdir, readFile, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const project = path.join(root, "mods", "StationeersToolkit", "src", "StationeersToolkit.csproj");
const output = path.join(root, "dist", "stationeers-toolkit");
const configuration = process.env.CONFIGURATION || "Release";

const packageJson = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
const about = await readFile(path.join(root, "mods", "StationeersToolkit", "About", "About.xml"), "utf8");
const aboutVersion = about.match(/<Version>([^<]+)<\/Version>/)?.[1];
if (aboutVersion !== packageJson.version) {
  throw new Error(`StationeersToolkit About.xml version ${aboutVersion ?? "missing"} != ${packageJson.version}`);
}

const result = spawnSync("dotnet", [
  "build", project, "--configuration", configuration, "--no-restore",
  "-p:DeployRemoteNetworkMod=false",
], { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

const buildOutput = path.join(root, "mods", "StationeersToolkit", "bin", configuration, "netstandard2.1");
await rm(output, { recursive: true, force: true });
await mkdir(output, { recursive: true });
await cp(path.join(root, "mods", "StationeersToolkit", "About"), path.join(output, "About"), { recursive: true });
await cp(path.join(root, "mods", "StationeersToolkit", "GameData"), path.join(output, "GameData"), { recursive: true });

const files = ["StationeersToolkit.dll", "StationeersToolkit.Core.dll", "StationeersToolkit.deps.json"];
for (const file of files) {
  await cp(path.join(buildOutput, file), path.join(output, file));
}

console.log(`Built StationeersToolkit ${packageJson.version} at ${output}`);
