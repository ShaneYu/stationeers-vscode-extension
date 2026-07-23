#!/usr/bin/env python3
"""Generate the immutable IC10 reference data bundled with the language server.

The script uses only Python's standard library. It reads ``STATIONEERS_DIR`` from
the process environment or the repository-root ``.env`` file. The configured
path may point at the game installation or directly at the Stationpedia export.
"""

from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
import sys
from collections.abc import Iterable, Mapping
from pathlib import Path
from typing import Any

REPOSITORY_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_ENV_FILE = REPOSITORY_ROOT / ".env"
DEFAULT_OUTPUT_DIR = REPOSITORY_ROOT / "data" / "generated"
DEFAULT_ASSETS_DIR = REPOSITORY_ROOT / "packages" / "vscode" / "assets" / "devices"
DEFAULT_GRAMMAR_FILE = (
    REPOSITORY_ROOT / "packages" / "vscode" / "syntaxes" / "ic10.tmLanguage.json"
)
OVERRIDES_FILE = Path(__file__).with_name("overrides.json")
SCHEMA_VERSION = 1

TAG_RE = re.compile(r"<[^>]+>")
BREAK_RE = re.compile(r"<br\s*/?>", re.IGNORECASE)
WHITESPACE_RE = re.compile(r"\s+")
OPERAND_RE = re.compile(r"^(?P<label>[A-Za-z][A-Za-z0-9_?]*)\((?P<kind>.+)\)$")

DEVICE_INSTRUCTIONS = {
    "l",
    "s",
    "ls",
    "lr",
    "sb",
    "lb",
    "lbs",
    "lbn",
    "sbn",
    "lbns",
    "ss",
    "sbs",
    "ld",
    "sd",
    "rmap",
    "bdse",
    "bdns",
    "brdse",
    "brdns",
    "bdseal",
    "bdnsal",
    "bdnvl",
    "bdnvs",
}
SELECTION_INSTRUCTIONS = {
    "sdse",
    "sdns",
    "slt",
    "sgt",
    "sle",
    "sge",
    "seq",
    "sne",
    "sap",
    "sna",
    "sltz",
    "sgtz",
    "slez",
    "sgez",
    "seqz",
    "snez",
    "sapz",
    "snaz",
    "snan",
    "snanz",
    "select",
}
MATH_INSTRUCTIONS = {
    "add",
    "sub",
    "mul",
    "div",
    "mod",
    "sqrt",
    "round",
    "trunc",
    "ceil",
    "floor",
    "max",
    "min",
    "abs",
    "log",
    "exp",
    "rand",
    "sin",
    "asin",
    "tan",
    "atan",
    "cos",
    "acos",
    "atan2",
    "pow",
    "lerp",
    "sgn",
    "clamp",
}
BITWISE_INSTRUCTIONS = {
    "and",
    "or",
    "xor",
    "nor",
    "not",
    "srl",
    "sra",
    "sll",
    "sla",
    "ext",
    "ins",
    "rol",
    "ror",
}
MEMORY_INSTRUCTIONS = {
    "peek",
    "push",
    "pop",
    "poke",
    "get",
    "put",
    "getd",
    "putd",
    "clr",
    "clrd",
}
UTILITY_INSTRUCTIONS = {"alias", "define", "move", "yield", "sleep", "hcf", "label"}
ACCESS = {
    "Read": {"read": True, "write": False},
    "Write": {"read": False, "write": True},
    "ReadWrite": {"read": True, "write": True},
}


class GenerationError(RuntimeError):
    """An export or override cannot be transformed without losing correctness."""


