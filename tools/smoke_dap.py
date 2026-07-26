#!/usr/bin/env python3
"""Exercise the compiled IC10 debug adapter over its real DAP transport."""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import tempfile
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
        for capability in [
            "supportsConditionalBreakpoints",
            "supportsHitConditionalBreakpoints",
            "supportsLogPoints",
            "supportsFunctionBreakpoints",
            "supportsDataBreakpoints",
            "supportsExceptionInfoRequest",
            "supportsRestartRequest",
            "supportsGotoTargetsRequest",
            "supportsInlineValues",
            "supportsStepBack",
        ]:
            assert initialized[capability], capability

        receive_response(
            request(
                "launch",
                {
                    "scenario": str(SCENARIO.resolve()),
                    "focusIc": "requester",
                    "stopOnEntry": True,
                    "enableHistory": True,
                },
            )
        )
        requester_source = SCENARIO.with_name("requester.ic10").resolve()
        invalid_breakpoint = receive_response(
            request(
                "setBreakpoints",
                {
                    "source": {"path": str(requester_source)},
                    "breakpoints": [{"line": 7, "condition": "(r0 + 1"}],
                },
            )
        )["body"]["breakpoints"][0]
        assert invalid_breakpoint["verified"] is False
        assert "unclosed" in invalid_breakpoint["message"]
        breakpoints = receive_response(
            request(
                "setBreakpoints",
                {
                    "source": {"path": str(requester_source)},
                    "breakpoints": [
                        {
                            "line": 2,
                            "logMessage": "request value {r0}",
                        },
                        {
                            "line": 7,
                            "condition": "r0 == 42",
                            "hitCondition": "1",
                        },
                    ],
                },
            )
        )["body"]["breakpoints"]
        assert [(value["verified"], value["line"]) for value in breakpoints] == [
            (True, 2),
            (True, 7),
        ]
        function_breakpoint = receive_response(
            request(
                "setFunctionBreakpoints",
                {"breakpoints": [{"name": "received", "hitCondition": "1"}]},
            )
        )["body"]["breakpoints"][0]
        assert function_breakpoint["verified"] is True
        receive_response(request("setExceptionBreakpoints", {"filters": []}))
        receive_response(request("configurationDone"))
        entry = receive_event("stopped", reason="entry")
        assert entry["body"]["threadId"] == 2
        topology = receive_response(request("ic10/getTopologyState"))["body"]
        assert Path(topology["scenarioId"]).resolve() == SCENARIO.resolve()
        assert topology["devices"]["requester"]["behaviour"]["modelled"] is False
        assert topology["networks"]
        assert topology["ics"]["requester"]["sourceId"]
        assert topology["behaviourCatalog"]

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
        trace = receive_response(request("ic10/getTrace"))["body"]
        assert trace["history"]["retainedEvents"] >= 5
        assert trace["history"]["eventLimit"] == 20000
        assert trace["pathsRedacted"] is False
        assert trace["coverage"]
        written = next(
            write["target"]
            for record in trace["records"]
            if record["cpu"] == 0
            for write in record["writes"]
        )
        provenance = receive_response(
            request("ic10/previousWrite", {"target": written})
        )["body"]
        assert provenance["ic"] == "supplier"
        assert provenance["line"] > 0
        receive_response(request("stepBack", {"threadId": 1}))
        receive_event("stopped", reason="step")
        receive_response(request("next", {"threadId": 1}))
        receive_event("stopped", reason="step")
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
        inline_values = receive_response(
            request("inlineValues", {"frameId": 2, "viewPort": {"startLine": 0, "endLine": 20}})
        )["body"]["inlineValues"]
        assert {value["variableName"] for value in inline_values} >= {
            "tick",
            "operationsThisTick",
        }

        receive_response(request("continue", {"threadId": 2}))
        log_output = receive_event("output")
        assert "request value" in log_output["body"]["output"]
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

        scopes_at_breakpoint = receive_response(
            request("scopes", {"frameId": 2})
        )["body"]["scopes"]
        register_scope_at_breakpoint = next(
            scope for scope in scopes_at_breakpoint if scope["name"] == "Registers"
        )
        data_info = receive_response(
            request(
                "dataBreakpointInfo",
                {
                    "variablesReference": register_scope_at_breakpoint["variablesReference"],
                    "name": "r1",
                },
            )
        )["body"]
        assert data_info["dataId"]
        stack_scope = next(
            scope for scope in scopes_at_breakpoint if scope["name"] == "Stack"
        )
        stack_data_info = receive_response(
            request(
                "dataBreakpointInfo",
                {
                    "variablesReference": stack_scope["variablesReference"],
                    "name": "0",
                },
            )
        )["body"]
        slot_data_info = receive_response(
            request(
                "dataBreakpointInfo",
                {
                    "variablesReference": slot_zero["variablesReference"],
                    "name": "Quantity",
                },
            )
        )["body"]
        memory_data_info = receive_response(
            request(
                "dataBreakpointInfo",
                {
                    "variablesReference": memory_entry["variablesReference"],
                    "name": "3",
                },
            )
        )["body"]
        requester_network = next(
            entry for entry in network_entries if entry["name"] == "requester-data"
        )
        network_data_info = receive_response(
            request(
                "dataBreakpointInfo",
                {
                    "variablesReference": requester_network["variablesReference"],
                    "name": "Channel0",
                },
            )
        )["body"]
        status_light = next(
            entry for entry in device_entries if entry["name"] == "status-light"
        )
        device_data_info = receive_response(
            request(
                "dataBreakpointInfo",
                {
                    "variablesReference": status_light["variablesReference"],
                    "name": "On",
                },
            )
        )["body"]
        assert all(
            info["dataId"]
            for info in [
                stack_data_info,
                slot_data_info,
                memory_data_info,
                network_data_info,
                device_data_info,
            ]
        )
        data_breakpoint = receive_response(
            request(
                "setDataBreakpoints",
                {"breakpoints": [{"dataId": data_info["dataId"]}]},
            )
        )["body"]["breakpoints"][0]
        assert data_breakpoint["verified"] is True

        receive_response(request("continue", {"threadId": 2}))
        receive_event("stopped", reason="data breakpoint")
        requester = receive_response(
            request("ic10/getState", {"threadId": 2})
        )["body"]
        registers = {
            register["name"]: register["value"]
            for register in requester["registers"]
        }
        assert registers["r1"] == "1"
        changed_registers = receive_response(
            request(
                "variables",
                {
                    "variablesReference": register_scope_at_breakpoint["variablesReference"]
                },
            )
        )["body"]["variables"]
        changed_r1 = next(value for value in changed_registers if value["name"] == "r1")
        assert "valueChanged" in changed_r1["presentationHint"]["attributes"]

        goto_targets = receive_response(
            request(
                "gotoTargets",
                {"source": {"path": str(requester_source)}, "line": 2, "column": 1},
            )
        )["body"]["targets"]
        requester_target = next(
            target for target in goto_targets if "Requester" in target["label"]
        )
        receive_response(
            request("goto", {"threadId": 2, "targetId": requester_target["id"]})
        )
        receive_event("stopped", reason="goto")

        receive_response(request("restart"))
        receive_event("stopped", reason="restart")
        restarted = receive_response(
            request("ic10/getState", {"threadId": 2})
        )["body"]
        assert next(
            register["value"]
            for register in restarted["registers"]
            if register["name"] == "r1"
        ) == "0"
        receive_response(
            request("ic10/hotReload", {"preserveState": True})
        )
        receive_event("stopped", reason="restart")
        receive_response(
            request(
                "setBreakpoints",
                {
                    "source": {"path": str(requester_source)},
                    "breakpoints": [
                        {"line": 2, "logMessage": "after restart {tick}"}
                    ],
                },
            )
        )
        receive_response(request("setDataBreakpoints", {"breakpoints": []}))
        receive_response(
            request(
                "setFunctionBreakpoints",
                {"breakpoints": [{"name": "received", "hitCondition": "1"}]},
            )
        )
        receive_response(request("continue", {"threadId": 2}))
        assert "after restart" in receive_event("output")["body"]["output"]
        label_stop = receive_event("stopped", reason="breakpoint")
        assert label_stop["body"]["threadId"] == 2

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
        receive_response(
            request("ic10/hotReload", {"preserveState": False})
        )
        receive_event("stopped", reason="restart")
        reset_state = receive_response(
            request("ic10/getState", {"threadId": 2})
        )["body"]
        assert next(
            register["value"]
            for register in reset_state["registers"]
            if register["name"] == "r3"
        ) == "0"

        with tempfile.TemporaryDirectory(prefix="ic10-dap-smoke-") as temporary:
            temporary_path = Path(temporary)
            hcf_program = temporary_path / "hcf.ic10"
            hcf_program.write_text("hcf\n", encoding="utf-8")
            hcf_scenario = json.loads(SCENARIO.read_text(encoding="utf-8"))
            for device in hcf_scenario["devices"]:
                if "ic" not in device:
                    continue
                if device["id"] == "requester":
                    device["ic"]["program"] = str(hcf_program)
                else:
                    device["ic"]["program"] = str(
                        (SCENARIO.parent / device["ic"]["program"]).resolve()
                    )
            hcf_scenario_path = temporary_path / "hcf.ic10sim.json"
            hcf_scenario_path.write_text(
                json.dumps(hcf_scenario),
                encoding="utf-8",
            )
            receive_response(
                request(
                    "launch",
                    {
                        "scenario": str(hcf_scenario_path),
                        "focusIc": "requester",
                        "stopOnEntry": False,
                    },
                )
            )
            receive_response(
                request("setExceptionBreakpoints", {"filters": ["hcf"]})
            )
            receive_response(request("configurationDone"))
            hcf_stop = receive_event("stopped", reason="exception")
            assert hcf_stop["body"]["threadId"] == 2
            exception = receive_response(
                request("exceptionInfo", {"threadId": 2})
            )["body"]
            assert exception["exceptionId"] == "hcf"
            assert "explicit" in exception["description"]

        receive_response(request("disconnect", {"terminateDebuggee": True}))
        process.stdin.close()
        process.wait(timeout=10)
        if process.returncode != 0:
            raise RuntimeError(f"DAP exited with {process.returncode}")
        print(
            "DAP transport smoke test passed "
            "(conditional/hit/log/label/data/exception breakpoints, evaluator hovers, "
            "changed values, inline values, goto, restart, hot reload, deterministic "
            "stepping, editable world state, and exceptionInfo)."
        )
        return 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
