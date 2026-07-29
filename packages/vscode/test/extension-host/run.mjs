import { build } from "esbuild";
import { cp, mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { runTests } from "@vscode/test-electron";
import { runKeyboardTopologySmoke } from "./topology-keyboard-cdp.mjs";

const packageRoot = path.resolve(import.meta.dirname, "../..");
const temporary = await mkdtemp(path.join(os.tmpdir(), "ic10-extension-host-"));
const extensionRoot = path.join(temporary, "extension");
const luaDependencyRoot = path.join(temporary, "sumneko-lua");
const workspace = path.join(temporary, "workspace");
await mkdir(path.join(extensionRoot, "dist"), { recursive: true });
await mkdir(luaDependencyRoot, { recursive: true });
await mkdir(workspace, { recursive: true });

await writeFile(
  path.join(luaDependencyRoot, "package.json"),
  `${JSON.stringify(
    {
      name: "lua",
      displayName: "Lua test dependency",
      version: "0.0.0",
      publisher: "sumneko",
      engines: { vscode: "^1.107.0" },
    },
    null,
    2,
  )}\n`,
);

const manifest = JSON.parse(
  await readFile(path.join(packageRoot, "package.json"), "utf8"),
);
manifest.main = "./dist/extension.js";
await writeFile(
  path.join(extensionRoot, "package.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
for (const directory of ["assets", "reference", "schemas", "syntaxes"]) {
  try {
    await cp(path.join(packageRoot, directory), path.join(extensionRoot, directory), {
      recursive: true,
    });
  } catch {
    // Optional development artifacts are not needed by every smoke assertion.
  }
}
await build({
  entryPoints: [path.join(packageRoot, "src", "extension.ts")],
  outfile: path.join(extensionRoot, "dist", "extension.js"),
  bundle: true,
  platform: "node",
  format: "cjs",
  external: ["vscode"],
});

const scenario = path.join(workspace, "accessibility.stationeerssim.json");
await writeFile(
  scenario,
  `${JSON.stringify(
    {
      schemaVersion: 1,
      networks: [{ id: "data", kind: "cable", cableRole: "data" }],
      devices: [
        {
          id: "sensor",
          prefab: "StructureGasSensor",
          connections: { "0": "data" },
          fields: {},
        },
        {
          id: "indicator",
          prefab: "StructureDiode",
          connections: { "0": "data" },
          fields: {},
        },
      ],
    },
    null,
    2,
  )}\n`,
);
process.env.IC10_EXTENSION_HOST_SCENARIO = scenario;

const installedCode = path.join(
  process.env.LOCALAPPDATA ?? "",
  "Programs",
  "Microsoft VS Code",
  process.platform === "win32" ? "Code.exe" : "code",
);

await runTests({
  vscodeExecutablePath: installedCode,
  extensionDevelopmentPath: [luaDependencyRoot, extensionRoot],
  extensionTestsPath: path.join(import.meta.dirname, "suite.cjs"),
  launchArgs: [workspace, "--disable-extensions"],
});

if (process.env.IC10_RUN_KEYBOARD_CDP === "1") {
  await runKeyboardTopologySmoke({
    codePath: installedCode,
    extensionRoot,
    scenario,
    temporary,
  });
}
