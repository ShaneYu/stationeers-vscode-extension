import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";
import net from "node:net";
import path from "node:path";

class CdpClient {
  #id = 0;
  #pending = new Map();

  constructor(url) {
    this.socket = new WebSocket(url);
  }

  async open() {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(
        () => reject(new Error("CDP WebSocket connection timed out")),
        1_000,
      );
      this.socket.addEventListener(
        "open",
        () => {
          clearTimeout(timeout);
          resolve();
        },
        { once: true },
      );
      this.socket.addEventListener(
        "error",
        (event) => {
          clearTimeout(timeout);
          reject(event.error ?? new Error("CDP WebSocket connection failed"));
        },
        { once: true },
      );
    });
    this.socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (!message.id) return;
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
  }

  send(method, params = {}) {
    const id = ++this.#id;
    this.socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`${method} timed out`));
      }, 2_000);
      this.#pending.set(id, {
        resolve: (value) => {
          clearTimeout(timeout);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timeout);
          reject(error);
        },
      });
    });
  }

  close() {
    this.socket.close();
  }
}

async function unusedPort() {
  const server = net.createServer();
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  const port = typeof address === "object" && address ? address.port : 0;
  await new Promise((resolve) => server.close(resolve));
  return port;
}

async function eventually(read, description, attempts = 100) {
  let lastError;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const value = await read();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(
    `${description}${lastError ? `: ${lastError.message}` : ""}`,
  );
}

async function evaluate(client, expression) {
  const result = await client.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.text);
  }
  return result.result?.value;
}

async function key(client, key, code = key) {
  const common = {
    key,
    code,
    windowsVirtualKeyCode:
      key === "Enter" ? 13 : key === "Tab" ? 9 : key === "Escape" ? 27 : 39,
    nativeVirtualKeyCode:
      key === "Enter" ? 13 : key === "Tab" ? 9 : key === "Escape" ? 27 : 39,
  };
  await client.send("Input.dispatchKeyEvent", { type: "rawKeyDown", ...common });
  await client.send("Input.dispatchKeyEvent", { type: "keyUp", ...common });
}

async function webviewClient(port) {
  return eventually(async () => {
    const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then(
      (response) => response.json(),
    );
    for (const target of targets) {
      if (!target.webSocketDebuggerUrl) continue;
      const client = new CdpClient(target.webSocketDebuggerUrl);
      try {
        await client.open();
        await client.send("Runtime.enable");
        if (await evaluate(client, "Boolean(document.querySelector('#topologyTab'))")) {
          return client;
        }
      } catch {
        // The workbench and extension host expose several short-lived targets.
      }
      client.close();
    }
    return undefined;
  }, "the environment-editor webview did not appear");
}

export async function runKeyboardTopologySmoke({
  codePath,
  extensionRoot,
  scenario,
  temporary,
}) {
  const userDataDir = path.join(temporary, "cdp-user-data");
  const userSettingsDir = path.join(userDataDir, "User");
  await mkdir(userSettingsDir, { recursive: true });
  await writeFile(
    path.join(userSettingsDir, "settings.json"),
    JSON.stringify(
      {
        "workbench.editorAssociations": {
          "*.icsim": "ic10.environment",
          "*.icsim": "ic10.environment",
        },
      },
      null,
      2,
    ),
  );

  const port = await unusedPort();
  const child = spawn(
    codePath,
    [
      `--user-data-dir=${userDataDir}`,
      `--extensions-dir=${path.join(temporary, "cdp-extensions")}`,
      `--extensionDevelopmentPath=${extensionRoot}`,
      `--remote-debugging-port=${port}`,
      "--disable-extensions",
      "--skip-welcome",
      "--skip-release-notes",
      scenario,
    ],
    { stdio: "ignore", windowsHide: true },
  );

  let client;
  try {
    client = await webviewClient(port);
    await evaluate(client, "document.querySelector('#topologyTab').focus()");
    await key(client, "Enter");
    await eventually(
      () => evaluate(client, "document.body.classList.contains('topology-mode')"),
      "Enter did not open the Topology tab",
    );

    let focusedNode;
    for (let attempt = 0; attempt < 24 && !focusedNode; attempt += 1) {
      await key(client, "Tab");
      focusedNode = await evaluate(
        client,
        "document.activeElement?.classList.contains('topology-node') && document.activeElement.dataset.nodeId",
      );
    }
    assert(focusedNode, "Tab reaches the topology graph without a mouse");

    const originalFocus = await evaluate(
      client,
      "document.activeElement.dataset.focus",
    );
    let movedFocus;
    for (const arrow of [
      ["ArrowRight", "ArrowRight"],
      ["ArrowDown", "ArrowDown"],
      ["ArrowLeft", "ArrowLeft"],
      ["ArrowUp", "ArrowUp"],
    ]) {
      await key(client, arrow[0], arrow[1]);
      movedFocus = await evaluate(
        client,
        "document.activeElement.dataset.focus",
      );
      if (movedFocus && movedFocus !== originalFocus) break;
    }
    assert.notEqual(movedFocus, originalFocus, "an Arrow key moves graph focus");

    await key(client, "Enter");
    const sync = await eventually(
      () =>
        evaluate(
          client,
          `(() => {
            const focused = document.activeElement?.closest('[data-node-id]');
            const activeNode = document.querySelector('.topology-node.active');
            const activeList = document.querySelector('.sidebar .item.active');
            return focused && activeNode && activeList &&
              focused.dataset.nodeId === activeNode.dataset.nodeId;
          })()`,
        ),
      "Enter did not synchronize graph and inspector selection",
    );
    assert.equal(sync, true);

    await key(client, "Escape");
    assert.equal(
      await evaluate(client, "document.activeElement?.id"),
      "topologyTab",
      "Escape returns focus to the Topology tab",
    );
  } finally {
    if (client) {
      try {
        await client.send("Browser.close");
      } catch {
        // Closing the target can race with VS Code shutdown.
      }
      client.close();
    }
    if (!child.killed) child.kill();
  }
}
