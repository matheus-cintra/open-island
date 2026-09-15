#!/usr/bin/env python3
"""Inspect an extracted normal bundle without launching its executables."""
import argparse
import hashlib
import json
from pathlib import Path
import stat

SOUNDS = {f"{name}.wav" for name in ["device-added", "complete", "message", "dialog-warning", "suspend-error"]}
QA_MARKERS = [b"qa-harness=registered-only", b"OPEN_ISLAND_QA_WEBDRIVER_PORT"]


def inspect(root, platform):
    root = root.resolve(strict=True)
    binary_dir = root / ("Contents/MacOS" if platform == "macos" else "usr/bin")
    resources = root / ("Contents/Resources" if platform == "macos" else "usr/lib/Open Island")
    for name in ["AUDIO-LICENSES.txt", "audio-manifest.json"]:
        path = resources / "licenses" / name
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"missing audio notices: {name}")
    for name in ["open-island", "open-islandd"]:
        path = binary_dir / name
        if not path.is_file() or not path.stat().st_mode & 0o111:
            raise ValueError(f"missing executable: {name}")
    for name in SOUNDS:
        path = resources / "sounds" / name
        if not path.is_file() or path.stat().st_size == 0:
            raise ValueError(f"missing sound: {name}")
    if platform == "macos":
        localized = resources / "pt-BR.lproj/InfoPlist.strings"
        if not localized.is_file() or b"NSMicrophoneUsageDescription" not in localized.read_bytes():
            raise ValueError("missing microphone localization")
    manifest = []
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode) or not (stat.S_ISREG(mode) or stat.S_ISDIR(mode)):
            raise ValueError(f"unexpected link or special file: {relative}")
        if path.is_dir():
            continue
        if path.suffix.lower() in {".bin", ".gguf", ".ggml", ".sock"} or any(part in {"test-fixtures", ".env", "voice.json"} for part in path.parts):
            raise ValueError(f"model, private state or QA artifact: {relative}")
        if path.suffix.lower() == ".wav" and (path.parent != resources / "sounds" or path.name not in SOUNDS):
            raise ValueError(f"unexpected audio fixture: {relative}")
        digest = hashlib.sha256()
        previous = b""
        with path.open("rb") as stream:
            while chunk := stream.read(1024 * 1024):
                digest.update(chunk)
                block = previous + chunk
                if any(marker in block for marker in QA_MARKERS):
                    raise ValueError(f"QA code in normal bundle: {relative}")
                previous = block[-64:]
        manifest.append({"path": relative, "bytes": path.stat().st_size, "sha256": digest.hexdigest()})
    return {"status": "PASS", "platform": platform, "files": manifest,
            "scope": "content only; architecture, signatures, dependencies and native behavior require separate checks"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--platform", required=True, choices=["linux", "macos"])
    args = parser.parse_args()
    print(json.dumps(inspect(args.root, args.platform), indent=2))
