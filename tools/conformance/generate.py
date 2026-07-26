"""Generate and verify the IC10 simulator conformance matrix and report."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INSTRUCTIONS = ROOT / "data/generated/instructions.json"
MANIFEST = ROOT / "data/conformance/manifest.json"
FIXTURES = ROOT / "data/conformance/fixtures.json"
MATRIX = ROOT / "data/generated/conformance.json"
REPORT = ROOT / "docs/simulator-compatibility.md"
STATUSES = ("supported", "partial", "unsupported", "unverified")


def load(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def build() -> tuple[str, str]:
    reference = load(INSTRUCTIONS)
    manifest = load(MANIFEST)
    fixture_catalog = load(FIXTURES)["fixtures"]
    for fixture_id, fixture in fixture_catalog.items():
        scenario = ROOT / fixture["scenario"]
        if not scenario.is_file():
            raise ValueError(f"{fixture_id} references missing scenario {fixture['scenario']}")
        if fixture.get("ticks", 0) < 1 or not fixture.get("expected"):
            raise ValueError(f"{fixture_id} is not an executable golden fixture")
    if manifest["gameVersion"] != reference["gameVersion"]:
        raise ValueError("conformance manifest gameVersion does not match instructions.json")

    unknown = sorted(set(manifest["overrides"]) - set(reference["instructions"]))
    if unknown:
        raise ValueError(f"conformance overrides name unknown instructions: {unknown}")

    entries = {}
    for name, instruction in sorted(reference["instructions"].items()):
        override = manifest["overrides"].get(name, {})
        status = override.get("status", manifest["defaults"]["status"])
        if status not in STATUSES:
            raise ValueError(f"{name} has invalid status {status!r}")
        fixtures = override.get("fixtures", [])
        missing = sorted(set(fixtures) - set(fixture_catalog))
        if missing:
            raise ValueError(f"{name} references unknown fixtures: {missing}")
        if status == "supported" and not fixtures:
            raise ValueError(f"supported instruction {name} has no execution fixture")
        deviations = override.get(
            "knownDeviations",
            [] if status == "supported"
            else manifest["defaults"].get("knownDeviations", []),
        )
        if status != "supported" and not deviations:
            raise ValueError(f"{status} instruction {name} must explain its limitations")
        entries[name] = {
            "status": status,
            "sourceGameVersion": reference["gameVersion"],
            "evidence": {
                **manifest["evidence"],
                "syntax": instruction["syntax"],
                "operands": instruction["operands"],
            },
            "fixtures": fixtures,
            "knownDeviations": deviations,
            "deviceBehaviours": override.get("deviceBehaviours", []),
        }

    matrix = {
        "schemaVersion": manifest["schemaVersion"],
        "gameVersion": reference["gameVersion"],
        "instructions": entries,
    }
    matrix_text = json.dumps(matrix, indent=2, ensure_ascii=False) + "\n"

    counts = {status: 0 for status in STATUSES}
    for entry in entries.values():
        counts[entry["status"]] += 1
    unsupported = [
        f"`{name}`" for name, entry in entries.items() if entry["status"] == "unsupported"
    ]
    active = sorted(
        {
            dependency
            for entry in entries.values()
            for dependency in entry["deviceBehaviours"]
        }
    )
    report = f"""# Simulator compatibility

This report is generated from `data/generated/instructions.json` and
`data/conformance/manifest.json`. Do not edit it by hand.

- Bundled Stationeers data: `{reference["gameVersion"]}`
- Generated instructions: {len(entries)}
- Supported with golden fixtures: {counts["supported"]}
- Partial: {counts["partial"]}
- Unverified: {counts["unverified"]}
- Unsupported: {counts["unsupported"]}
- Known unsupported instructions: {", ".join(unsupported) or "none"}

## Device behaviour boundary

The CPU and passive world model are deterministic. Device fields, slots,
memory, and cable channels are modelled as passive state. Active machine
physics remains outside the simulator unless explicitly listed in the matrix.

Dependencies currently called out by the matrix: {", ".join(active) or "none"}.

## Evidence and deviations

The machine-readable detail, including syntax, operand types, fixture IDs, and
known deviations, is in `data/generated/conformance.json`. `rol`, `ror`,
`clamp`, and `sgn` remain unverified where generated Stationpedia descriptions
do not define edge behaviour; see `examples/conformance/README.md` for the
minimal real-game capture workflow.
"""
    return matrix_text, report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    matrix, report = build()
    outputs = ((MATRIX, matrix), (REPORT, report))
    if args.check:
        stale = [str(path.relative_to(ROOT)) for path, text in outputs
                 if not path.exists() or path.read_text(encoding="utf-8") != text]
        if stale:
            raise SystemExit("stale conformance output: " + ", ".join(stale))
        return 0
    for path, text in outputs:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
