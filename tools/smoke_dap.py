#!/usr/bin/env python3
"""Exercise the compiled IC10 debug adapter over its real DAP transport."""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import threading
from pathlib import Path
from typing import Any, BinaryIO

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
SCENARIO = (
    REPOSITORY_ROOT
    / "crates"
    / "ic10-sim"
    / "tests"
    / "fixtures"
    / "multi-ic.ic10sim.json"
)


def default_binary() -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    return REPOSITORY_ROOT / "target" / "debug" / f"ic10-dap{suffix}"


def write_message(stream: BinaryIO, message: dict[str, Any]) -> None:
    payload = json.dumps(message, separators=(",", ":")).encode()
    stream.write(f"Content-Length: {len(payload)}\r\n\r\n".encode() + payload)
    stream.flush()


def read_message(stream: BinaryIO) -> dict[str, Any] | None:
    content_length = None
    while True:
        line = stream.readline()
        if not line:
            return None
        if line in {b"\r\n", b"\n"}:
            break
        name, _, value = line.decode("ascii").partition(":")
        if name.casefold() == "content-length":
            content_length = int(value.strip())
    if content_length is None:
        raise RuntimeError("DAP message had no Content-Length header")
    payload = stream.read(content_length)
    if len(payload) != content_length:
        raise RuntimeError("DAP closed in the middle of a message")
    # VS Code parses DAP messages through JavaScript, so exercise the adapter with
    # the same IEEE-754 integer round trip instead of Python's arbitrary precision.
    value = json.loads(payload, parse_int=lambda number: int(float(number)))
    if not isinstance(value, dict):
        raise RuntimeError("Expected a DAP object")
    return value


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=default_binary())
    args = parser.parse_args(argv)
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"debug adapter does not exist: {binary}")

    process = subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    messages: queue.Queue[dict[str, Any] | BaseException | None] = queue.Queue()

    def read_adapter() -> None:
        try:
            while (message := read_message(process.stdout)) is not None:
                messages.put(message)
            messages.put(None)
        except BaseException as error:
            messages.put(error)

    threading.Thread(target=read_adapter, daemon=True).start()
    sequence = 0

    def request(command: str, arguments: dict[str, Any] | None = None) -> int:
        nonlocal sequence
        sequence += 1
        write_message(
            process.stdin,
            {
                "seq": sequence,
                "type": "request",
                "command": command,
                "arguments": arguments or {},
            },
        )
        return sequence

    def receive_response(request_sequence: int) -> dict[str, Any]:
        while True:
            message = messages.get(timeout=10)
            if isinstance(message, BaseException):
                raise message
            if message is None:
                stderr = process.stderr.read().decode(errors="replace") if process.stderr else ""
                raise RuntimeError(f"DAP exited unexpectedly: {stderr}")
            if message.get("type") == "response" and message.get("request_seq") == request_sequence:
                if not message.get("success"):
                    raise RuntimeError(f"DAP request failed: {message}")
                return message

    def receive_event(name: str, *, reason: str | None = None) -> dict[str, Any]:
        while True:
            message = messages.get(timeout=10)
            if isinstance(message, BaseException):
                raise message
            if message is None:
                stderr = process.stderr.read().decode(errors="replace") if process.stderr else ""
                raise RuntimeError(f"DAP exited unexpectedly: {stderr}")
            if message.get("type") != "event" or message.get("event") != name:
                continue
            if reason is not None and message.get("body", {}).get("reason") != reason:
                continue
            return message

    try:
        initialized = receive_response(request("initialize"))["body"]
        assert initialized["supportsSetVariable"]
        assert initialized["supportsSingleThreadExecutionRequests"]

        receive_response(
            request(
                "launch",
                {
                    "scenario": str(SCENARIO.resolve()),
                    "focusIc": "requester",
                    "stopOnEntry": True,
                },
            )
        )
        requester_source = SCENARIO.with_name("requester.ic10").resolve()
        breakpoints = receive_response(
            request(
                "setBreakpoints",
                {
                    "source": {"path": str(requester_source)},
                    "breakpoints": [{"line": 7}],
                },
            )
        )["body"]["breakpoints"]
        assert breakpoints == [
            {
                "verified": True,
                "line": 7,
                "message": None,
            }
        ]
        receive_response(request("setExceptionBreakpoints", {"filters": []}))
        receive_response(request("configurationDone"))
        entry = receive_event("stopped", reason="entry")
        assert entry["body"]["threadId"] == 2

        threads = receive_response(request("threads"))["body"]["threads"]
        assert [thread["name"] for thread in threads] == [
            "Ingot Supplier",
            "Ingot Requester",
        ]
        runtime_scopes = receive_response(
            request("scopes", {"frameId": 2})
        )["body"]["scopes"]
        devices_scope = next(
            scope for scope in runtime_scopes if scope["name"] == "Devices"
        )
        device_entries = receive_response(
            request(
                "variables",
                {
                    "variablesReference": devices_scope["variablesReference"],
                },
            )
        )["body"]["variables"]
        sorter_entry = next(entry for entry in device_entries if entry["name"] == "sorter")
        sorter_values = receive_response(
            request(
                "variables",
                {
                    "variablesReference": sorter_entry["variablesReference"],
                },
            )
        )["body"]["variables"]
        slots_entry = next(entry for entry in sorter_values if entry["name"] == "Slots")
        memory_entry = next(entry for entry in sorter_values if entry["name"] == "Memory")
        slot_entries = receive_response(
            request(
                "variables",
                {
                    "variablesReference": slots_entry["variablesReference"],
                },
            )
        )["body"]["variables"]
        slot_zero = next(entry for entry in slot_entries if entry["name"] == "Slot 0")
        slot_values = receive_response(
            request(
                "variables",
                {
                    "variablesReference": slot_zero["variablesReference"],
                },
            )
        )["body"]["variables"]
        assert next(value for value in slot_values if value["name"] == "Class")["value"] == "19"
        assert next(value for value in slot_values if value["name"] == "Quantity")["value"] == "5"
        receive_response(
            request(
                "setVariable",
                {
                    "variablesReference": slot_zero["variablesReference"],
                    "name": "Quantity",
                    "value": "6",
                },
            )
        )
        memory_values = receive_response(
            request(
                "variables",
                {
                    "variablesReference": memory_entry["variablesReference"],
                    "start": 3,
                    "count": 1,
                },
            )
        )["body"]["variables"]
        assert memory_values[0]["name"] == "3"
        assert memory_values[0]["value"] == "77"
        receive_response(
            request(
                "setVariable",
                {
                    "variablesReference": memory_entry["variablesReference"],
                    "name": "3",
                    "value": "88",
                },
            )
        )
        slot_watch = receive_response(
            request(
                "evaluate",
                {
                    "expression": 'device("sorter").slot[0].Quantity',
                    "frameId": 2,
                    "context": "watch",
                },
            )
        )["body"]
        assert slot_watch["result"] == "6"
        memory_watch = receive_response(
            request(
                "evaluate",
                {
                    "expression": 'device("sorter").memory[3]',
                    "frameId": 2,
                    "context": "watch",
                },
            )
        )["body"]
        assert memory_watch["result"] == "88"
        networks_scope = next(
            scope for scope in runtime_scopes if scope["name"] == "Networks"
        )
        network_entries = receive_response(
            request(
                "variables",
                {
                    "variablesReference": networks_scope["variablesReference"],
                },
            )
        )["body"]["variables"]
        assert {entry["name"] for entry in network_entries} == {
            "supplier-data",
            "requester-data",
            "shared-power",
        }
        for _ in range(5):
            receive_response(request("next", {"threadId": 1}))
            supplier_step = receive_event("stopped", reason="step")
            assert supplier_step["body"]["threadId"] == 1
        db_hover = receive_response(
            request(
                "evaluate",
                {"expression": "db:1", "frameId": 2, "context": "hover"},
            )
        )["body"]
        assert db_hover["type"] == "string"
        assert "requester" in db_hover["result"]
        assert "shared-power" in db_hover["result"]
        channel_hover = receive_response(
            request(
                "evaluate",
                {"expression": "Channel0", "frameId": 2, "context": "hover"},
            )
        )["body"]
        assert channel_hover == {
            "result": "165",
            "type": "number",
            "variablesReference": 0,
        }

        receive_response(request("continue", {"threadId": 2}))
        stopped = receive_event("stopped", reason="breakpoint")
        assert stopped["body"]["threadId"] == 2
        requester = receive_response(
            request("ic10/getState", {"threadId": 2})
        )["body"]
        registers = {
            register["name"]: register["value"]
            for register in requester["registers"]
        }
        assert registers["r0"] == "42"
        assert registers["r1"] == "0"

        receive_response(request("next", {"threadId": 2}))
        receive_event("stopped", reason="step")
        requester = receive_response(
            request("ic10/getState", {"threadId": 2})
        )["body"]
        registers = {
            register["name"]: register["value"]
            for register in requester["registers"]
        }
        assert registers["r1"] == "1"

        tick = receive_response(
            request("ic10/stepTick", {"threadId": 2})
        )["body"]["tick"]
        assert tick == 1
        receive_event("stopped", reason="step")

        scopes = receive_response(request("scopes", {"frameId": 2}))["body"]["scopes"]
        register_scope = next(scope for scope in scopes if scope["name"] == "Registers")
        receive_response(
            request(
                "setVariable",
                {
                    "variablesReference": register_scope["variablesReference"],
                    "name": "r3",
                    "value": "123",
                },
            )
        )
        requester = receive_response(
            request("ic10/getState", {"threadId": 2})
        )["body"]
        registers = {
            register["name"]: register["value"]
            for register in requester["registers"]
        }
        assert registers["r3"] == "123"

        receive_response(request("disconnect", {"terminateDebuggee": True}))
        process.stdin.close()
        process.wait(timeout=10)
        if process.returncode != 0:
            raise RuntimeError(f"DAP exited with {process.returncode}")
        print(
            "DAP transport smoke test passed "
            "(focused entry, symbolic hovers, post-yield jump, multi-file breakpoint, "
            "threads, stepping, editable registers, slots, memory, and cable channels)."
        )
        return 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