def parse_arguments(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stationeers-dir", type=Path)
    parser.add_argument("--env-file", type=Path, default=DEFAULT_ENV_FILE)
    parser.add_argument("--output-dir", type=Path, default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--assets-dir", type=Path, default=DEFAULT_ASSETS_DIR)
    parser.add_argument("--grammar-file", type=Path, default=DEFAULT_GRAMMAR_FILE)
    parser.add_argument(
        "--no-assets",
        action="store_true",
        help="Generate JSON and grammar without copying Stationpedia thumbnails.",
    )
    return parser.parse_args(argv)


def load_dotenv(path: Path, environ: dict[str, str] | None = None) -> dict[str, str]:
    """Load a deliberately small, predictable subset of dotenv syntax."""
    target = os.environ if environ is None else environ
    if not path.is_file():
        return target

    for line_number, raw_line in enumerate(
        path.read_text(encoding="utf-8-sig").splitlines(), start=1
    ):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        if line.startswith("export "):
            line = line.removeprefix("export ").lstrip()
        if "=" not in line:
            raise GenerationError(f"Invalid .env entry at {path}:{line_number}")
        key, value = (part.strip() for part in line.split("=", 1))
        if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key):
            raise GenerationError(f"Invalid .env key at {path}:{line_number}")
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {'"', "'"}:
            value = value[1:-1]
        else:
            value = value.split(" #", 1)[0].rstrip()
        target.setdefault(key, value)
    return target


def resolve_export_directory(configured_path: Path) -> Path:
    path = configured_path.expanduser().resolve()
    for candidate in (path / "Stationpedia", path):
        if all((candidate / filename).is_file() for filename in ("stationpedia.json", "enums.json")):
            return candidate
    raise GenerationError(
        "STATIONEERS_DIR must point to a Stationeers installation or a Stationpedia "
        f"export containing stationpedia.json and enums.json: {path}"
    )


def read_object(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8-sig"))
    except (OSError, json.JSONDecodeError) as error:
        raise GenerationError(f"Could not read {path}: {error}") from error
    if not isinstance(value, dict):
        raise GenerationError(f"Expected a JSON object in {path}")
    return value


def clean_text(value: Any) -> str:
    text = BREAK_RE.sub("\n", "" if value is None else str(value))
    text = TAG_RE.sub("", text)
    return WHITESPACE_RE.sub(" ", html.unescape(text)).strip()


def clean_syntax(value: Any) -> str:
    return clean_text(value)


def as_list(value: Any) -> list[Any]:
    if value is None:
        return []
    return value if isinstance(value, list) else [value]


def instruction_category(name: str) -> str:
    if name in DEVICE_INSTRUCTIONS:
        return "device"
    if name in SELECTION_INSTRUCTIONS:
        return "selection"
    if name in MATH_INSTRUCTIONS:
        return "mathematics"
    if name in BITWISE_INSTRUCTIONS:
        return "bitwise"
    if name in MEMORY_INSTRUCTIONS:
        return "memory"
    if name in UTILITY_INSTRUCTIONS:
        return "utility"
    if name in {"j", "jal", "jr"} or name.startswith("b"):
        return "flowControl"
    raise GenerationError(f"No category rule exists for instruction {name!r}")


def apply_instruction_override(
    name: str, syntax: str, description: str, overrides: Mapping[str, Any]
) -> tuple[str, str]:
    override = overrides.get(name, {})
    if not isinstance(override, dict):
        raise GenerationError(f"Instruction override for {name!r} is not an object")
    last_operand = override.get("lastOperand")
    if last_operand:
        parts = syntax.split()
        if len(parts) < 2:
            raise GenerationError(f"Cannot replace the final operand of {name!r}")
        parts[-1] = str(last_operand)
        syntax = " ".join(parts)
    for replacement in override.get("descriptionReplacements", []):
        if not isinstance(replacement, list) or len(replacement) != 2:
            raise GenerationError(f"Malformed description replacement for {name!r}")
        description = description.replace(str(replacement[0]), str(replacement[1]))
    return syntax, description


def parse_operands(syntax: str) -> list[dict[str, str]]:
    parts = syntax.split()
    operands: list[dict[str, str]] = []
    for index, token in enumerate(parts[1:], start=1):
        match = OPERAND_RE.match(token)
        if match:
            label = match.group("label")
            kind = match.group("kind")
        else:
            label = f"operand{index}"
            kind = token
        operands.append({"label": label, "type": kind, "display": token})
    return operands


def transform_enum(listing: Any, context: str) -> dict[str, Any]:
    if not isinstance(listing, dict) or not isinstance(listing.get("values"), dict):
        raise GenerationError(f"Malformed enum {context!r}")
    values: dict[str, Any] = {}
    for name, raw in sorted(listing["values"].items()):
        if not isinstance(raw, dict):
            raise GenerationError(f"Malformed value {context}.{name}")
        values[name] = {
            "value": raw.get("value"),
            "deprecated": bool(raw.get("deprecated", False)),
            "description": clean_text(raw.get("description")),
        }
    return {"displayName": listing.get("enumName", context), "values": values}


def transform_instructions(
    stationpedia: Mapping[str, Any],
    enums: Mapping[str, Any],
    overrides: Mapping[str, Any],
) -> dict[str, Any]:
    source_commands = stationpedia.get("scriptCommands")
    source_constants = stationpedia.get("scriptConstants")
    source_enums = enums.get("scriptEnums")
    if not isinstance(source_commands, dict) or not source_commands:
        raise GenerationError("stationpedia.json contains no scriptCommands object")
    if not isinstance(source_constants, dict):
        raise GenerationError("stationpedia.json contains no scriptConstants object")
    if not isinstance(source_enums, dict):
        raise GenerationError("enums.json contains no scriptEnums object")

    commands: dict[str, Any] = {}
    for name, raw in sorted(source_commands.items()):
        if not isinstance(raw, dict):
            raise GenerationError(f"Malformed instruction {name!r}")
        syntax, description = apply_instruction_override(
            name,
            clean_syntax(raw.get("example")),
            clean_text(raw.get("desc")),
            overrides,
        )
        commands[name] = {
            "category": instruction_category(name),
            "syntax": syntax,
            "description": description,
            "deprecated": name == "label" or description.casefold() == "deprecated",
            "operands": parse_operands(syntax),
        }

    constants: dict[str, Any] = {}
    for name, raw in sorted(source_constants.items()):
        if not isinstance(raw, dict):
            raise GenerationError(f"Malformed constant {name!r}")
        constants[name] = {
            "value": raw.get("value"),
            "description": clean_text(raw.get("desc")),
        }

    return {
        "schemaVersion": SCHEMA_VERSION,
        "gameVersion": stationpedia.get("version"),
        "architecture": {
            "numericStorage": "IEEE 754 double",
            "generalRegisters": "r0-r15",
            "returnAddressRegister": "ra",
            "stackPointerRegister": "sp",
            "stackSize": 512,
            "devicePins": "d0-d5",
            "baseDevice": "db",
            "maximumProgramLines": 128,
            "maximumInstructionsPerTick": 128,
            "tickSeconds": 0.5,
        },
        "instructions": commands,
        "constants": constants,
        "enums": {
            name: transform_enum(listing, name)
            for name, listing in sorted(source_enums.items())
        },
    }


def access_flags(raw: Any, context: str) -> dict[str, bool]:
    try:
        return dict(ACCESS[str(raw)])
    except KeyError as error:
        raise GenerationError(f"Unknown logic access {raw!r} at {context}") from error


def transform_slots(page: Mapping[str, Any]) -> dict[str, Any]:
    inserts: dict[int, Mapping[str, Any]] = {}
    for raw in as_list(page.get("SlotInserts")):
        if isinstance(raw, dict):
            try:
                inserts[int(raw.get("SlotIndex"))] = raw
            except (TypeError, ValueError):
                continue

    slots = [item for item in as_list(page.get("Slots")) if isinstance(item, dict)]
    logic_info = page.get("LogicInfo")
    logic_slots = logic_info.get("LogicSlotTypes", {}) if isinstance(logic_info, dict) else {}
    if not isinstance(logic_slots, dict):
        logic_slots = {}
    indices = set(inserts) | set(range(len(slots)))
    for raw_index in logic_slots:
        try:
            indices.add(int(raw_index))
        except ValueError as error:
            raise GenerationError(
                f"Invalid slot index {raw_index!r} on {page.get('PrefabName')}"
            ) from error

    result: dict[str, Any] = {}
    for index in sorted(indices):
        insert = inserts.get(index, {})
        slot = slots[index] if index < len(slots) else {}
        raw_logic = logic_slots.get(str(index), {})
        result[str(index)] = {
            "name": clean_text(insert.get("SlotName") or slot.get("SlotName")),
            "type": clean_text(insert.get("SlotType")),
            "class": slot.get("SlotClass"),
            "hash": slot.get("StringHash"),
            "logicTypes": {
                name: access_flags(access, f"{page.get('PrefabName')} slot {index}.{name}")
                for name, access in sorted(raw_logic.items())
            }
            if isinstance(raw_logic, dict)
            else {},
        }
    return result


def transform_device(page: Mapping[str, Any], textures_dir: Path) -> dict[str, Any]:
    prefab_name = str(page["PrefabName"])
    logic_info = page.get("LogicInfo")
    raw_logic = logic_info.get("LogicTypes", {}) if isinstance(logic_info, dict) else {}
    if not isinstance(raw_logic, dict):
        raise GenerationError(f"Malformed LogicTypes on {prefab_name}")
    image_name = f"{prefab_name}.png"
    image = image_name if (textures_dir / image_name).is_file() else None

    modes: dict[str, Any] = {}
    for entry in as_list(page.get("ModeInsert")):
        if not isinstance(entry, dict):
            continue
        name = clean_text(entry.get("LogicName"))
        if name:
            raw_value = entry.get("LogicAccessTypes")
            try:
                modes[name] = int(raw_value)
            except (TypeError, ValueError):
                modes[name] = raw_value

    connections = []
    device = page.get("Device")
    if isinstance(device, dict):
        for entry in as_list(device.get("ConnectionList")):
            if isinstance(entry, list) and len(entry) >= 2:
                connections.append({"type": entry[0], "role": entry[1]})

    result: dict[str, Any] = {
        "prefabName": prefab_name,
        "prefabHash": page.get("PrefabHash"),
        "displayName": clean_text(page.get("Title")) or prefab_name,
        "description": clean_text(page.get("Description")),
        "image": image,
        "logicTypes": {
            name: access_flags(access, f"{prefab_name}.{name}")
            for name, access in sorted(raw_logic.items())
        },
        "slots": transform_slots(page),
        "modes": modes,
        "connections": connections,
    }
    memory = page.get("Memory")
    if isinstance(memory, dict):
        result["memory"] = {
            "size": memory.get("MemorySize"),
            "access": memory.get("MemoryAccess"),
        }
    return result


def transform_devices(stationpedia: Mapping[str, Any], textures_dir: Path) -> dict[str, Any]:
    pages = stationpedia.get("pages")
    if not isinstance(pages, list):
        raise GenerationError("stationpedia.json contains no pages array")

    devices: dict[str, Any] = {}
    other_logicables: dict[str, Any] = {}
    hashes: dict[int, str] = {}
    for page in pages:
        if not isinstance(page, dict) or not isinstance(page.get("LogicInfo"), dict):
            continue
        logic_info = page["LogicInfo"]
        if not logic_info.get("LogicTypes") and not logic_info.get("LogicSlotTypes"):
            continue
        name = page.get("PrefabName")
        if not isinstance(name, str) or not name:
            raise GenerationError("A logic-capable Stationpedia page has no PrefabName")
        prefab_hash = page.get("PrefabHash")
        if isinstance(prefab_hash, int):
            previous = hashes.get(prefab_hash)
            if previous is not None and previous != name:
                raise GenerationError(
                    f"Prefab hash collision {prefab_hash}: {previous!r} and {name!r}"
                )
            hashes[prefab_hash] = name
        transformed = transform_device(page, textures_dir)
        target = devices if isinstance(page.get("Device"), dict) else other_logicables
        if name in target:
            raise GenerationError(f"Duplicate PrefabName {name!r}")
        target[name] = transformed

    return {
        "schemaVersion": SCHEMA_VERSION,
        "gameVersion": stationpedia.get("version"),
        "devices": dict(sorted(devices.items())),
        "otherLogicables": dict(sorted(other_logicables.items())),
    }


def resource_kind(page: Mapping[str, Any]) -> str | None:
    prefab_name = page.get("PrefabName")
    title = clean_text(page.get("Title"))
    if isinstance(prefab_name, str) and prefab_name.endswith("Ingot"):
        return "ingot"
    if title.startswith("Ice ("):
        return "ice"
    return None


def transform_resources(stationpedia: Mapping[str, Any], textures_dir: Path) -> dict[str, Any]:
    pages = stationpedia.get("pages")
    source_reagents = stationpedia.get("reagents")
    if not isinstance(pages, list):
        raise GenerationError("stationpedia.json contains no pages array")
    if not isinstance(source_reagents, dict):
        raise GenerationError("stationpedia.json contains no reagents object")

    resources: dict[str, Any] = {}
    for page in pages:
        if not isinstance(page, dict):
            continue
        kind = resource_kind(page)
        if kind is None:
            continue
        prefab_name = page.get("PrefabName")
        prefab_hash = page.get("PrefabHash")
        item = page.get("Item")
        if not isinstance(prefab_name, str) or not isinstance(prefab_hash, int):
            raise GenerationError("An ingot or ice page has invalid prefab identity")
        if not isinstance(item, dict):
            raise GenerationError(f"Resource page {prefab_name!r} contains no Item object")
        image_name = f"{prefab_name}.png"
        resources[prefab_name] = {
            "prefabName": prefab_name,
            "prefabHash": prefab_hash,
            "displayName": clean_text(page.get("Title")) or prefab_name,
            "description": clean_text(page.get("Description")),
            "image": image_name if (textures_dir / image_name).is_file() else None,
            "kind": kind,
            "slotClass": item.get("SlotClass"),
            "sortingClass": item.get("SortingClass"),
            "maxQuantity": item.get("MaxQuantity"),
            "reagents": dict(sorted(item.get("Reagents", {}).items()))
            if isinstance(item.get("Reagents"), dict)
            else {},
            "gases": [
                {
                    "type": gas.get("Type"),
                    "quantity": gas.get("Quantity"),
                    "temperature": gas.get("Temperature"),
                }
                for gas in as_list(item.get("Gases"))
                if isinstance(gas, dict)
            ],
        }

    reagents: dict[str, Any] = {}
    for name, raw in sorted(source_reagents.items()):
        if not isinstance(raw, dict):
            raise GenerationError(f"Malformed reagent {name!r}")
        sources = raw.get("Sources", {})
        if not isinstance(sources, dict):
            raise GenerationError(f"Malformed reagent sources for {name!r}")
        reagents[name] = {
            "name": name,
            "id": raw.get("Id"),
            "hash": raw.get("Hash"),
            "unit": raw.get("Unit"),
            "isOrganic": bool(raw.get("IsOrganic", False)),
            "sources": dict(sorted(sources.items())),
        }

    return {
        "schemaVersion": SCHEMA_VERSION,
        "gameVersion": stationpedia.get("version"),
        "resources": dict(sorted(resources.items())),
        "reagents": reagents,
    }


def regex_alternation(values: Iterable[str]) -> str:
    return "|".join(re.escape(value) for value in sorted(values, key=lambda item: (-len(item), item)))


def build_textmate_grammar(instructions: Mapping[str, Any]) -> dict[str, Any]:
    command_names = instructions["instructions"].keys()
    constant_names = instructions["constants"].keys()
    enum_names = {
        value_name
        for enum in instructions["enums"].values()
        for value_name in enum["values"]
    }
    return {
        "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
        "name": "Stationeers IC10",
        "scopeName": "source.ic10",
        "patterns": [
            {"include": "#comments"},
            {"include": "#labels"},
            {"include": "#macros"},
            {"include": "#instructions"},
            {"include": "#registers"},
            {"include": "#deviceReferences"},
            {"include": "#constants"},
            {"include": "#enumValues"},
            {"include": "#numbers"},
        ],
        "repository": {
            "comments": {"patterns": [{"name": "comment.line.number-sign.ic10", "match": "#.*$"}]},
            "labels": {
                "patterns": [
                    {
                        "match": "^\\s*([A-Za-z_][A-Za-z0-9_]*)(:)",
                        "captures": {
                            "1": {"name": "entity.name.label.ic10"},
                            "2": {"name": "punctuation.separator.label.ic10"},
                        },
                    }
                ]
            },
            "macros": {
                "patterns": [
                    {
                        "begin": "\\b(HASH|STR)(\\()",
                        "beginCaptures": {
                            "1": {"name": "support.function.macro.ic10"},
                            "2": {"name": "punctuation.section.parens.begin.ic10"},
                        },
                        "end": "(\\))",
                        "endCaptures": {
                            "1": {"name": "punctuation.section.parens.end.ic10"}
                        },
                        "patterns": [
                            {
                                "name": "string.quoted.double.ic10",
                                "begin": "\"",
                                "end": "\"",
                                "patterns": [
                                    {
                                        "name": "constant.character.escape.ic10",
                                        "match": "\\\\.",
                                    }
                                ],
                            }
                        ],
                    }
                ]
            },
            "instructions": {
                "patterns": [
                    {
                        "name": "keyword.control.instruction.ic10",
                        "match": f"(?i)(?<![A-Za-z0-9_])(?:{regex_alternation(command_names)})(?![A-Za-z0-9_])",
                    }
                ]
            },
            "registers": {
                "patterns": [
                    {
                        "name": "variable.language.register.ic10",
                        "match": "(?i)\\b(?:r(?:1[0-5]|[0-9])|ra|sp|rr(?:1[0-5]|[0-9]))\\b",
                    }
                ]
            },
            "deviceReferences": {
                "patterns": [
                    {
                        "name": "variable.language.device.ic10",
                        "match": "(?i)\\b(?:d[0-5]|db|dr(?:1[0-5]|[0-9]))\\b",
                    }
                ]
            },
            "constants": {
                "patterns": [
                    {
                        "name": "support.constant.ic10",
                        "match": f"\\b(?:{regex_alternation(constant_names)})\\b",
                    }
                ]
            },
            "enumValues": {
                "patterns": [
                    {
                        "name": "constant.language.enum.ic10",
                        "match": f"\\b(?:{regex_alternation(enum_names)})\\b",
                    }
                ]
            },
            "numbers": {
                "patterns": [
                    {
                        "name": "constant.numeric.hex.ic10",
                        "match": "(?<![A-Za-z0-9_])\\$[0-9A-Fa-f](?:_?[0-9A-Fa-f])*",
                    },
                    {
                        "name": "constant.numeric.binary.ic10",
                        "match": "(?<![A-Za-z0-9_])%[01](?:_?[01])*",
                    },
                    {
                        "name": "constant.numeric.decimal.ic10",
                        "match": "(?<![A-Za-z0-9_])[+-]?(?:\\d+(?:\\.\\d*)?|\\.\\d+)(?:[eE][+-]?\\d+)?",
                    },
                ]
            },
        },
    }


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(
        json.dumps(value, ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
        newline="\n",
    )
    temporary.replace(path)


def sync_assets(
    devices: Mapping[str, Any],
    resources: Mapping[str, Any],
    textures_dir: Path,
    assets_dir: Path,
) -> tuple[int, list[str]]:
    assets_dir.mkdir(parents=True, exist_ok=True)
    expected: set[str] = set()
    missing: list[str] = []
    entries = (
        list(devices["devices"].values())
        + list(devices["otherLogicables"].values())
        + list(resources["resources"].values())
    )
    for entry in entries:
        image = entry.get("image")
        if not image:
            missing.append(entry["prefabName"])
            continue
        expected.add(image)
        shutil.copy2(textures_dir / image, assets_dir / image)
    for existing in assets_dir.glob("*.png"):
        if existing.name not in expected:
            existing.unlink()
    return len(expected), missing


def main(argv: list[str] | None = None) -> int:
    args = parse_arguments(sys.argv[1:] if argv is None else argv)
    try:
        load_dotenv(args.env_file.resolve())
        configured = args.stationeers_dir or os.environ.get("STATIONEERS_DIR")
        if configured is None:
            raise GenerationError(
                "STATIONEERS_DIR is not set. Copy .env.example to .env, set the "
                "environment variable, or pass --stationeers-dir."
            )
        export_dir = resolve_export_directory(Path(configured))
        stationpedia = read_object(export_dir / "stationpedia.json")
        enums = read_object(export_dir / "enums.json")
        override_root = read_object(OVERRIDES_FILE)
        if override_root.get("schemaVersion") != SCHEMA_VERSION:
            raise GenerationError("Unsupported override schema version")
        instruction_overrides = override_root.get("instructions", {})
        if not isinstance(instruction_overrides, dict):
            raise GenerationError("The instructions override must be an object")

        instructions = transform_instructions(stationpedia, enums, instruction_overrides)
        devices = transform_devices(stationpedia, export_dir / "Textures")
        resources = transform_resources(stationpedia, export_dir / "Textures")
        output_dir = args.output_dir.resolve()
        write_json(output_dir / "instructions.json", instructions)
        write_json(output_dir / "devices.json", devices)
        write_json(output_dir / "resources.json", resources)
        write_json(args.grammar_file.resolve(), build_textmate_grammar(instructions))

        image_count = 0
        missing: list[str] = []
        if not args.no_assets:
            image_count, missing = sync_assets(
                devices,
                resources,
                export_dir / "Textures",
                args.assets_dir.resolve(),
            )
        manifest = {
            "schemaVersion": SCHEMA_VERSION,
            "gameVersion": stationpedia.get("version"),
            "instructionCount": len(instructions["instructions"]),
            "deviceCount": len(devices["devices"]),
            "otherLogicableCount": len(devices["otherLogicables"]),
            "resourceCount": len(resources["resources"]),
            "reagentCount": len(resources["reagents"]),
            "imageCount": image_count,
            "missingImages": missing,
        }
        write_json(output_dir / "manifest.json", manifest)
        print(
            f"Generated {manifest['instructionCount']} instructions, "
            f"{manifest['deviceCount']} devices, "
            f"{manifest['otherLogicableCount']} other logicables, and "
            f"{manifest['resourceCount']} resources, "
            f"{manifest['reagentCount']} reagents, and "
            f"{manifest['imageCount']} images for Stationeers {manifest['gameVersion']}."
        )
        if missing:
            print(f"Missing images: {', '.join(missing)}", file=sys.stderr)
        return 0
    except GenerationError as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
