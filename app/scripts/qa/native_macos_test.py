import struct
import tempfile
import unittest
import zlib
from pathlib import Path

import native_macos


def png(width, height, rows):
    header = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    payload = b"".join(bytes([0]) + row for row in rows)
    def chunk(kind, value):
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(payload)) + chunk(b"IEND", b"")


class NativeMacRunnerTests(unittest.TestCase):
    def test_png_decoder_rejects_blank_capture_and_accepts_visible_capture(self):
        blank = png(100, 20, [bytes(400) for _ in range(20)])
        visible = png(100, 20, [bytes([255, 255, 255, 255] * 100) for _ in range(20)])
        self.assertFalse(native_macos.has_visible_content(blank))
        self.assertTrue(native_macos.has_visible_content(visible))

    def test_png_decoder_rejects_invalid_signature(self):
        with self.assertRaises(RuntimeError):
            native_macos.png_info(b"not-a-png")

    def test_app_command_resolves_bundle_executable(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary) / "Open Island.app" / "Contents" / "MacOS"
            bundle.mkdir(parents=True)
            executable = bundle / "open-island"
            executable.write_bytes(b"#! /bin/sh\n")
            executable.chmod(0o700)
            self.assertEqual(native_macos.app_command(bundle.parents[1]), [str(executable)])


if __name__ == "__main__":
    unittest.main()
