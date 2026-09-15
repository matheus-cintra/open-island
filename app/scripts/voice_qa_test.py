import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("voice-qa.py")
SPEC = importlib.util.spec_from_file_location("voice_qa", SCRIPT)
QA = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(QA)


class VoiceGate(unittest.TestCase):
    def test_cancel_requires_observed_request_error_and_bounded_return(self):
        valid = {"result": {"Err": "voice_cancelled"}, "elapsed_ms": 5200,
                 "cancellation": {"requested_at_ms": 5000}}
        self.assertEqual(QA.cancellation_failures(valid), [])
        for change in [
                {"result": {"Ok": "late transcript"}},
                {"result": {"Err": "invalid_model"}},
                {"cancellation": {"requested_at_ms": None}},
                {"cancellation": {"requested_at_ms": True}},
                {"cancellation": {"requested_at_ms": 4999}},
                {"elapsed_ms": 4999}, {"elapsed_ms": 15001}]:
            with self.subTest(change=change):
                self.assertTrue(QA.cancellation_failures(valid | change))

    def test_normalization_preserves_words_and_accents(self):
        self.assertEqual(QA.words("VERIFIQUE-SE a sessa\u0303o correta!"),
                         ["verifique", "se", "a", "sessão", "correta"])
        self.assertEqual(QA.wer(["um", "dois"], []), 1)
        self.assertEqual(QA.wer(["um", "dois"], ["um", "três"]), 0.5)
        self.assertEqual(QA.wer(["um", "dois"], ["um", "dois", "três"]), 0.5)

    def test_invalid_model_fails_before_probe_even_with_python_optimization(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            model = root / "invalid.bin"
            model.write_bytes(b"invalid model")
            evidence = root / "evidence"
            result = subprocess.run(
                [sys.executable, "-O", str(SCRIPT), "--model", str(model),
                 "--probe", str(root / "must-not-run"), "--evidence", str(evidence)],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
            self.assertEqual(result.returncode, 1)
            report = json.loads((evidence / "result.json").read_text())
            self.assertEqual(report["status"], "FAIL")
            self.assertEqual(report["stage"], "validation_or_result")
            self.assertFalse((evidence / "pt-br.log").exists())


if __name__ == "__main__":
    unittest.main()
