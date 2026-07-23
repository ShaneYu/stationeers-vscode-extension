import * as esbuild from "esbuild";
import { rm } from "node:fs/promises";

const production = process.argv.includes("--production");
const watch = process.argv.includes("--watch");

if (production && !watch) {
  await rm("dist", { force: true, recursive: true });
}

const context = await esbuild.context({
  bundle: true,
  entryPoints: ["src/extension.ts"],
  external: ["vscode"],
  format: "cjs",
  logLevel: "info",
  minify: production,
  outfile: "dist/extension.js",
  platform: "node",
  sourcemap: !production,
  target: "node22",
});

if (watch) {
  await context.watch();
  console.log("Watching the IC10 extension...");
} else {
  await context.rebuild();
  await context.dispose();
}
