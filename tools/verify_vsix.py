"""Validate the security- and platform-sensitive contents of a built VSIX."""

from __future__ import annotations

import glob
import json
import pathlib
import struct
import sys
import zipfile


SUPPORTED_TARGETS = {
    "darwin-arm64",
    "darwin-x64",
    "linux-arm64",
    "linux-x64",
    "win32-arm64",
    "win32-x64",
}
VSCODE_ENGINE = "^1.107.0"


def fail(message: str) -> None:
    raise SystemExit(f"VSIX verification failed: {message}")


def resolve_vsix(pattern: str) -> pathlib.Path:
    matches = [pathlib.Path(item) for item in glob.glob(pattern)]
    if len(matches) != 1:
        fail(f"expected one file matching {pattern!r}, found {len(matches)}")
    return matches[0]


def main() -> None:
    if len(sys.argv) != 3:
        fail("usage: verify_vsix.py <vsix path-or-glob> <target>")

    vsix_path = resolve_vsix(sys.argv[1])
    target = sys.argv[2]
    if target not in SUPPORTED_TARGETS:
        fail(f"unsupported target {target!r}")
    if f"@{target}.vsix" not in vsix_path.name:
        fail(f"{vsix_path.name!r} does not identify target {target!r}")

    suffix = ".exe" if target.startswith("win32-") else ""
    expected_servers = {
        f"extension/server/{target}/ic10-lsp{suffix}",
        f"extension/server/{target}/ic10-dap{suffix}",
    }
    required_files = {
        "extension/changelog.md",
        "extension/LICENSE.txt",
        "extension/readme.md",
        "extension/SUPPORT.md",
        "extension/THIRD_PARTY_NOTICES.md",
        "extension/assets/icon.png",
        "extension/assets/devices/StructureChuteStraight.png",
        "extension/assets/devices/ItemCableCoil.png",
        "extension/assets/devices/ItemKitPipe.png",
        "extension/assets/devices/ItemKitPipeLiquid.png",
        "extension/dist/extension.js",
        "extension/package.json",
        "extension/reference/devices.json",
        "extension/reference/instructions.json",
        "extension/reference/resources.json",
        "extension/schemas/ic10sim.schema.json",
        *expected_servers,
    }

    with zipfile.ZipFile(vsix_path) as archive:
        names = set(archive.namelist())
        missing = sorted(required_files - names)
        if missing:
            fail(f"missing required files: {', '.join(missing)}")

        manifest = json.loads(archive.read("extension/package.json"))
        if manifest.get("publisher") != "shaneyu":
            fail("unexpected publisher in extension/package.json")
        if manifest.get("name") != "stationeers":
            fail("unexpected extension name in extension/package.json")
        if manifest.get("engines", {}).get("vscode") != VSCODE_ENGINE:
            fail(
                "unexpected VS Code compatibility baseline; "
                f"expected {VSCODE_ENGINE!r}"
            )
        expected_name = (
            f"{manifest['name']}-{manifest.get('version', 'missing')}@{target}.vsix"
        )
        if vsix_path.name != expected_name:
            fail(f"expected package filename {expected_name!r}")

        bundled_servers = sorted(
            name
            for name in names
            if name.startswith("extension/server/") and not name.endswith("/")
        )
        if set(bundled_servers) != expected_servers:
            fail(
                "platform package must contain exactly its native LSP and DAP binaries; "
                f"found {bundled_servers}"
            )

        forbidden = sorted(
            name
            for name in names
            if name.endswith((".env", ".map", ".ts"))
            or "/.git/" in name
            or "/node_modules/" in name
        )
        if forbidden:
            fail(f"forbidden development files found: {', '.join(forbidden)}")

        icon = archive.read("extension/assets/icon.png")
        if icon[:8] != b"\x89PNG\r\n\x1a\n" or len(icon) < 24:
            fail("extension icon is not a valid PNG")
        width, height = struct.unpack(">II", icon[16:24])
        if width < 128 or height < 128:
            fail(f"extension icon is too small: {width}x{height}")

    print(f"Verified {vsix_path} for {target}.")


if __name__ == "__main__":
    main()
