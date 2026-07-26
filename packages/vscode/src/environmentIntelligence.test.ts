import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { resolveScenarioProgramPath } from "./scenarioUri.ts";

describe("environment context URI resolution", () => {
  it("resolves programs relative to each multi-root scenario", () => {
    const first = {
      scheme: "file",
      authority: "",
      path: "/workspace-a/env/sim.ic10sim.json",
    };
    const second = {
      scheme: "file",
      authority: "",
      path: "/workspace-b/sim.ic10sim.json",
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
      path: "/root/env/sim.ic10sim.json",
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
});
