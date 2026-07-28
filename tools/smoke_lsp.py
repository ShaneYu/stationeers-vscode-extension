#!/usr/bin/env python3
"""Exercise the compiled IC10 server over its real stdio JSON-RPC transport."""

from __future__ import annotations

import argparse
import json
import os
import queue
import subprocess
import sys
import threading
from pathlib import Path
from typing import Any, BinaryIO

REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


def default_binary() -> Path:
    suffix = ".exe" if os.name == "nt" else ""
    return REPOSITORY_ROOT / "target" / "debug" / f"ic10-lsp{suffix}"


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
        raise RuntimeError("LSP message had no Content-Length header")
    body = stream.read(content_length)
    if len(body) != content_length:
        raise RuntimeError("LSP server closed in the middle of a message")
    value = json.loads(body)
    if not isinstance(value, dict):
        raise RuntimeError("Expected a JSON-RPC object")
    return value


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=default_binary())
    args = parser.parse_args(argv)
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"server binary does not exist: {binary}")

    process = subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    messages: queue.Queue[dict[str, Any] | BaseException | None] = queue.Queue()

    def read_server() -> None:
        try:
            while (message := read_message(process.stdout)) is not None:
                messages.put(message)
            messages.put(None)
        except BaseException as error:  # surfaced on the main test thread
            messages.put(error)

    threading.Thread(target=read_server, daemon=True).start()
    observed_notifications: list[dict[str, Any]] = []

    def receive(request_id: int, *, allow_error: bool = False) -> dict[str, Any]:
        while True:
            message = messages.get(timeout=30)
            if isinstance(message, BaseException):
                raise message
            if message is None:
                stderr = process.stderr.read().decode(errors="replace") if process.stderr else ""
                raise RuntimeError(f"LSP server exited unexpectedly: {stderr}")
            if message.get("id") == request_id:
                if "error" in message and not allow_error:
                    raise RuntimeError(f"LSP request {request_id} failed: {message['error']}")
                return message
            if "method" in message:
                observed_notifications.append(message)

    source = (
        'define Bridge HASH("StructureAccessBridge")\n'
        "start:\n"
        "move r0 1\n"
        "j start\n"
        'push HASH("Iron")\n'
        "push -1301215609\n"
        'push HASH("ItemOxite")\n'
        "s db Setting 0\n"
        "l r0 db Color\n"
        'push HASH("Sorter Corn")\n'
        'push STR("Hello!")\n'
        "define DOOR -793837322\n"
        "push DOOR\n"
        "alias Light d0\n"
        "s Light On 1\n"
        "l r1 Light Setting\n"
        "define LED 1944485013\n"
        "sb LED RatioCarbonDioxideInput2 0.34\n"
        "move r2 nan\n"
        "move r3 pinf\n"
        "move r4 ninf\n"
        "l r5 db:1 Channel0\n"
        "# Wait for the supplier response.\n"
    )
    uri = (REPOSITORY_ROOT / "examples" / "smoke.ic10").as_uri()

    try:
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "capabilities": {},
                    "initializationOptions": {"assetUri": "file:///extension/assets/devices"},
                },
            },
        )
        initialize = receive(1)["result"]
        capabilities = initialize["capabilities"]
        assert capabilities["completionProvider"]
        assert "-" in capabilities["completionProvider"]["triggerCharacters"]
        assert capabilities["hoverProvider"]
        assert capabilities["signatureHelpProvider"]
        assert capabilities["definitionProvider"]
        assert capabilities["referencesProvider"]
        assert capabilities["documentHighlightProvider"]
        assert capabilities["documentSymbolProvider"]
        assert capabilities["workspaceSymbolProvider"]
        assert capabilities["renameProvider"]["prepareProvider"]
        assert capabilities["codeActionProvider"]
        assert capabilities["semanticTokensProvider"]["full"]
        assert capabilities["semanticTokensProvider"]["range"]
        assert capabilities["foldingRangeProvider"]
        assert capabilities["inlayHintProvider"]
        assert capabilities["codeLensProvider"]
        assert capabilities["documentFormattingProvider"]

        write_message(
            process.stdin,
            {"jsonrpc": "2.0", "method": "initialized", "params": {}},
        )
        scenario_uri = (REPOSITORY_ROOT / "examples" / "context.stationeerssim.json").as_uri()
        backup_scenario_uri = (
            REPOSITORY_ROOT / "examples" / "backup-context.stationeerssim.json"
        ).as_uri()

        def scenario(name: str) -> str:
            return json.dumps(
                {
                    "schemaVersion": 1,
                    "networks": [
                        {"id": "data", "kind": "cable", "cableRole": "data"}
                    ],
                    "devices": [
                        {
                            "id": "main-ic",
                            "prefab": "StructureCircuitHousing",
                            "name": name,
                            "connections": {"0": "data"},
                            "ic": {
                                "program": "smoke.ic10",
                                "pins": {"d0": "light"},
                            },
                        },
                        {
                            "id": "light",
                            "prefab": "StructureWallLight",
                            "name": f"{name} Light",
                            "connections": {"0": "data"},
                        },
                    ],
                }
            )

        for environment_uri, name in [
            (scenario_uri, "Outside"),
            (backup_scenario_uri, "Backup"),
        ]:
            write_message(
                process.stdin,
                {
                    "jsonrpc": "2.0",
                    "method": "ic10/scenarioChanged",
                    "params": {
                        "scenarioUri": environment_uri,
                        "version": 1,
                        "source": scenario(name),
                        "resolvedPrograms": {"smoke.ic10": uri},
                    },
                },
            )
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didOpen",
                "params": {
                    "textDocument": {
                        "uri": uri,
                        "languageId": "ic10",
                        "version": 1,
                        "text": source,
                    }
                },
            },
        )
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 0, "character": 28},
                },
            },
        )
        hover = receive(2)["result"]["contents"]["value"]
        assert "Access Bridge" in hover
        assert "1298920475" in hover
        assert "StructureAccessBridge.png" in hover
        assert 'width="96" align="right"' in hover
        assert (
            "| Parameter&nbsp;&nbsp;&nbsp; | Logic&nbsp;ID&nbsp;&nbsp;&nbsp; "
            "| Access&nbsp;&nbsp;&nbsp; | Description |"
            in hover
        )
        assert "**R** = read · **W** = write" in hover
        assert "**R / W**&nbsp;&nbsp;&nbsp;" in hover
        assert "additional parameters are omitted" not in hover
        assert "IC10 hex `$4D6BF41B`" in hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 14,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 0, "character": 8},
                },
            },
        )
        define_hover = receive(14)["result"]["contents"]["value"]
        assert '**Value:** `HASH("StructureAccessBridge")`' in define_hover
        assert "**Hash:** `1298920475`" in define_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "textDocument/definition",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 3, "character": 3},
                },
            },
        )
        definition = receive(3)["result"]
        assert definition["range"]["start"] == {"line": 1, "character": 0}

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 4,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 2, "character": 0},
                },
            },
        )
        completion = receive(4)["result"]
        assert any(item["label"] == "add" for item in completion)

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 5,
                "method": "textDocument/signatureHelp",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 2, "character": 8},
                },
            },
        )
        signature = receive(5)["result"]
        assert signature["signatures"][0]["label"].startswith("move ")

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 6,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 4, "character": 12},
                },
            },
        )
        reagent_hover = receive(6)["result"]["contents"]["value"]
        assert "Reagent" in reagent_hover
        assert "-666742878" in reagent_hover
        assert "ItemIronIngot.png" in reagent_hover
        assert "IC10 hex `$D8424FA2`" in reagent_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 7,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 5, "character": 10},
                },
            },
        )
        ingot_hover = receive(7)["result"]["contents"]["value"]
        assert "Ingot (Iron)" in ingot_hover
        assert "ItemIronIngot.png" in ingot_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 8,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 6, "character": 15},
                },
            },
        )
        ice_hover = receive(8)["result"]["contents"]["value"]
        assert "Ice (Oxite)" in ice_hover
        assert "ItemOxite.png" in ice_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 9,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 7, "character": 3},
                },
            },
        )
        base_device_hover = receive(9)["result"]["contents"]["value"]
        assert "Base-device reference" in base_device_hover
        assert "IC Housing" in base_device_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 10,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 8, "character": 10},
                },
            },
        )
        color_hover = receive(10)["result"]["contents"]["value"]
        assert color_hover.count(">Blue</span>") == 2
        assert "background-color:#2563EB80" in color_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 11,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 9, "character": 15},
                },
            },
        )
        hash_hover = receive(11)["result"]["contents"]["value"]
        assert "CRC-32 hash literal" in hash_hover
        assert "signed `2146757988`" in hash_hover
        assert "IC10 hex `$7FF4ED64`" in hash_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 12,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 10, "character": 12},
                },
            },
        )
        string_hover = receive(12)["result"]["contents"]["value"]
        assert "Packed display string" in string_hover
        assert "`79600447942433`" in string_hover
        assert "$48656C6C6F21" in string_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 13,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 2, "character": 8},
                },
            },
        )
        literal_completion = receive(13)["result"]
        labels = {item["label"] for item in literal_completion}
        assert 'HASH("…")' in labels
        assert 'STR("…")' in labels

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 15,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 12, "character": 7},
                },
            },
        )
        define_reference_hover = receive(15)["result"]["contents"]["value"]
        assert "**Friendly name:** Composite Door" in define_reference_hover
        assert "**Prefab name:** `StructureCompositeDoor`" in define_reference_hover
        assert "StructureCompositeDoor.png" in define_reference_hover
        assert 'width="96" align="right"' in define_reference_hover
        assert "Logic parameters" not in define_reference_hover
        assert "steep pressure differentials" not in define_reference_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 16,
                "method": "textDocument/definition",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 12, "character": 7},
                },
            },
        )
        define_reference = receive(16)["result"]
        assert define_reference["range"]["start"] == {"line": 11, "character": 7}

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 17,
                "method": "textDocument/prepareRename",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 12, "character": 7},
                },
            },
        )
        prepare_rename = receive(17)["result"]
        assert prepare_rename["placeholder"] == "DOOR"
        assert prepare_rename["range"]["start"] == {"line": 12, "character": 5}
        assert prepare_rename["range"]["end"] == {"line": 12, "character": 9}

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 18,
                "method": "textDocument/rename",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 12, "character": 7},
                    "newName": "GATE",
                },
            },
        )
        rename = receive(18)["result"]
        edits = rename["changes"][uri]
        assert len(edits) == 2
        assert {edit["range"]["start"]["line"] for edit in edits} == {11, 12}
        assert all(edit["newText"] == "GATE" for edit in edits)

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 19,
                "method": "textDocument/prepareRename",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 1, "character": 2},
                },
            },
        )
        prepare_label_rename = receive(19)["result"]
        assert prepare_label_rename["placeholder"] == "start"
        assert prepare_label_rename["range"]["start"] == {"line": 1, "character": 0}
        assert prepare_label_rename["range"]["end"] == {"line": 1, "character": 5}

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 20,
                "method": "textDocument/rename",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 1, "character": 2},
                    "newName": "mainLoop",
                },
            },
        )
        label_rename = receive(20)["result"]
        label_edits = label_rename["changes"][uri]
        assert len(label_edits) == 2
        assert {edit["range"]["start"]["line"] for edit in label_edits} == {1, 3}
        assert label_edits[0]["range"]["end"]["character"] != 6
        assert all(edit["newText"] == "mainLoop" for edit in label_edits)

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 21,
                "method": "textDocument/rename",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 12, "character": 7},
                    "newName": "Bridge",
                },
            },
        )
        collision = receive(21, allow_error=True)["error"]
        assert collision["code"] == -32602
        assert "already declared as a define" in collision["message"]

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 22,
                "method": "textDocument/references",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 3, "character": 3},
                    "context": {"includeDeclaration": True},
                },
            },
        )
        references = receive(22)["result"]
        assert len(references) == 2
        assert {location["range"]["start"]["line"] for location in references} == {1, 3}

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 23,
                "method": "textDocument/documentHighlight",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 12, "character": 7},
                },
            },
        )
        highlights = receive(23)["result"]
        assert len(highlights) == 2
        assert {highlight["kind"] for highlight in highlights} == {2, 3}

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 24,
                "method": "textDocument/codeAction",
                "params": {
                    "textDocument": {"uri": uri},
                    "range": {
                        "start": {"line": 4, "character": 0},
                        "end": {"line": 4, "character": 20},
                    },
                    "context": {"diagnostics": []},
                },
            },
        )
        actions = receive(24)["result"]
        assert any("preserve line numbering" in action["title"] for action in actions)

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 25,
                "method": "textDocument/semanticTokens/full",
                "params": {"textDocument": {"uri": uri}},
            },
        )
        semantic_tokens = receive(25)["result"]["data"]
        assert len(semantic_tokens) > 20

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 26,
                "method": "textDocument/foldingRange",
                "params": {"textDocument": {"uri": uri}},
            },
        )
        folding = receive(26)["result"]
        assert folding[0]["startLine"] == 1
        assert folding[0]["endLine"] >= 12

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 27,
                "method": "textDocument/inlayHint",
                "params": {
                    "textDocument": {"uri": uri},
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 20, "character": 0},
                    },
                },
            },
        )
        inlay_hints = receive(27)["result"]
        assert any(hint["label"] == " = -793837322" for hint in inlay_hints)

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 28,
                "method": "workspace/symbol",
                "params": {"query": "DOOR"},
            },
        )
        workspace_symbols = receive(28)["result"]
        assert len(workspace_symbols) == 1
        assert workspace_symbols[0]["name"] == "DOOR"

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 29,
                "method": "textDocument/formatting",
                "params": {
                    "textDocument": {"uri": uri},
                    "options": {"tabSize": 2, "insertSpaces": True},
                },
            },
        )
        formatting = receive(29)["result"]
        assert formatting and "  move r0 1" in formatting[0]["newText"]
        assert formatting[0]["newText"].count("# Wait for the supplier response.") == 1

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 36,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 16, "character": 11},
                },
            },
        )
        define_value_completion = receive(36)["result"]
        diode_completion = next(
            item
            for item in define_value_completion
            if item["label"] == "StructureDiode"
        )
        assert diode_completion["insertText"] == "1944485013"
        assert "LED" in diode_completion["filterText"]

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 37,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 17, "character": 7},
                },
            },
        )
        batch_completion = receive(37)["result"]
        batch_labels = {item["label"] for item in batch_completion}
        assert "On" in batch_labels
        assert "Color" in batch_labels
        assert "Power" not in batch_labels
        assert "RatioCarbonDioxideInput2" not in batch_labels

        for request_id, line, constant, expected in [
            (38, 18, "nan", "not a number"),
            (39, 19, "pinf", "positive infinite"),
            (40, 20, "ninf", "negative infinite"),
        ]:
            write_message(
                process.stdin,
                {
                    "jsonrpc": "2.0",
                    "id": request_id,
                    "method": "textDocument/hover",
                    "params": {
                        "textDocument": {"uri": uri},
                        "position": {"line": line, "character": 9},
                    },
                },
            )
            constant_hover = receive(request_id)["result"]["contents"]["value"]
            assert f"### `{constant}`" in constant_hover
            assert expected in constant_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 41,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 21, "character": 8},
                },
            },
        )
        connection_hover = receive(41)["result"]["contents"]["value"]
        assert "Device connection 1" in connection_hover
        assert "Channel0" in connection_hover

        # Multiple contexts are indexed but deliberately do not affect language
        # intelligence until the client makes an explicit visible selection.
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 30,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 14, "character": 3},
                },
            },
        )
        ambiguous_hover = receive(30)["result"]["contents"]["value"]
        assert "Outside Light" not in ambiguous_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "method": "ic10/selectContext",
                "params": {
                    "programUri": uri,
                    "scenarioUri": scenario_uri,
                    "icId": "main-ic",
                },
            },
        )
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 31,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 14, "character": 3},
                },
            },
        )
        selected_hover = receive(31)["result"]["contents"]["value"]
        assert "Outside Light" in selected_hover
        assert "StructureWallLight" in selected_hover
        assert "`0` → `data`" in selected_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 34,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 14, "character": 8},
                },
            },
        )
        environment_completion = receive(34)["result"]
        environment_labels = {item["label"] for item in environment_completion}
        assert "On" in environment_labels
        assert "Setting" not in environment_labels

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 42,
                "method": "textDocument/completion",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 15, "character": 11},
                },
            },
        )
        load_completion = receive(42)["result"]
        load_labels = {item["label"] for item in load_completion}
        assert "Power" in load_labels
        assert "RatioCarbonDioxideInput2" not in load_labels

        environment_diagnostics = next(
            notification["params"]["diagnostics"]
            for notification in reversed(observed_notifications)
            if notification.get("method") == "textDocument/publishDiagnostics"
            and notification["params"]["uri"] == uri
            and any(
                diagnostic.get("source") == "ic10 environment"
                for diagnostic in notification["params"]["diagnostics"]
            )
        )
        assert any(
            diagnostic.get("code") == "unsupported-prefab-logic-type"
            and diagnostic["range"]["start"]["line"] == 17
            and "StructureDiode" in diagnostic["message"]
            for diagnostic in environment_diagnostics
        )
        unsupported = next(
            diagnostic
            for diagnostic in environment_diagnostics
            if diagnostic.get("code") == "environment-unsupported-field"
            and diagnostic["range"]["start"]["line"] == 15
        )
        assert unsupported["data"]["deviceId"] == "light"
        assert unsupported["data"]["property"] == "fields.Setting"
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 35,
                "method": "textDocument/codeAction",
                "params": {
                    "textDocument": {"uri": uri},
                    "range": unsupported["range"],
                    "context": {"diagnostics": [unsupported]},
                },
            },
        )
        environment_actions = receive(35)["result"]
        assert any(
            action.get("command") == "ic10.openEnvironmentTarget"
            for action in environment_actions
        )
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "method": "textDocument/didChange",
                "params": {
                    "textDocument": {"uri": uri, "version": 2},
                    "contentChanges": [
                        {
                            "text": source.replace(
                                "sb LED RatioCarbonDioxideInput2 0.34",
                                "sb LED On 0.34",
                            )
                        }
                    ],
                },
            },
        )

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 32,
                "method": "textDocument/codeLens",
                "params": {"textDocument": {"uri": uri}},
            },
        )
        lenses = receive(32)["result"]
        assert len(lenses) == 2
        assert all(
            lens["command"]["command"] == "ic10.openEnvironmentTarget"
            for lens in lenses
        )

        # File-watch deletion invalidates the cache. With one context left it
        # becomes active automatically, without retaining the stale selection.
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "method": "ic10/scenarioChanged",
                "params": {"scenarioUri": scenario_uri, "version": 2},
            },
        )
        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 33,
                "method": "textDocument/hover",
                "params": {
                    "textDocument": {"uri": uri},
                    "position": {"line": 14, "character": 3},
                },
            },
        )
        refreshed_hover = receive(33)["result"]["contents"]["value"]
        assert "Backup Light" in refreshed_hover

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 30,
                "method": "ic10/build",
                "params": {
                    "uri": uri,
                    "options": {
                        "optimization": "compact",
                        "sourcePath": str(REPOSITORY_ROOT / "examples" / "smoke.ic10"),
                    },
                },
            },
        )
        deployment = receive(30)["result"]
        assert "#" not in deployment["code"]
        assert deployment["metadata"]["sourceSha256"]
        assert deployment["metadata"]["gameDataVersion"] == "0.2.6403.27689"
        assert deployment["sourceMap"][0]["sourceLine"] == 3
        assert deployment["report"]["generatedLines"] < deployment["report"]["sourceLines"]
        assert deployment["report"]["limits"][0]["value"] == 128
        assert deployment["report"]["limits"][1]["value"] is None

        write_message(
            process.stdin,
            {
                "jsonrpc": "2.0",
                "id": 31,
                "method": "ic10/build",
                "params": {
                    "uri": uri,
                    "options": {
                        "optimization": "readable",
                        "gameVersion": "future",
                    },
                },
            },
        )
        mismatch = receive(31, allow_error=True)["error"]
        assert mismatch["code"] == -32602
        assert "game-version-mismatch" in mismatch["message"]

        write_message(
            process.stdin,
            {"jsonrpc": "2.0", "id": 99, "method": "shutdown"},
        )
        receive(99)
        write_message(
            process.stdin,
            {"jsonrpc": "2.0", "method": "exit"},
        )
        process.stdin.close()
        process.wait(timeout=10)
        if process.returncode != 0:
            raise RuntimeError(f"LSP server exited with {process.returncode}")
        print(
            "LSP transport smoke test passed "
            "(initialize, document/environment intelligence, context transport, "
            "navigation, actions, invalidation, formatting, and deployment builds)."
        )
        return 0
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=5)


if __name__ == "__main__":
    import traceback
    try:
        raise SystemExit(main())
    except Exception as err:
        traceback.print_exc()
        raise SystemExit(1)
