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
const description = about.match(/<Description>([\s\S]*?)<\/Description>/)?.[1].trim() ?? title;

const prompt = createInterface({ input: process.stdin, output: process.stdout });
try {
  const workshopId = process.env.STEAM_WORKSHOP_ID || "3774046989";
  if (!/^\d+$/.test(workshopId)) throw new Error("STEAM_WORKSHOP_ID must contain only digits");
  const username = process.env.STEAM_USERNAME || (await prompt.question("Steam username: ")).trim();
  if (!username) throw new Error("A Steam username is required");
  // Do not leave a Node readline listener attached while SteamCMD owns stdin.
  prompt.close();

  const escape = (value) => value
    .replaceAll("\\", "\\\\")
    .replaceAll('"', '\\"')
    .replaceAll("\r", " ")
    .replaceAll("\n", " ");

  console.log("Building the local StationeersToolkit package...");
  const build = spawnSync(process.execPath, [path.join(root, "tools", "package-mod.mjs")], { cwd: root, stdio: "inherit" });
  if (build.error) throw build.error;
  if (build.status !== 0) process.exit(build.status ?? 1);

  await mkdir(dist, { recursive: true });
  await writeFile(vdf, `"workshopitem" {\n  "appid" "544550"\n  "publishedfileid" "${workshopId}"\n  "contentfolder" "${escape(content)}"\n  "previewfile" "${escape(path.join(content, "About", "thumb.png"))}"\n  "title" "${escape(title)}"\n  "description" "${escape(description)}"\n  "changenote" "Version ${packageJson.version}"\n}\n`, "utf8");

  console.log("SteamCMD will prompt locally for your password and Steam Guard code.");
  console.log("Password input is intentionally invisible; type it and press Enter.");
  console.log(`Workshop VDF: ${vdf}`);
  const steamCmd = process.env.STEAMCMD_PATH || (process.platform === "win32" ? "steamcmd.exe" : "steamcmd");
  const args = ["+login", username];
  if (process.env.STEAM_PASSWORD) args.push(process.env.STEAM_PASSWORD);
  args.push("+workshop_build_item", vdf, "+quit");
  const upload = spawnSync(steamCmd, args, { cwd: root, stdio: "inherit" });
  if (upload.error) throw upload.error;
  if (upload.status !== 0) process.exit(upload.status ?? 1);
  console.log("Workshop publish completed. Keep the VDF's publishedfileid for future updates.");
} finally {
  prompt.close();
}
