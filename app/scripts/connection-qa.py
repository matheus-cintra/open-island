#!/usr/bin/env python3
"""Exercise the real shell transport across an owned QA daemon restart."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


def stop(child):
    if child is None:
        return
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=3)
        except subprocess.TimeoutExpired:
            child.kill()
    child.wait(timeout=3)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--daemon", type=Path, required=True, help="qa-harness daemon binary")
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    daemon = args.daemon.resolve(strict=True)
    probe = args.probe.resolve(strict=True)
    args.out.mkdir(parents=True, exist_ok=True)
    result = {"status": "FAIL", "kind": "real Unix IPC, QA daemon and production shell transport", "binaries": {
        "daemon_sha256": hashlib.sha256(daemon.read_bytes()).hexdigest(),
        "probe_sha256": hashlib.sha256(probe.read_bytes()).hexdigest(),
    }}
    server = client = None
    try:
        with tempfile.TemporaryDirectory(prefix="oi-conn-", dir="/tmp") as temporary, ExitStack() as cleanup:
            cleanup.callback(lambda: stop(server))
            cleanup.callback(lambda: stop(client))
            home = Path(temporary).resolve()
            socket = home / "daemon.sock"
            config = home / "config.json"
            config.write_text(json.dumps({"usage": {"show_limits": False}, "updates": {"check_enabled": False}, "integrations": {"auto_configure": False}, "sound": {"enabled": False}}))
            config.chmod(0o600)
            env = {"PATH": os.defpath, "LANG": "C.UTF-8", "HOME": str(home),
                   "OPEN_ISLAND_QA": "1", "OPEN_ISLAND_QA_CREDENTIALS_BLOCKED": "1",
                   "OPEN_ISLAND_CONFIG": str(config), "OPEN_ISLAND_SOCKET": str(socket),
                   "DBUS_SESSION_BUS_ADDRESS": f"unix:path={home}/unavailable-bus"}
            for key in ["XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "XDG_RUNTIME_DIR", "XDG_CACHE_HOME"]:
                directory = home / key.lower()
                directory.mkdir(mode=0o700)
                env[key] = str(directory)
            with (args.out / "daemon.log").open("wb") as daemon_log, (args.out / "probe.log").open("wb") as probe_log:
                def spawn():
                    return subprocess.Popen([str(daemon), "--socket", str(socket)], env=env, stdin=subprocess.DEVNULL, stdout=daemon_log, stderr=daemon_log)
                server = spawn()
                deadline = time.monotonic() + 5
                while not socket.exists():
                    if server.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("daemon_start_failed")
                    time.sleep(0.02)
                client = subprocess.Popen([str(probe), str(socket), "2"], env=env, stdin=subprocess.DEVNULL, stdout=probe_log, stderr=probe_log)
                deadline = time.monotonic() + 10
                while True:
                    lines = (args.out / "probe.log").read_text().splitlines()
                    if any(json.loads(line).get("connected_epochs") == 1 for line in lines):
                        break
                    if client.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("initial_shell_hydration_failed")
                    time.sleep(0.02)
                stop(server)
                socket.unlink(missing_ok=True)
                server = spawn()
                if client.wait(timeout=20) != 0:
                    raise RuntimeError("shell_reconnection_failed")
                observations = [json.loads(line) for line in (args.out / "probe.log").read_text().splitlines()]
                if len(observations) != 2 or [row["connected_epochs"] for row in observations] != [1, 2]:
                    raise RuntimeError("missing_epoch_transition")
                if not all(row["cached"] for row in observations) or observations[1]["generation"] <= observations[0]["generation"]:
                    raise RuntimeError("invalid_cached_generation")
                result.update(status="PASS", observations=observations, assertions=4)
    except (OSError, RuntimeError, subprocess.TimeoutExpired, ValueError) as error:
        result["error"] = type(error).__name__
    finally:
        stop(client)
        stop(server)
        result["children_reaped"] = all(child is None or child.poll() is not None for child in [server, client])
        (args.out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return 0 if result["status"] == "PASS" and result["children_reaped"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
