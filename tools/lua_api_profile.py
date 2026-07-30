"""Generate and check the evidence-backed Stationeers Lua API profile."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tools" / "lua_api_profile.json"
ANNOTATION = ROOT / "packages/vscode/assets/lua/stationeers-v1/library.lua"
TOOLKIT_OVERLAY = ROOT / "packages/vscode/assets/lua/stationeers-toolkit/library.lua"
RUST_DECLARATIONS = ROOT / "crates/ic10-sim/src/generated/lua_api_profile.rs"
RUST_RUNTIME = ROOT / "crates/ic10-sim/src/lua.rs"
DOC_PROFILE = ROOT / "docs/live-integration/lua-simulator-profile.json"
DOCS = ROOT / "docs/live-integration/lua-simulator-profile.md"

TOOLKIT_OVERLAY_NAMES = {
    "device.get",
    "device.getReferenceId",
    "IcDevice:get",
    "IcDevice:set",
    "IcDevice:slot",
    "IcDevice:memory",
    "IcDevice:setMemory",
    "IcSlot:get",
    "IcSlot:set",
    "ic.get",
    "ic.set",
    "ic.read",
    "ic.write",
    "log",
}


def load() -> dict[str, Any]:
    profile = json.loads(SOURCE.read_text(encoding="utf-8"))
    names = [entry["name"] for entry in profile["supportedFunctions"]]
    if len(names) != len(set(names)):
        raise ValueError("supported function names must be unique")
    unsupported = [entry["name"] for entry in profile["unsupported"]]
    if set(names) & set(unsupported):
        raise ValueError("a function cannot be both supported and unsupported")
    return profile


def rust_names(profile: dict[str, Any]) -> list[str]:
    """Names used by the Rust registration surface, normalized to API paths."""
    source = RUST_RUNTIME.read_text(encoding="utf-8")
    required_literals = {"device", "getReferenceId", "ic", "setMemory", "slot", "memory", "print", "log"}
    missing = sorted(literal for literal in required_literals if f'"{literal}"' not in source)
    if missing:
        raise ValueError(f"runtime registration is missing literals: {', '.join(missing)}")
    # The profile's Rust name is generated from the same canonical names; this
    # guard catches accidental removal of a registration family in lua.rs.
    for name in ("device.get", "device.getReferenceId", "ic.get", "ic.set", "IcDevice:slot", "IcDevice:memory", "IcDevice:setMemory"):
        if name == "device.get" and 'device.set(' not in source:
            raise ValueError("device.get registration is missing")
        if name == "ic.get" and 'ic.set(' not in source:
            raise ValueError("ic.get registration is missing")
    return [entry["name"] for entry in profile["supportedFunctions"]]


def render_annotation(
    profile: dict[str, Any],
    entries: list[dict[str, Any]],
    include_stationeers_types: bool,
) -> str:
    lines = ["---@meta", "--- Generated from tools/lua_api_profile.json. Editor metadata only.", ""]
    lines += ["---@class IcSlot", "local IcSlot = {}", ""]
    lines += ["---@class IcDevice", "local IcDevice = {}", ""]
    if include_stationeers_types:
        lines += ["---@class StationeersDeviceInfo", "---@field ref_id number", "---@field prefab_hash number", "---@field name_hash number", "---@field display_name string", "local StationeersDeviceInfo = {}", ""]
        lines += ["---@class StationeersHostInfo", "---@field name string", "---@field ref_id number", "---@field prefab_hash number", "---@field type string", "---@field wearer string|nil", "local StationeersHostInfo = {}", ""]
        lines += ["---@class IcEnums", "---@field LogicType table<string, number>", "---@field LogicBatchMethod table<string, number>", "---@field LogicSlotType table<string, number>", "local IcEnums = {}", ""]
        lines += ["---@class IcDeviceApi", "---@field label fun(deviceIndex: number, name: string): nil", "---@field name fun(deviceIndex: number, networkIndex: number): string|nil", "local IcDeviceApi = {}", ""]
        lines += ["---@class Ic", "---@field enums IcEnums", "---@field device IcDeviceApi", "ic = {}", ""]
    else:
        lines += ["---@class ToolkitIc", "ic = ic or {}", ""]
    lines += [
        "---@class DeviceApi",
        "device = {}" if include_stationeers_types else "device = device or {}",
        "",
    ]
    for entry in entries:
        name = entry["name"]
        lua = entry["lua"]
        signature = lua[lua.index("(") + 1 : lua.rindex(")")].strip()
        for param in [part.strip() for part in signature.split(",") if part.strip() and part != "..."]:
            lines.append(f"---@param {param} {'string' if param in {'name', 'pin', 'field'} else 'number'}")
        if "..." in signature:
            lines.append("---@param ... any")
        lines.append(f"---@return {entry['returns']}")
        if ":" in name:
            owner, method = name.split(":", 1)
            lines.append(f"function {owner}:{method}({signature}) end")
        elif "." in name:
            owner, method = name.split(".", 1)
            lines.append(f"function {owner}.{method}({signature}) end")
        else:
            lines.append(f"function {name}({signature}) end")
        lines.append("")
    lines.append("return { device = device, ic = ic }")
    return "\n".join(lines) + "\n"


def render_rust(profile: dict[str, Any]) -> str:
    supported = [entry["name"] for entry in profile["supportedFunctions"]]
    unsupported = [entry["name"] for entry in profile["unsupported"]]
    def array(values: list[str]) -> str:
        return "[\n" + "".join(f'    "{value}",\n' for value in values) + "]"
    return "// @generated by tools/lua_api_profile.py; do not edit.\n\n" \
        + f'pub const PROFILE_ID: &str = "{profile["profileId"]}";\n' \
        + f'pub const API_PROFILE_ID: &str = "{profile["apiProfileId"]}";\n' \
        + f"pub const SUPPORTED_FUNCTIONS: &[&str] = &{array(supported)};\n" \
        + f"pub const UNSUPPORTED_CAPABILITIES: &[&str] = &{array(unsupported)};\n"


def render_doc_profile(profile: dict[str, Any]) -> str:
    return json.dumps(profile, indent=2) + "\n"


def render_docs(profile: dict[str, Any]) -> str:
    existing = DOCS.read_text(encoding="utf-8")
    start = "<!-- BEGIN GENERATED API PROFILE -->"
    end = "<!-- END GENERATED API PROFILE -->"
    table = [start, "", "## Generated core host API", "", "| Function | Status | Evidence |", "| --- | --- | --- |"]
    for entry in profile["supportedFunctions"]:
        table.append(f"| `{entry['lua'].strip()}` | `{entry['status']}` | {', '.join(f'`{item}`' for item in entry['evidence'])} |")
    table += ["", "### Explicitly unsupported", "", "| Capability | Status | Reason |", "| --- | --- | --- |"]
    for entry in profile["unsupported"]:
        table.append(f"| `{entry['name']}` | `{entry['status']}` | {entry['reason']} |")
    table += ["", end, ""]
    block = "\n".join(table)
    if start in existing and end in existing:
        existing = re.sub(re.escape(start) + r".*?" + re.escape(end) + r"\n?", block, existing, flags=re.S)
        return existing
    return existing.rstrip() + "\n\n" + block


def generated() -> dict[Path, str]:
    profile = load()
    rust_names(profile)
    return {
        ANNOTATION: render_annotation(
            profile,
            profile["supportedFunctions"],
            include_stationeers_types=True,
        ),
        TOOLKIT_OVERLAY: render_annotation(
            profile,
            [entry for entry in profile["supportedFunctions"] if entry["name"] in TOOLKIT_OVERLAY_NAMES],
            include_stationeers_types=False,
        ),
        RUST_DECLARATIONS: render_rust(profile),
        DOC_PROFILE: render_doc_profile(profile),
        DOCS: render_docs(profile),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail when generated files are stale")
    args = parser.parse_args()
    outputs = generated()
    stale = []
    for path, content in outputs.items():
        if args.check:
            if not path.exists() or path.read_text(encoding="utf-8") != content:
                stale.append(str(path.relative_to(ROOT)))
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding="utf-8", newline="\n")
    if stale:
        print("stale generated Lua profile files:")
        print("\n".join(f"- {path}" for path in stale))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
