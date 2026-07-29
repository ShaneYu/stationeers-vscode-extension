from __future__ import annotations

import json
import unittest
from pathlib import Path

from tools.lua_api_profile import SOURCE, generated, load


class LuaApiProfileTests(unittest.TestCase):
    def test_source_has_evidence_for_every_supported_function(self) -> None:
        profile = load()
        for function in profile["supportedFunctions"]:
            self.assertTrue(function["evidence"], function["name"])
            self.assertIn(function["status"], {"verified", "documented-only", "deviates"})

    def test_unsupported_entries_are_explicit(self) -> None:
        profile = load()
        self.assertTrue(profile["unsupported"])
        self.assertTrue(all(entry["status"] == "unsupported" for entry in profile["unsupported"]))
        self.assertTrue(all("reason" in entry for entry in profile["unsupported"]))

    def test_checked_in_outputs_are_deterministic(self) -> None:
        for path, content in generated().items():
            self.assertEqual(path.read_text(encoding="utf-8"), content, path)

    def test_source_is_machine_readable_json(self) -> None:
        self.assertIsInstance(json.loads(SOURCE.read_text(encoding="utf-8")), dict)


if __name__ == "__main__":
    unittest.main()
