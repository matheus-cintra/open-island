import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from contextlib import ExitStack
from argparse import Namespace
import json
from PIL import Image
from native_linux import Child, become_subreaper, has_visible_content, owns_listener, reap_owned_children, wait_for, run


class NativeRunnerTests(unittest.TestCase):
    def test_legacy_daemon_is_rejected_without_invoking_unknown_version_option(self):
        qa = Path(__file__).resolve().parents[2] / "target/portable-qa"
        qa.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="legacy-guard-", dir=qa) as temporary:
            directory = Path(temporary)
            sentinel = directory / "unsafe-option"
            daemon = directory / "open-islandd"
            daemon.write_text('#!/bin/sh\ncase "$1" in --help) printf "legacy help\\n";; *) printf started > "' + str(sentinel) + '";; esac\n')
            daemon.chmod(0o700)
            app = directory / "open-island"
            app.write_bytes(b"must never execute this application")
            out = directory / "result"
            code = run(Namespace(app=app, daemon=daemon, compositor=Path("/bin/true"), out=out, case="reconnect-empty"))
            report = json.loads((out / "result.json").read_text())
            self.assertEqual(code, 1)
            self.assertEqual(report["detail"], "daemon_advertises_safe_version_option")
            self.assertTrue(report["children_reaped"])
            self.assertFalse(sentinel.exists())
            self.assertFalse((out / "daemon-version.txt").exists())
            self.assertFalse((out / "application.log").exists())

    def test_blank_and_transparent_captures_are_rejected(self):
        for color in [(0, 0, 0, 255), (255, 255, 255, 0)]:
            output = io.BytesIO()
            Image.new("RGBA", (232, 46), color).save(output, format="PNG")
            self.assertFalse(has_visible_content(output.getvalue()))
        output = io.BytesIO()
        image = Image.new("RGBA", (664, 162), (0, 0, 0, 255))
        image.paste((255, 255, 255, 255), (20, 20, 40, 30))
        image.save(output, format="PNG")
        self.assertTrue(has_visible_content(output.getvalue()))
        self.assertFalse(has_visible_content(output.getvalue(), (0, 40, 664, 100)))
        self.assertTrue(has_visible_content(output.getvalue(), (15, 15, 50, 40)))

    def test_private_listener_must_belong_to_the_application(self):
        import socket
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            port = listener.getsockname()[1]
            self.assertTrue(owns_listener(os.getpid(), port))
            self.assertFalse(owns_listener(os.getppid(), port))

    def test_exception_cleanup_reaps_a_grandchild_that_detached_its_session(self):
        become_subreaper()
        with tempfile.TemporaryDirectory(prefix="oi-native-guard-") as temporary:
            directory = Path(temporary)
            pidfile = directory / "child.pid"
            script = "import pathlib,subprocess,sys,time; child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)'],stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,start_new_session=True); pathlib.Path(sys.argv[1]).write_text(str(child.pid)); time.sleep(60)"
            try:
                with ExitStack() as cleanup:
                    child = Child([sys.executable, "-c", script, str(pidfile)], {"PATH": os.defpath}, directory / "log", cleanup)
                    wait_for(pidfile.exists, [child], timeout=3)
                    pid = int(pidfile.read_text())
                    raise RuntimeError("injected failure")
            except RuntimeError as error:
                self.assertEqual(str(error), "injected failure")
            finally:
                reaped = reap_owned_children()
            self.assertGreaterEqual(reaped, 1)
            self.assertFalse(Path(f"/proc/{pid}").exists())
            self.assertIsNotNone(child.process.poll())


if __name__ == "__main__":
    unittest.main()
