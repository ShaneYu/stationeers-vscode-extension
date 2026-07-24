from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from tools.stationpedia.generate import (
    GenerationError,
    apply_instruction_override,
    enum_member_value,
    load_dotenv,
    parse_operands,
    read_object,
    resource_kind,
)


class DotenvTests(unittest.TestCase):
    def test_loads_quoted_windows_path_without_overwriting_environment(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env_file = Path(directory) / ".env"
            env_file.write_text(
                'STATIONEERS_DIR="C:\\\\Program Files (x86)\\\\Stationeers"\nKEEP=new\n',
                encoding="utf-8",
            )
            environment = {"KEEP": "existing"}

            load_dotenv(env_file, environment)

            self.assertEqual(
                environment["STATIONEERS_DIR"],
                "C:\\\\Program Files (x86)\\\\Stationeers",
            )
            self.assertEqual(environment["KEEP"], "existing")

    def test_rejects_malformed_entry(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            env_file = Path(directory) / ".env"
            env_file.write_text("not-an-assignment\n", encoding="utf-8")
            with self.assertRaises(GenerationError):
                load_dotenv(env_file, {})


class TransformationTests(unittest.TestCase):
    def test_parses_named_and_unnamed_operands(self) -> None:
        self.assertEqual(
            parse_operands("add r? a(r?|num) b(r?|num)"),
            [
                {"label": "operand1", "type": "r?", "display": "r?"},
                {"label": "a", "type": "r?|num", "display": "a(r?|num)"},
                {"label": "b", "type": "r?|num", "display": "b(r?|num)"},
            ],
        )

    def test_applies_data_driven_instruction_corrections(self) -> None:
        syntax, description = apply_instruction_override(
            "s",
            "s device(d?|r?|id) logicType r?",
            "Stores register value.",
            {
                "s": {
                    "lastOperand": "value(r?|num)",
                    "descriptionReplacements": [
                        ["Stores register value", "Stores a register or numeric value"]
                    ],
                }
            },
        )
        self.assertEqual(syntax, "s device(d?|r?|id) logicType value(r?|num)")
        self.assertEqual(description, "Stores a register or numeric value.")

    def test_requires_top_level_json_object(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "data.json"
            path.write_text(json.dumps([]), encoding="utf-8")
            with self.assertRaises(GenerationError):
                read_object(path)

    def test_identifies_items_without_matching_unrelated_devices(self) -> None:
        self.assertEqual(
            resource_kind({"PrefabName": "ItemIronIngot", "Title": "Ingot (Iron)"}),
            "ingot",
        )
        self.assertEqual(
            resource_kind({"PrefabName": "ItemOxite", "Title": "Ice (Oxite)"}),
            "ice",
        )
        self.assertEqual(
            resource_kind(
                {
                    "PrefabName": "ItemIntegratedCircuit10",
                    "Title": "Integrated Circuit (IC10)",
                    "Item": {},
                }
            ),
            "item",
        )
        self.assertIsNone(
            resource_kind({"PrefabName": "DeviceStepUnit", "Title": "Device Step Unit"})
        )

    def test_resolves_runtime_item_class_values(self) -> None:
        enums = {
            "basicEnums": {
                "SlotClass": {
                    "values": {
                        "Ingot": {"value": 19},
                    }
                },
                "SortingClass": {
                    "values": {
                        "Resources": {"value": 3},
                    }
                },
            }
        }

        self.assertEqual(enum_member_value(enums, "SlotClass", "Ingot"), 19)
        self.assertEqual(
            enum_member_value(enums, "SortingClass", "Resources"),
            3,
        )
        self.assertIsNone(enum_member_value(enums, "SlotClass", None))
        with self.assertRaises(GenerationError):
            enum_member_value(enums, "SlotClass", "Unknown")


if __name__ == "__main__":
    unittest.main()
