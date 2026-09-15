import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("audio_licenses", Path(__file__).with_name("verify-audio-licenses.py"))
licenses = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licenses)


class AudioLicenseTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        text = "Upstream permission and attribution\r\n"
        digest = hashlib.sha256(text.encode()).hexdigest()
        self.manifest = {"schema_version": 1, "texts": {digest: text}, "packages": [{
            "name": "audio", "version": "1.0.0", "license": "MIT", "crate_sha256": "crate-checksum",
            "license_files": [{"sha256": digest, "source_url": "https://example.org/LICENSE"}]}]}
        self.expected = {("audio", "1.0.0"): "MIT"}
        self.checksums = {("audio", "1.0.0"): "crate-checksum"}
        (self.root / "AUDIO-LICENSES.txt").write_bytes(("audio 1.0.0\n" + text).encode())

    def verify(self):
        (self.root / "audio-manifest.json").write_text(json.dumps(self.manifest))
        return licenses.verify(self.root, self.expected, self.checksums)

    def test_original_text_bytes_and_complete_inventory_pass(self):
        self.assertEqual(self.verify(), {"status": "PASS", "packages": 1, "texts": 1})

    def test_missing_dependency_or_changed_crate_checksum_is_rejected(self):
        self.expected[("new-audio", "2.0.0")] = "MIT"
        with self.assertRaisesRegex(ValueError, "coverage changed"):
            self.verify()
        del self.expected[("new-audio", "2.0.0")]
        self.checksums[("audio", "1.0.0")] = "changed"
        with self.assertRaisesRegex(ValueError, "checksum changed"):
            self.verify()

    def test_altered_license_or_missing_readable_notice_is_rejected(self):
        digest = next(iter(self.manifest["texts"]))
        original = self.manifest["texts"][digest]
        self.manifest["texts"][digest] = "altered"
        with self.assertRaisesRegex(ValueError, "altered license"):
            self.verify()
        self.manifest["texts"][digest] = original
        (self.root / "AUDIO-LICENSES.txt").write_text("audio 1.0.0\n")
        with self.assertRaisesRegex(ValueError, "missing readable text"):
            self.verify()


if __name__ == "__main__":
    unittest.main()
