#!/usr/bin/env python3
"""Check shipped audio notices against the locked supported-target dependency graph."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def verify(resources, expected, checksums):
    manifest = json.loads((resources / "audio-manifest.json").read_text())
    notices = (resources / "AUDIO-LICENSES.txt").read_bytes().decode()
    if manifest.get("schema_version") != 1:
        raise ValueError("unsupported audio license manifest")
    entries = {}
    for package in manifest["packages"]:
        key = (package["name"], package["version"])
        if key in entries or not package.get("license_files"):
            raise ValueError(f"duplicate or missing license entry: {key}")
        entries[key] = package["license"]
        if package["crate_sha256"] != checksums.get(key):
            raise ValueError(f"crate checksum changed: {key}")
        if f"{key[0]} {key[1]}" not in notices:
            raise ValueError(f"missing attribution in readable notices: {key}")
        for reference in package["license_files"]:
            digest = reference["sha256"]
            text = manifest["texts"].get(digest)
            if not text or hashlib.sha256(text.encode()).hexdigest() != digest:
                raise ValueError(f"missing or altered license text: {key}")
            if text not in notices or not reference["source_url"].startswith("https://"):
                raise ValueError(f"missing readable text or source: {key}")
    if entries != expected:
        raise ValueError(f"audio dependency coverage changed; missing={sorted(expected.keys() - entries.keys())}, extra={sorted(entries.keys() - expected.keys())}")
    return {"status": "PASS", "packages": len(entries), "texts": len(manifest["texts"])}


def dependency_inventory(app):
    roots = {"cpal", "whisper-rs", "rubato", "audioadapter-buffers"}
    expected = {}
    for target in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin", "x86_64-apple-darwin"]:
        data = json.loads(subprocess.check_output(["cargo", "metadata", "--manifest-path", str(app / "Cargo.toml"),
            "--locked", "--offline", "--format-version", "1", "--filter-platform", target], timeout=60))
        packages = {p["id"]: p for p in data["packages"]}
        nodes = {node["id"]: node for node in data["resolve"]["nodes"]}
        pending = [key for key, p in packages.items() if p["name"] in roots]
        if {packages[key]["name"] for key in pending} != roots:
            raise ValueError("missing audio dependency root")
        seen = set()
        while pending:
            key = pending.pop()
            if key in seen:
                continue
            seen.add(key)
            package = packages[key]
            expected[(package["name"], package["version"])] = package["license"]
            pending.extend(dep["pkg"] for dep in nodes[key]["deps"] if any(kind["kind"] is None for kind in dep["dep_kinds"]))
    checksums = {}
    for block in (app / "Cargo.lock").read_text().split("[[package]]")[1:]:
        fields = dict(re.findall(r'^(name|version|checksum) = "([^"\n]+)"$', block, re.M))
        if "checksum" in fields:
            checksums[(fields["name"], fields["version"])] = fields["checksum"]
    return expected, checksums


if __name__ == "__main__":
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser()
    parser.add_argument("--resources", type=Path, default=root / "app/src-tauri/resources/licenses")
    args = parser.parse_args()
    print(json.dumps(verify(args.resources, *dependency_inventory(root / "app")), indent=2))
