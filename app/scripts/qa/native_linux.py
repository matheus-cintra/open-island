#!/usr/bin/env python3
"""Run the real QA WebView in an owned headless Sway session, without host credentials."""
import argparse
import base64
import ctypes
from contextlib import ExitStack
import hashlib
import io
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import tempfile
import time
import urllib.request
from PIL import Image


def become_subreaper():
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "cannot own orphaned QA descendants")


def reap_owned_children():
    count = 0
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        # Only direct children of this supervisor, including adopted WebKit helpers.
        path = Path(f"/proc/{os.getpid()}/task/{os.getpid()}/children")
        for value in path.read_text().split():
            try:
                os.kill(int(value), signal.SIGKILL)
            except ProcessLookupError:
                pass
        try:
            pid, _ = os.waitpid(-1, os.WNOHANG)
        except ChildProcessError:
            return count
        if pid:
            count += 1
        else:
            time.sleep(0.01)
    raise RuntimeError("qa_descendants_not_reaped")


def has_visible_content(png, bounds=None):
    with Image.open(io.BytesIO(png)) as source:
        if bounds is not None:
            source = source.crop(bounds)
        rgba = source.convert("RGBA")
        pixels = Image.alpha_composite(Image.new("RGBA", rgba.size, (0, 0, 0, 255)), rgba).convert("L")
        return sum(pixels.histogram()[100:]) >= 20


class Child:
    def __init__(self, command, env, log, cleanup):
        output = cleanup.enter_context(log.open("wb"))
        self.process = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                        stdout=output, stderr=output, start_new_session=True)
        cleanup.callback(self.stop)

    def stop(self):
        if self.process.poll() is not None:
            return
        os.killpg(self.process.pid, signal.SIGTERM)
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.wait(timeout=3)
        # Remaining helpers are adopted and killed by reap_owned_children(), using
        # parenthood rather than a possibly retired process-group number.


