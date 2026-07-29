import tempfile
import unittest
import zipfile
from pathlib import Path
import importlib.util

_MODULE_PATH = Path(__file__).with_name("verify_reproducible.py")
_SPEC = importlib.util.spec_from_file_location("verify_reproducible", _MODULE_PATH)
assert _SPEC and _SPEC.loader
_MODULE = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(_MODULE)
digest = _MODULE.digest


class ReproducibilityTests(unittest.TestCase):
    def test_archive_metadata_is_ignored_but_content_is_not(self):
        with tempfile.TemporaryDirectory() as directory:
            first = Path(directory) / "first.zip"
            second = Path(directory) / "second.zip"
            for target, timestamp in ((first, (2020, 1, 1, 0, 0, 0)), (second, (2024, 1, 1, 0, 0, 0))):
                with zipfile.ZipFile(target, "w") as archive:
                    archive.writestr(zipfile.ZipInfo("extension/package.json", timestamp), b'{"version":"0.3.1"}')
            self.assertEqual(digest(first), digest(second))
            with zipfile.ZipFile(second, "a") as archive:
                archive.writestr("extension/extra.txt", b"unexpected")
            self.assertNotEqual(digest(first), digest(second))


if __name__ == "__main__":
    unittest.main()
