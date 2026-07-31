import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { resolveScenarioProgramPath } from "./scenarioUri.ts";

const intelligenceSource = fs.readFileSync(
  path.resolve(process.cwd(), "src", "environmentIntelligence.ts"),
  "utf8",
);

describe("environment context URI resolution", () => {
  it("resolves programs relative to each multi-root scenario", () => {
    const first = {
      scheme: "file",
      authority: "",
      path: "/workspace-a/env/sim.icsim",
    };
    const second = {
      scheme: "file",
      authority: "",
      path: "/workspace-b/sim.icsim",
    };
    assert.deepEqual(
      resolveScenarioProgramPath(first, "../src/main.ic10"),
      { scheme: "file", authority: "", path: "/workspace-a/src/main.ic10" },
    );
    assert.deepEqual(
      resolveScenarioProgramPath(second, "main.ic10"),
      { scheme: "file", authority: "", path: "/workspace-b/main.ic10" },
    );
  });

  it("preserves remote URI scheme and authority", () => {
    const remote = {
      scheme: "vscode-remote",
      authority: "ssh-remote+station",
      path: "/root/env/sim.icsim",
    };
    assert.deepEqual(
      resolveScenarioProgramPath(remote, "../main.ic10"),
      {
        scheme: "vscode-remote",
        authority: "ssh-remote+station",
        path: "/root/main.ic10",
      },
    );
  });

  it("keys canonical program resolutions by scenario path", () => {
    assert.match(
      intelligenceSource,
      /resolvedPrograms\[program\.path\] = resolveScenarioProgram\(uri, program\.path\)/,
    );
    assert.doesNotMatch(intelligenceSource, /resolvedPrograms\[program\.id\]/);
  });
});
