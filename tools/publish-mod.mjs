import { mkdir, readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import * as path from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";
import dotenv from "dotenv";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
dotenv.config({ path: path.join(root, ".env"), override: false });
const dist = path.join(root, "dist");
const content = path.join(dist, "stationeers-toolkit");
const vdf = path.join(dist, "stationeers-toolkit-workshop.vdf");
const packageJson = JSON.parse(await readFile(path.join(root, "package.json"), "utf8"));
const about = await readFile(path.join(root, "mods", "StationeersToolkit", "About", "About.xml"), "utf8");
const title = about.match(/<Name>([^<]+)<\/Name>/)?.[1] ?? "Stationeers Toolkit";
const description = about.match(/<Description>([\s\S]*?)<\/Description>/)?.[1].replace(/\s+/g, " ").trim() ?? title;
const tags = [...about.matchAll(/<Tag>([^<]+)<\/Tag>/g)].map((match) => match[1].trim()).filter(Boolean);
if (!tags.includes("Mod")) throw new Error("StationeersToolkit About.xml must include a Mod tag");
if (process.platform !== "win32") throw new Error("Workshop tag synchronization currently requires Windows and the installed Stationeers Steamworks runtime");
const stationeersDir = process.env.STATIONEERS_DIR?.trim() || path.join(process.env["ProgramFiles(x86)"] ?? "", "Steam", "steamapps", "common", "Stationeers");

const escape = (value) => value
  .replaceAll("\\", "\\\\")
  .replaceAll('"', '\\"')
  .replaceAll("\r", " ")
  .replaceAll("\n", " ");

const readVdfValue = (source, key) => {
  const pattern = new RegExp(`\\s*"${key}"\\s+"((?:\\\\.|[^"\\\\])*)"`, "m");
  return source.match(pattern)?.[1];
};

const setVdfValue = (source, key, value) => {
  const pattern = new RegExp(`(\\s*"${key}"\\s+)"((?:\\\\.|[^"\\\\])*)"`, "m");
  if (!pattern.test(source)) throw new Error(`Workshop VDF is missing a ${key} field`);
  return source.replace(pattern, `$1"${escape(value)}"`);
};

const updateVdfTags = (source) => {
  const block = [
    '  "tags" {',
    ...tags.map((tag, index) => `    "${index}" "${escape(tag)}"`),
    "  }",
  ].join("\n");
  const existing = /\n\s*"tags"\s*\{[\s\S]*?\n\s*\}/m;
  if (existing.test(source)) return source.replace(existing, `\n${block}`);
  return source.replace(/\n}\s*$/, `\n${block}\n}\n`);
};

const applyWorkshopTags = async (workshopId) => {
  const helper = path.join(root, "tools", "apply-workshop-tags.ps1");
  const result = spawnSync("powershell.exe", [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    helper,
    "-WorkshopId",
    workshopId,
    "-StationeersDir",
    stationeersDir,
    "-TagsJson",
    JSON.stringify(tags),
  ], { cwd: root, stdio: "inherit" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Steam Workshop tag synchronization exited with code ${result.status ?? 1}`);
};

const prompt = createInterface({ input: process.stdin, output: process.stdout });
try {
  const vdfSource = await readFile(vdf, "utf8");
  const configuredWorkshopId = readVdfValue(vdfSource, "publishedfileid");
  const environmentWorkshopId = process.env.STEAM_WORKSHOP_ID?.trim();
  const workshopId = environmentWorkshopId || configuredWorkshopId || "0";
  if (!/^\d+$/.test(workshopId)) throw new Error("STEAM_WORKSHOP_ID must contain only digits");
  if (environmentWorkshopId && configuredWorkshopId && configuredWorkshopId !== "0" && environmentWorkshopId !== configuredWorkshopId) {
    throw new Error(`STEAM_WORKSHOP_ID ${environmentWorkshopId} does not match committed VDF ID ${configuredWorkshopId}`);
  }
  const username = process.env.STEAM_USERNAME || (await prompt.question("Steam username: ")).trim();
  if (!username) throw new Error("A Steam username is required");
  // Do not leave a Node readline listener attached while SteamCMD owns stdin.
  prompt.close();

  console.log("Building the local StationeersToolkit package...");
  const build = spawnSync(process.execPath, [path.join(root, "tools", "package-mod.mjs")], { cwd: root, stdio: "inherit" });
  if (build.error) throw build.error;
  if (build.status !== 0) process.exit(build.status ?? 1);

  await mkdir(dist, { recursive: true });
  let updatedVdf = vdfSource;
  updatedVdf = setVdfValue(updatedVdf, "appid", "544550");
  updatedVdf = setVdfValue(updatedVdf, "publishedfileid", workshopId);
  updatedVdf = setVdfValue(updatedVdf, "contentfolder", content);
  updatedVdf = setVdfValue(updatedVdf, "previewfile", path.join(content, "About", "thumb.png"));
  updatedVdf = setVdfValue(updatedVdf, "title", title);
  updatedVdf = setVdfValue(updatedVdf, "description", description);
  updatedVdf = setVdfValue(updatedVdf, "changenote", `Version ${packageJson.version}`);
  updatedVdf = updateVdfTags(updatedVdf);
  await writeFile(vdf, updatedVdf, "utf8");

  console.log("SteamCMD will prompt locally for your password and Steam Guard code.");
  console.log("Password input is intentionally invisible; type it and press Enter.");
  console.log(`Workshop VDF (committed source of truth): ${vdf}`);
  const steamCmd = process.env.STEAMCMD_PATH || (process.platform === "win32" ? "steamcmd.exe" : "steamcmd");
  const args = ["+login", username];
  if (process.env.STEAM_PASSWORD) args.push(process.env.STEAM_PASSWORD);
  args.push("+workshop_build_item", vdf, "+quit");
  const upload = spawnSync(steamCmd, args, { cwd: root, stdio: "inherit" });
  if (upload.error) throw upload.error;
  if (upload.status !== 0) process.exit(upload.status ?? 1);
  console.log("Workshop publish completed. Applying Workshop tags through the logged-in Steam client...");
  try {
    await applyWorkshopTags(workshopId);
    console.log("Workshop tags applied. Commit the VDF after reviewing its publishedfileid and metadata.");
  } catch (error) {
    console.error(`Workshop publish completed, but tag synchronization failed: ${error.message}`);
    process.exitCode = 1;
  }
} finally {
  prompt.close();
}
