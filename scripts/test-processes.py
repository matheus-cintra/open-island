#!/usr/bin/env python3
"""Run tests in an owned process group; never pipe their inherited stdout."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

log = Path(os.environ.get("OPEN_ISLAND_TEST_LOG", "/tmp/open-island-tests.log"))
command = sys.argv[1:] or ["cargo", "test", "--workspace"]
test_env = os.environ.copy()
# macOS's default /var/folders/... path can exceed sockaddr_un.sun_path once a
# fixture adds its unique name. Production uses a similarly short private path.
if sys.platform == "darwin":
    test_env["TMPDIR"] = "/tmp"
with log.open("wb") as output:
    child = subprocess.Popen(command, stdout=output, stderr=subprocess.STDOUT, start_new_session=True, env=test_env)
    try:
        code = child.wait(timeout=600)
    except subprocess.TimeoutExpired:
        code = 124
    finally:
        try:
            os.killpg(child.pid, 0)
        except ProcessLookupError:
            pass
        else:
            print("Test process group survived; terminating its children.", file=sys.stderr)
            if code == 0:
                code = 1
            os.killpg(child.pid, signal.SIGTERM)
            time.sleep(1)
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        child.wait()
print(f"Test log: {log}")
for line in log.read_text(errors="replace").splitlines():
    if "test result:" in line or "error" in line or "FAILED" in line:
        print(line)
if code:
    print("\n".join(log.read_text(errors="replace").splitlines()[-80:]))
sys.exit(code)
