#!/usr/bin/env python3
"""Assemble Tauri's signed macOS updater index after both architecture jobs finish."""
import argparse
import base64
import json
import re
from pathlib import Path
from urllib.parse import quote


def manifest(version: str, assets: Path, repository: str) -> dict:
    version = version.removeprefix("v")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", version):
        raise ValueError("Invalid release version")
    if not re.fullmatch(r"[\w.-]+/[\w.-]+", repository):
        raise ValueError("Invalid repository")
    platforms = {}
    for arch in ("aarch64", "x86_64"):
        filename = f"open-island-macos-{arch}.app.tar.gz"
        if not (assets / filename).is_file() or (assets / filename).stat().st_size == 0:
            raise ValueError(f"Missing updater bundle: {filename}")
        signature = (assets / (filename + ".sig")).read_text().strip()
        # This checks transport encoding only. The updater verifies the cryptographic signature.
        if not base64.b64decode(signature, validate=True):
            raise ValueError(f"Empty signature: {filename}")
        platforms[f"darwin-{arch}"] = {
            "signature": signature,
            "url": f"https://github.com/{repository}/releases/download/v{quote(version, safe='')}/{filename}",
        }
    return {"version": version, "platforms": platforms}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version")
    parser.add_argument("assets", type=Path)
    parser.add_argument("repository")
    args = parser.parse_args()
    result = manifest(args.version, args.assets, args.repository)
    (args.assets / "latest.json").write_text(json.dumps(result, indent=2) + "\n")
