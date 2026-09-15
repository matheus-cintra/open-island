import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("verify_bundle_content", Path(__file__).with_name("verify-bundle-content.py"))
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class BundleTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.binaries = self.root / "Contents/MacOS"
        self.resources = self.root / "Contents/Resources"
        self.binaries.mkdir(parents=True)
        for name in ["open-island", "open-islandd"]:
            path = self.binaries / name
            path.write_bytes(b"test fixture; never executed")
            path.chmod(0o755)
        (self.resources / "sounds").mkdir(parents=True)
        for name in bundle.SOUNDS:
            (self.resources / "sounds" / name).write_bytes(b"RIFF")
        self.localized = self.resources / "pt-BR.lproj/InfoPlist.strings"
        self.localized.parent.mkdir()
        self.localized.write_text('"NSMicrophoneUsageDescription" = "Local";')
        (self.resources / "licenses").mkdir()
        for name in ["AUDIO-LICENSES.txt", "audio-manifest.json"]:
            (self.resources / "licenses" / name).write_text("content fixture; full license validation is separate")

    def test_required_content_and_stable_relative_manifest(self):
        report = bundle.inspect(self.root, "macos")
        self.assertEqual(report["status"], "PASS")
        self.assertEqual(len(report["files"]), 10)
        self.assertTrue(all(not Path(file["path"]).is_absolute() for file in report["files"]))

    def test_missing_sidecar_or_localization_is_rejected(self):
        for path in [self.binaries / "open-islandd", self.localized, self.resources / "licenses/AUDIO-LICENSES.txt"]:
            with self.subTest(path=path.name):
                original = path.read_bytes()
                path.unlink()
                with self.assertRaises(ValueError):
                    bundle.inspect(self.root, "macos")
                path.write_bytes(original)
                path.chmod(0o755)

    def test_model_audio_fixture_and_qa_binary_are_rejected(self):
        for name in ["ggml-small.bin", "model.gguf", "fixture.wav", "voice.json"]:
            with self.subTest(name=name):
                path = self.resources / name
                path.write_bytes(b"unexpected")
                with self.assertRaises(ValueError):
                    bundle.inspect(self.root, "macos")
                path.unlink()
        (self.binaries / "open-island").write_bytes(b"x" * (1024 * 1024 - 8) + bundle.QA_MARKERS[1])
        with self.assertRaisesRegex(ValueError, "QA code"):
            bundle.inspect(self.root, "macos")


if __name__ == "__main__":
    unittest.main()