def wait_for(check, children, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if any(child.process.poll() is not None for child in children):
            raise RuntimeError("owned_process_exited")
        result = check()
        if result:
            return result
        time.sleep(0.05)
    raise RuntimeError("native_condition_timeout")


def owns_listener(pid, port):
    """Do not send automation commands to an unrelated process that won the port race."""
    try:
        sockets = set()
        for fd in Path(f"/proc/{pid}/fd").iterdir():
            try:
                sockets.add(os.readlink(fd))
            except FileNotFoundError:
                continue
        for line in Path(f"/proc/{pid}/net/tcp").read_text().splitlines()[1:]:
            fields = line.split()
            if fields[1] == f"0100007F:{port:04X}" and fields[3] == "0A" and f"socket:[{fields[9]}]" in sockets:
                return True
    except (OSError, IndexError):
        return False
    return False


class Driver:
    def __init__(self, port):
        self.url = f"http://127.0.0.1:{port}"
        self.http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        self.session = None

    def request(self, method, path, data=None):
        body = None if data is None else json.dumps(data).encode()
        request = urllib.request.Request(self.url + path, data=body, method=method,
                                         headers={"Content-Type": "application/json"})
        with self.http.open(request, timeout=10) as response:
            raw = response.read(8 * 1024 * 1024 + 1)
        if len(raw) > 8 * 1024 * 1024:
            raise RuntimeError("webdriver_response_too_large")
        value = json.loads(raw)["value"]
        if isinstance(value, dict) and "error" in value:
            raise RuntimeError("webdriver_command_failed")
        return value

    def connect(self, window="main"):
        value = self.request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "wdio:tauriServiceOptions": {"windowLabel": window}}}})
        self.session = value["sessionId"]
        self.request("POST", f"/session/{self.session}/timeouts", {"script": 5000})
        return value["capabilities"]

    def script(self, script, asynchronous=False):
        method = "async" if asynchronous else "sync"
        return self.request("POST", f"/session/{self.session}/execute/{method}", {"script": script, "args": []})

    def screenshot(self):
        encoded = self.request("GET", f"/session/{self.session}/screenshot")
        png = base64.b64decode(encoded, validate=True)
        if png[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" or len(png) < 64:
            raise RuntimeError("invalid_native_png")
        width, height = struct.unpack(">II", png[16:24])
        if not (100 <= width <= 4096 and 20 <= height <= 2160):
            raise RuntimeError("unexpected_native_png_dimensions")
        return png, width, height


def run(args):
    become_subreaper()
    app = args.app.resolve(strict=True)
    compositor = args.compositor.resolve(strict=True)
    # An explicitly built QA artifact is required; never fall back to the installed application.
    qa_target = Path(__file__).resolve().parents[2] / "target/portable-qa"
    if not app.is_relative_to(qa_target):
        raise ValueError("application_must_come_from_portable_qa_build")
    daemon = args.daemon.resolve(strict=True) if args.daemon else None
    if args.case in ["reconnect-empty", "reconnect-draft", "message-success", "message-failure", "message-unconfirmed", "many-sessions", "render-burst", "diagnostics-ready"] and (daemon is None or not daemon.is_relative_to(qa_target)):
        raise ValueError("daemon_must_come_from_portable_qa_build")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False, mode=0o700)
    result = {"status": "FAIL", "case": args.case, "platform": "linux", "assertions": [],
              "app_sha256": hashlib.sha256(app.read_bytes()).hexdigest(),
              "compositor_sha256": hashlib.sha256(compositor.read_bytes()).hexdigest(),
              "capture_source": "WebKitGTK snapshot API; private headless Sway",
              "capture_phase": "initial_offline_boot",
              "physical_input_tested": False}
    children = []
    if daemon:
        result["daemon_sha256"] = hashlib.sha256(daemon.read_bytes()).hexdigest()

    def check(condition, name):
        result["assertions"].append({"name": name, "pass": bool(condition)})
        if not condition:
            raise RuntimeError(name)

    try:
        with tempfile.TemporaryDirectory(prefix="oi-native-") as directory, ExitStack() as cleanup:
            home = Path(directory).resolve()
            env = {"PATH": os.defpath, "HOME": str(home), "LANG": "C.UTF-8",
                   "OPEN_ISLAND_QA": "1", "OPEN_ISLAND_QA_CREDENTIALS_BLOCKED": "1",
                   "OPEN_ISLAND_SOCKET": str(home / "daemon.sock"),
                   "DBUS_SESSION_BUS_ADDRESS": f"unix:path={home}/bus",
                   "WLR_BACKENDS": "headless", "WLR_RENDERER": "pixman",
                   "WLR_LIBINPUT_NO_DEVICES": "1", "GDK_BACKEND": "wayland"}
            for key in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR", "XDG_CACHE_HOME"]:
                path = home / key.lower()
                path.mkdir(mode=0o700)
                env[key] = str(path)
            daemon_config = home / "config.json"
            configuration = {"usage": {"show_limits": False},
                "updates": {"check_enabled": False}, "integrations": {"auto_configure": False},
                "sound": {"enabled": False}}
            if args.case in ["message-success", "message-failure", "message-unconfirmed"]:
                configuration["sessions"] = {"idle_after_ms": 1000}
            result["fixture_config"] = configuration
            daemon_config.write_text(json.dumps(configuration))
            daemon_config.chmod(0o600)
            env["OPEN_ISLAND_CONFIG"] = str(daemon_config)
            if daemon:
                help_probe = Child([str(daemon), "--help"], env, out / "daemon-help.txt", cleanup)
                children.append(help_probe)
                help_probe.process.wait(timeout=5)
                help_text = (out / "daemon-help.txt").read_text()
                check(help_probe.process.returncode == 0 and any(line.lstrip().startswith("--version ") for line in help_text.splitlines()), "daemon_advertises_safe_version_option")
                children.remove(help_probe)
                probe = Child([str(daemon), "--version"], env, out / "daemon-version.txt", cleanup)
                children.append(probe)
                probe.process.wait(timeout=5)
                check(probe.process.returncode == 0, "daemon_version_probe_exits_successfully")
                info = (out / "daemon-version.txt").read_text().strip()
                check(len(info.split()) == 3 and info.split()[0] == "open-islandd" and info.split()[2] == "qa-harness=registered-only", "daemon_has_registered_only_process_source")
                children.remove(probe)
                result["daemon_version"] = info
            config = home / "sway.conf"
            config.write_text("output HEADLESS-1 mode 1280x720\nseat seat0 fallback true\ndefault_border none\n")
            bus = Child(["dbus-daemon", "--session", "--nofork", f"--address=unix:path={home}/bus", "--nopidfile"], env, out / "bus.log", cleanup)
            children.append(bus)
            wait_for(lambda: (home / "bus").is_socket(), children)
            display = Child([str(compositor), "--config", str(config)], env, out / "compositor.log", cleanup)
            children.append(display)
            runtime = Path(env["XDG_RUNTIME_DIR"])
            wayland = wait_for(lambda: next((p for p in runtime.glob("wayland-*") if p.is_socket()), None), children)
            env["WAYLAND_DISPLAY"] = wayland.name
            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            env["OPEN_ISLAND_QA_WEBDRIVER_PORT"] = str(port)
            application = Child([str(app)], env, out / "application.log", cleanup)
            children.append(application)
            wait_for(lambda: owns_listener(application.process.pid, port), children, timeout=20)
            driver = Driver(port)
            result["capabilities"] = driver.connect()
            identity = driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.app.getIdentifier().then(done);", True)
            check(isinstance(identity, str) and identity.startswith("app.open-island.qa."), "qa_bundle_identifier")
            result["identifier"] = identity
            wait_for(lambda: driver.script("return document.readyState === 'complete' && !!document.querySelector('.daemon-connection-status');"), children)
            driver.script("const done = arguments[arguments.length - 1]; requestAnimationFrame(() => requestAnimationFrame(() => done(true)));", True)
            driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.event.emitTo('main', 'island-toggle', {}).then(() => done(true));", True)
            last_geometry = None
            stable_samples = 0
            def state():
                nonlocal last_geometry, stable_samples
                value = driver.script("const status = document.querySelector('.daemon-connection-status'); const rect = status?.getBoundingClientRect(); return {text: status?.textContent, visible: !!status && !status.hidden && document.querySelector('#island').classList.contains('expanded'), contained: !!rect && rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.top >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight, muted: document.querySelector('#header-mute')?.disabled, editors: document.querySelectorAll('.message-input').length, width: innerWidth, height: innerHeight};")
                result["last_observation"] = value
                geometry = (value["width"], value["height"])
                stable_samples = stable_samples + 1 if geometry == last_geometry else 1
                last_geometry = geometry
                return value if stable_samples >= 3 and value.get("visible") and value.get("contained") and value["width"] >= 600 and value["height"] >= 158 and value.get("text") == "Conectando ao Open Island…" else None
            observation = wait_for(state, children)
            driver.script("const done = arguments[arguments.length - 1]; requestAnimationFrame(() => requestAnimationFrame(() => done(true)));", True)
            check(observation["muted"], "offline_actions_disabled")
            check(observation["editors"] == 0, "offline_boot_has_no_invented_session")
            check(not (home / "daemon.sock").exists(), "qa_did_not_spawn_a_daemon")
            png, width, height = driver.screenshot()
            (out / "island.png").write_bytes(png)
            result.update(png_sha256=hashlib.sha256(png).hexdigest(), png_size=[width, height])
            check(has_visible_content(png), "native_capture_is_not_blank")
            check(width == observation["width"] and height == observation["height"], "capture_matches_webview_geometry")
            result.update(observation=observation, png_sha256=hashlib.sha256(png).hexdigest(), status="PASS")
            if args.case == "reconnect-empty":
                result["status"] = "FAIL"
                def cache():
                    return driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('get_daemon_ui_state').then(done);", True)

                def hydrated():
                    value = cache()
                    snapshot = value.get("snapshot")
                    if value["phase"] != "connected" or not snapshot or snapshot["generation"] != value["generation"] or snapshot["snapshot"].get("discovering"):
                        return None
                    ui = driver.script("return {hidden: document.querySelector('.daemon-connection-status').hidden, disabled: document.querySelector('#header-mute').disabled, editors: document.querySelectorAll('.message-input').length};")
                    return value if ui["hidden"] and not ui["disabled"] and ui["editors"] == 0 else None

                def start_daemon(number):
                    child = Child([str(daemon), "--socket", env["OPEN_ISLAND_SOCKET"]], env, out / f"daemon-{number}.log", cleanup)
                    children.append(child)
                    return child

                first_daemon = start_daemon(1)
                first = wait_for(hydrated, children, timeout=25)
                check(not first["snapshot"]["snapshot"]["sessions"], "first_daemon_hydrates_empty_session_list")
                first_daemon.stop()
                children.remove(first_daemon)
                check(first_daemon.process.poll() is not None, "first_daemon_reaped_before_restart")
                def disconnected():
                    value = cache()
                    ui = driver.script("return {hidden: document.querySelector('.daemon-connection-status').hidden, disabled: document.querySelector('#header-mute').disabled, text: document.querySelector('.daemon-connection-status').textContent};")
                    return value if value["phase"] == "reconnecting" and not ui["hidden"] and ui["disabled"] and ui["text"] == "Reconectando… Os dados exibidos podem estar desatualizados." else None
                down = wait_for(disconnected, children, timeout=15)
                check(down["phase"] == "reconnecting", "webview_disables_actions_during_real_disconnect")
                (home / "daemon.sock").unlink(missing_ok=True)
                start_daemon(2)
                second = wait_for(hydrated, children, timeout=25)
                check(second["generation"] > first["generation"], "shell_connection_generation_advances")
                check(second["snapshot"]["snapshot"]["daemon_epoch"] != first["snapshot"]["snapshot"]["daemon_epoch"], "new_daemon_epoch_replaces_old_cache")
                check(not second["snapshot"]["snapshot"]["sessions"], "second_daemon_hydrates_empty_session_list")
                result["connection_observations"] = [{"phase": value["phase"], "generation": value["generation"],
                    "epoch": value["snapshot"]["snapshot"]["daemon_epoch"] if value.get("snapshot") else None}
                    for value in [first, down, second]]
                result["status"] = "PASS"
            if args.case == "many-sessions":
                from native_sessions import many_sessions
                many_sessions(driver, daemon, env, home, out, cleanup, children, result, check)
            if args.case == "render-burst":
                from native_render import render_burst
                render_burst(driver, daemon, env, home, out, cleanup, children, result, check)
            if args.case in ["reconnect-draft", "message-success", "message-failure", "message-unconfirmed"]:
                from native_sessions import reconnect_draft
                reconnect_draft(driver, daemon, env, home, out, cleanup, children, result, check)
            if args.case in ["diagnostics-offline", "diagnostics-ready"]:
                from native_diagnostics import diagnostics_case
                diagnostics_case(driver, port, daemon, env, home, out, cleanup, children, result, check)
            driver.request("DELETE", f"/session/{driver.session}")
    except Exception as error:
        result.update(status="FAIL", error=type(error).__name__, detail=str(error)[:300])
    finally:
        result["children_reaped"] = all(child.process.poll() is not None for child in children)
        try:
            result["adopted_children_reaped"] = reap_owned_children()
        except RuntimeError:
            result["children_reaped"] = False
        if not result["children_reaped"] or not result["assertions"]:
            result["status"] = "FAIL"
        (out / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    def interrupted(_signal, _frame):
        raise RuntimeError("native_run_interrupted")
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGHUP, interrupted)
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--compositor", type=Path, required=True)
    parser.add_argument("--daemon", type=Path)
    parser.add_argument("--case", choices=["baseline", "daemon-unavailable", "reconnect-empty", "reconnect-draft", "message-success", "message-failure", "message-unconfirmed", "many-sessions", "render-burst", "diagnostics-offline", "diagnostics-ready"], required=True)
    parser.add_argument("--out", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
