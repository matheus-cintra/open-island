#!/usr/bin/env python3
"""Evaluate only the pinned synthetic fixtures/model; never opens audio devices."""
import argparse
import hashlib
import json
import os
import re
import resource
import subprocess
import sys
import unicodedata
from pathlib import Path

APP = Path(__file__).resolve().parents[1]
REFERENCE = "Mostre os testes do projeto e verifique se a mensagem chegou na sessão correta."
MODEL_HASH = "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def words(text):
    return re.findall(r"\w+", unicodedata.normalize("NFC", text).lower())


def wer(reference, hypothesis):
    previous = list(range(len(hypothesis) + 1))
    for index, expected in enumerate(reference, 1):
        row = [index]
        for column, actual in enumerate(hypothesis, 1):
            row.append(min(row[-1] + 1, previous[column] + 1,
                           previous[column - 1] + (expected != actual)))
        previous = row
    return previous[-1] / len(reference)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--probe", required=True, type=Path)
    parser.add_argument("--model", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--qemu", type=Path)
    parser.add_argument("--sysroot", type=Path)
    parser.add_argument("--case", choices=["quality", "cancel"], default="quality")
    args = parser.parse_args()
    if args.sysroot and not args.qemu:
        parser.error("--sysroot requires --qemu")
    args.evidence.mkdir(parents=True, exist_ok=True)
    try:
        return evaluate(args)
    except (OSError, ValueError, KeyError, TypeError) as error:
        report = dict(status="FAIL", stage="validation_or_result", error_type=type(error).__name__)
        (args.evidence / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


def require(condition, message):
    if not condition:
        raise ValueError(message)


def cancellation_failures(result):
    failures = []
    requested = result.get("cancellation", {}).get("requested_at_ms")
    elapsed = result.get("elapsed_ms")
    if result.get("result") != {"Err": "voice_cancelled"}:
        failures.append("cancelled work returned a result other than voice_cancelled")
    if (type(requested) is not int or type(elapsed) is not int
            or requested < 5000 or elapsed < requested or elapsed - requested > 10000):
        failures.append("cancellation was not requested in flight or exceeded the 10s QA grace")
    return failures


def evaluate(args):
    # A failed CPU baseline must produce evidence, not a potentially huge core.
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    (args.evidence / "result.json").write_text('{"status":"RUNNING"}\n')
    manifest = json.loads((APP / "test-fixtures/voice/manifest.json").read_text())
    require(manifest["reference"] == REFERENCE, "reference changed")
    require(manifest["max_normalized_wer"] == 0.35, "WER limit changed")
    require(manifest["required_words"] == ["testes", "sessão"], "required words changed")
    require(args.model.stat().st_size == 487601967, "model size mismatch")
    require(digest(args.model) == MODEL_HASH, "model digest mismatch")
    require({item["path"] for item in manifest["files"]} == {"pt-br.wav", "silence.wav"}, "fixture set changed")
    for item in manifest["files"]:
        require(digest(APP / "test-fixtures/voice" / item["path"]) == item["sha256"], "fixture digest mismatch")
    prefix = []
    if args.qemu:
        prefix = [str(args.qemu.resolve()), "-cpu",
                  "qemu64,-pni,-ssse3,-sse4.1,-sse4.2,-avx,-avx2,-fma,-f16c,-bmi2"]
        if args.sysroot:
            prefix += ["-L", str(args.sysroot.resolve())]
    results = {}
    for name in (["pt-br"] if args.case == "cancel" else ["pt-br", "silence"]):
        log = args.evidence / f"{name}.log"
        command = prefix + [str(args.probe.resolve()), str(args.model.resolve()),
                            str(APP / "test-fixtures/voice" / f"{name}.wav")]
        if args.qemu:
            command.append("--require-v1")
        if args.case == "cancel":
            command.append("--cancel-after-ms=5000")
        # The shared runner owns the process group, deadline, output and cleanup.
        env = dict(os.environ, OPEN_ISLAND_TEST_LOG=str(log.resolve()))
        process = subprocess.run([sys.executable, str(APP.parent / "scripts/test-processes.py"), *command],
                                 env=env)
        if process.returncode:
            report = dict(status="FAIL", stage="probe_process", fixture=name,
                          returncode=process.returncode, probe_sha256=digest(args.probe),
                          model_sha256=MODEL_HASH, mode="qemu-v1" if args.qemu else "host")
            (args.evidence / "result.json").write_text(json.dumps(report, indent=2) + "\n")
            return 1
        results[name] = json.loads(log.read_text())
    if args.case == "cancel":
        failures = cancellation_failures(results["pt-br"])
        report = dict(status="FAIL" if failures else "PASS", failures=failures,
                      mode="qemu-v1" if args.qemu else "host", case="cancel",
                      probe_sha256=digest(args.probe), model_sha256=MODEL_HASH,
                      manifest_sha256=digest(APP / "test-fixtures/voice/manifest.json"),
                      results=results,
                      scope="timed cancellation of real-model work; no capture, GUI, or exact native phase proof")
        (args.evidence / "result.json").write_text(json.dumps(report, indent=2) + "\n")
        print(f"{report['status']}: real-model cancellation ({report['mode']})")
        return 1 if failures else 0
    transcript = results["pt-br"]["result"].get("Ok")
    hypothesis = words(transcript) if isinstance(transcript, str) else []
    error_rate = wer(words(REFERENCE), hypothesis)
    failures = []
    if not isinstance(transcript, str):
        failures.append("speech did not produce a transcript")
    if error_rate > 0.35:
        failures.append(f"WER {error_rate} exceeds 0.35")
    if not all(word in hypothesis for word in ["testes", "sessão"]):
        failures.append("required word missing")
    if results["silence"]["result"] != {"Err": "no_speech"}:
        failures.append("silence did not return no_speech")
    report = dict(status="FAIL" if failures else "PASS", failures=failures, mode="qemu-v1" if args.qemu else "host",
                  probe_sha256=digest(args.probe), model_sha256=MODEL_HASH,
                  manifest_sha256=digest(APP / "test-fixtures/voice/manifest.json"),
                  normalized_wer=error_rate, results=results,
                  scope="synthetic transcription and silence; no microphone or GUI proof")
    (args.evidence / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(f"{report['status']}: synthetic voice WER={error_rate:.3f} ({report['mode']})")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
