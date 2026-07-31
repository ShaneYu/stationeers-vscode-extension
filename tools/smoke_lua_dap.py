#!/usr/bin/env python3
"""Exercise Lua source breakpoints, frames, and locals over real DAP."""

from __future__ import annotations

import queue
import subprocess
import sys
import threading
from pathlib import Path

from smoke_dap import read_message, write_message

ROOT = Path(__file__).resolve().parents[1]
SCENARIO = ROOT / "examples" / "scenario-workbench" / "testing" / "workbench.icsim"
SOURCE = ROOT / "examples" / "scenario-workbench" / "supplier.lua"
REQUESTER = ROOT / "examples" / "scenario-workbench" / "requester.ic10"
BINARY = ROOT / "target" / "debug" / "ic10-dap.exe"


def main() -> int:
    process = subprocess.Popen(
        [str(BINARY)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE
    )
    assert process.stdin is not None and process.stdout is not None
    messages: queue.Queue[dict] = queue.Queue()

    def reader() -> None:
        while (message := read_message(process.stdout)) is not None:
            messages.put(message)

    threading.Thread(target=reader, daemon=True).start()
    sequence = 0

    def request(command: str, arguments: dict | None = None) -> int:
        nonlocal sequence
        sequence += 1
        write_message(
            process.stdin,
            {"seq": sequence, "type": "request", "command": command, "arguments": arguments or {}},
        )
        return sequence

    def response(request_sequence: int) -> dict:
        while True:
            message = messages.get(timeout=30)
            if message.get("type") == "response" and message.get("request_seq") == request_sequence:
                if not message.get("success"):
                    raise RuntimeError(message)
                return message

    def stopped() -> dict:
        while True:
            message = messages.get(timeout=30)
            if message.get("type") == "event" and message.get("event") == "stopped":
                return message

    try:
        response(request("initialize"))
        response(
            request(
                "launch",
                {
                    "scenario": str(SCENARIO.resolve()),
                    "focusProgram": "requester",
                    "stopOnEntry": True,
                },
            )
        )
        breakpoint = response(
            request(
                "setBreakpoints",
                {"source": {"path": str(SOURCE.resolve())}, "breakpoints": [{"line": 10}]},
            )
        )["body"]["breakpoints"][0]
        assert breakpoint["verified"] and breakpoint["line"] == 10
        response(request("configurationDone"))
        assert stopped()["body"]["reason"] == "entry"
        response(request("continue", {"threadId": 1}))
        assert stopped()["body"]["threadId"] == 2

        threads = response(request("threads"))["body"]["threads"]
        assert len(threads) == 2 and threads[1]["name"] == "Lua Item Supplier"
        stack = response(request("stackTrace", {"threadId": 2}))["body"]["stackFrames"]
        assert stack and stack[0]["line"] == 10
        scopes = response(request("scopes", {"frameId": stack[0]["id"]}))["body"]["scopes"]
        lua_scope = next(scope for scope in scopes if scope["name"] == "Lua")
        variables = response(
            request("variables", {"variablesReference": lua_scope["variablesReference"]})
        )["body"]["variables"]
        assert any(variable["name"] == "LT" for variable in variables)
        supplier_local = next(variable for variable in variables if variable["name"] == "supplier")
        assert supplier_local["value"].startswith("{")
        assert response(
            request("evaluate", {"expression": "supplier", "frameId": stack[0]["id"]})
        )["success"]
        world_scope = next(scope for scope in response(request("scopes", {"frameId": stack[0]["id"]}))["body"]["scopes"] if scope["name"] == "World")
        response(request("variables", {"variablesReference": world_scope["variablesReference"]}))
        response(
            request(
                "setBreakpoints",
                {"source": {"path": str(SOURCE.resolve())}, "breakpoints": []},
            )
        )
        response(
            request(
                "setBreakpoints",
                {"source": {"path": str(REQUESTER.resolve())}, "breakpoints": [{"line": 49}]},
            )
        )
        response(request("continue", {"threadId": 2}))
        response(
            request(
                "ic10/setWorldField",
                {"deviceId": "iron-button", "field": "Activate", "value": "1"},
            )
        )
        stimulus_stop = stopped()
        assert stimulus_stop["body"]["threadId"] == 1
        print("Lua DAP smoke test passed (mixed thread, source breakpoint, frame, and locals).")
        return 0
    finally:
        process.terminate()


if __name__ == "__main__":
    sys.exit(main())
