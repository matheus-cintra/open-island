from pathlib import Path
import os
import subprocess
import tempfile
import unittest


class ResourceUninstallTests(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.home = Path(directory.name) / "home"
        self.sounds = self.home / ".local/lib/Open Island/sounds"
        self.sounds.mkdir(parents=True)
        source = Path(__file__).resolve().parents[1].joinpath("install.sh").read_text()
        self.assertTrue(source.endswith('main "$@"\n'))
        # Load functions only. Never invoke platform preflight, integrations or a daemon.
        self.source = source.removesuffix('main "$@"\n')

    def run_cleanup(self):
        return subprocess.run(["sh"], input=self.source + '\nremove_tarball_resources\nprintf "%s" "$FAILED_SUMMARY"\n',
            env={"HOME": str(self.home), "PATH": os.defpath}, capture_output=True, text=True, timeout=5, check=True)

    def test_known_resources_removed_unknown_files_preserved_and_repeat_is_safe(self):
        owned = [self.sounds / f"{name}.wav" for name in ["device-added", "complete", "message", "dialog-warning", "suspend-error"]]
        owned.append(self.sounds / "README.md")
        notices = self.sounds.parent / "licenses"
        notices.mkdir()
        owned.extend(notices / name for name in ["AUDIO-LICENSES.txt", "audio-manifest.json"])
        for path in owned:
            path.write_text("distributed resource")
        custom = self.sounds / "custom.wav"
        custom.write_text("user content")
        self.assertEqual(self.run_cleanup().stdout, "")
        self.assertTrue(all(not path.exists() for path in owned))
        self.assertEqual(custom.read_text(), "user content")
        self.assertEqual(self.run_cleanup().stdout, "")
        self.assertEqual(custom.read_text(), "user content")

    def test_symlinked_resource_directory_does_not_remove_external_files(self):
        self.sounds.rmdir()
        outside = self.home.parent / "outside"
        outside.mkdir()
        original = outside / "complete.wav"
        original.write_text("unrelated")
        self.sounds.symlink_to(outside, target_is_directory=True)
        self.assertIn("symbolic link", self.run_cleanup().stdout)
        self.assertEqual(original.read_text(), "unrelated")
        self.assertTrue(self.sounds.is_symlink())


if __name__ == "__main__":
    unittest.main()
