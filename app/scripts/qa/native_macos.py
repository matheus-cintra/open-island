#!/usr/bin/env python3
"""Run the QA WebView in an isolated native macOS GUI session."""
import argparse
import base64
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time
import urllib.request
import zlib


FULL_CASES = [
    "baseline", "daemon-unavailable", "reconnect-empty", "reconnect-draft",
    "message-success", "message-failure", "message-unconfirmed", "many-sessions",
    "render-burst", "diagnostics-offline", "diagnostics-ready",
]
DAEMON_CASES = set(FULL_CASES[2:])


def digest(path):
    with path.open("rb") as source:
        return hashlib.sha256(source.read()).hexdigest()


def _png_chunks(png):
    if png[:8] != b"\x89PNG\r\n\x1a\n":
        raise RuntimeError("invalid_native_png")
    offset = 8
    chunks = []
    while offset + 12 <= len(png):
        length = int.from_bytes(png[offset:offset + 4], "big")
        end = offset + 12 + length
        if end > len(png):
            raise RuntimeError("truncated_native_png")
        kind = png[offset + 4:offset + 8]
        chunks.append((kind, png[offset + 8:offset + 8 + length]))
        offset = end
        if kind == b"IEND":
            break
    return chunks


def _unfilter(row, previous, filter_type, bpp):
    output = bytearray(len(row))
    for index, value in enumerate(row):
        left = output[index - bpp] if index >= bpp else 0
        above = previous[index] if previous else 0
        upper_left = previous[index - bpp] if previous and index >= bpp else 0
        if filter_type == 0:
            result = value
        elif filter_type == 1:
            result = (value + left) & 0xff
        elif filter_type == 2:
            result = (value + above) & 0xff
        elif filter_type == 3:
            result = (value + ((left + above) // 2)) & 0xff
        elif filter_type == 4:
            estimate = left + above - upper_left
            distances = (abs(estimate - left), abs(estimate - above), abs(estimate - upper_left))
            predictor = (left, above, upper_left)[distances.index(min(distances))]
            result = (value + predictor) & 0xff
        else:
            raise RuntimeError("unsupported_native_png_filter")
        output[index] = result
    return output


def png_info(png):
    chunks = _png_chunks(png)
    header = next((value for kind, value in chunks if kind == b"IHDR"), None)
    if header is None or len(header) != 13:
        raise RuntimeError("missing_native_png_header")
    width = int.from_bytes(header[:4], "big")
    height = int.from_bytes(header[4:8], "big")
    depth, color, compression, filtering, interlace = header[8:]
    if not (100 <= width <= 8192 and 20 <= height <= 4320):
        raise RuntimeError("unexpected_native_png_dimensions")
    data = b"".join(value for kind, value in chunks if kind == b"IDAT")
    if not data:
        raise RuntimeError("empty_native_png")
    if depth != 8 or interlace != 0 or color not in (2, 6):
        return width, height, bool(data)
    bpp = 3 if color == 2 else 4
    raw = zlib.decompress(data)
    stride = width * bpp
    if len(raw) != (stride + 1) * height:
        raise RuntimeError("invalid_native_png_scanlines")
    previous = None
    visible = 0
    offset = 0
    for _ in range(height):
        filter_type = raw[offset]
        offset += 1
        row = _unfilter(raw[offset:offset + stride], previous, filter_type, bpp)
        offset += stride
        for pixel in range(0, stride, bpp):
            if color == 6:
                alpha = row[pixel + 3]
                luminance = (row[pixel] * 299 + row[pixel + 1] * 587 + row[pixel + 2] * 114) // 1000
                luminance = luminance * alpha // 255
            else:
                luminance = (row[pixel] * 299 + row[pixel + 1] * 587 + row[pixel + 2] * 114) // 1000
            visible += luminance > 12
        previous = row
    return width, height, visible >= 20


def has_visible_content(png, bounds=None):
    if bounds is not None:
        # The WebDriver screenshot is already scoped to the WebView. Bounds are
        # retained in the report; decoding the full PNG avoids a second image
        # implementation whose coordinate rounding could hide a real failure.
        del bounds
    return png_info(png)[2]


class Child:
    def __init__(self, command, env, log, cleanup):
        output = cleanup.enter_context(log.open("wb"))
        self.process = subprocess.Popen(command, env=env, stdin=subprocess.DEVNULL,
                                        stdout=output, stderr=output, start_new_session=True)
        cleanup.callback(self.stop)

    def stop(self):
        if self.process.poll() is not None:
            return
        try:
            os.killpg(self.process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(self.process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            self.process.wait(timeout=3)


def wait_for(check, children, timeout=10):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if any(child.process.poll() is not None for child in children):
            raise RuntimeError("owned_process_exited")
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise RuntimeError("native_condition_timeout")


def owns_listener(pid, port):
    try:
        result = subprocess.run(
            ["lsof", "-nP", "-a", "-p", str(pid), "-iTCP:" + str(port),
             "-sTCP:LISTEN", "-F", "n"], capture_output=True, text=True,
            timeout=2, check=False,
        )
        return result.returncode == 0 and (":" + str(port)) in result.stdout
    except (OSError, subprocess.TimeoutExpired):
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
        width, height, _ = png_info(png)
        return png, width, height


def app_command(app):
    if app.is_dir():
        executable = app / "Contents" / "MacOS" / "open-island"
        if not executable.is_file():
            candidates = list((app / "Contents" / "MacOS").glob("*"))
            executable = next((path for path in candidates if os.access(path, os.X_OK)), executable)
        return [str(executable)]
    return [str(app)]


def private_environment(home, config):
    env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(home), "LANG": "C.UTF-8",
           "OPEN_ISLAND_QA": "1", "OPEN_ISLAND_QA_CREDENTIALS_BLOCKED": "1",
           "OPEN_ISLAND_SOCKET": str(home / "daemon.sock"), "OPEN_ISLAND_CONFIG": str(config)}
    for key in ["XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR", "XDG_CACHE_HOME"]:
        path = home / key.lower()
        path.mkdir(mode=0o700)
        env[key] = str(path)
    return env


def run(args):
    app = args.app.resolve(strict=True)
    qa_target = Path(__file__).resolve().parents[2] / "target/portable-qa"
    if not app.is_relative_to(qa_target):
        raise ValueError("application_must_come_from_portable_qa_build")
    daemon = args.daemon.resolve(strict=True) if args.daemon else None
    if args.case in DAEMON_CASES and (daemon is None or not daemon.is_relative_to(qa_target)):
        raise ValueError("daemon_must_come_from_portable_qa_build")
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False, mode=0o700)
    executable = Path(app_command(app)[0])
    result = {"status": "FAIL", "case": args.case, "platform": "macos", "assertions": [],
              "app_sha256": digest(executable),
              "capture_source": "WKWebView WebDriver screenshot; native macOS GUI",
              "physical_input_tested": False, "virtual_input_tested": "WebDriver DOM events"}
    if daemon:
        result["daemon_sha256"] = digest(daemon)

    def check(condition, name):
        result["assertions"].append({"name": name, "pass": bool(condition)})
        if not condition:
            raise RuntimeError(name)

    children = []
    with tempfile.TemporaryDirectory(prefix="oi-native-macos-") as directory, ExitStack() as cleanup:
        home = Path(directory).resolve()
        config_file = home / "config.json"
        configuration = {"usage": {"show_limits": False}, "updates": {"check_enabled": False},
                         "integrations": {"auto_configure": False}, "sound": {"enabled": False}}
        if args.case in {"message-success", "message-failure", "message-unconfirmed"}:
            configuration["sessions"] = {"idle_after_ms": 1000}
        config_file.write_text(json.dumps(configuration))
        config_file.chmod(0o600)
        env = private_environment(home, config_file)
        result["fixture_config"] = configuration

        def start_daemon(number, extra=None):
            daemon_env = {**env, **(extra or {})}
            child = Child([str(daemon), "--socket", daemon_env["OPEN_ISLAND_SOCKET"]], daemon_env,
                          out / f"daemon-{number}.log", cleanup)
            children.append(child)
            return child

        def cache(driver):
            return driver.script("const done = arguments[arguments.length - 1]; "
                                 "window.__TAURI__.core.invoke('get_daemon_ui_state').then(done).catch(() => done({}));", True)

        def hydrated(driver, expected=None):
            value = cache(driver)
            snapshot = value.get("snapshot")
            if value.get("phase") != "connected" or not snapshot:
                return None
            sessions = snapshot["snapshot"]["sessions"]
            if expected is not None and len(sessions) != expected:
                return None
            usable = driver.script("return [...document.querySelectorAll('.message-input')].every(input => !input.disabled && input.getBoundingClientRect().height > 0);")
            return value if usable else None

        try:
            if daemon:
                help_probe = Child([str(daemon), "--help"], env, out / "daemon-help.txt", cleanup)
                children.append(help_probe)
                help_probe.process.wait(timeout=5)
                help_text = (out / "daemon-help.txt").read_text()
                check(help_probe.process.returncode == 0 and "--version" in help_text, "daemon_advertises_safe_version_option")
                children.remove(help_probe)
                version_probe = Child([str(daemon), "--version"], env, out / "daemon-version.txt", cleanup)
                children.append(version_probe)
                version_probe.process.wait(timeout=5)
                info = (out / "daemon-version.txt").read_text().strip()
                check(version_probe.process.returncode == 0 and "qa-harness=registered-only" in info, "daemon_version_probe_is_qa_only")
                children.remove(version_probe)
                result["daemon_version"] = info

            with socket.socket() as reservation:
                reservation.bind(("127.0.0.1", 0))
                port = reservation.getsockname()[1]
            env["OPEN_ISLAND_QA_WEBDRIVER_PORT"] = str(port)
            application = Child(app_command(app), env, out / "application.log", cleanup)
            children.append(application)
            wait_for(lambda: owns_listener(application.process.pid, port), children, timeout=30)
            driver = Driver(port)
            result["capabilities"] = driver.connect()
            identity = driver.script("const done = arguments[arguments.length - 1]; "
                                     "window.__TAURI__.app.getIdentifier().then(done);", True)
            check(isinstance(identity, str) and identity.startswith("app.open-island.qa."), "qa_bundle_identifier")
            result["identifier"] = identity
            wait_for(lambda: driver.script("return document.readyState === 'complete' && !!document.querySelector('.daemon-connection-status');"), children, timeout=30)
            driver.script("const done = arguments[arguments.length - 1]; "
                          "window.__TAURI__.event.emitTo('main', 'island-toggle', {}).then(() => done(true));", True)
            def observation():
                value = driver.script("const status = document.querySelector('.daemon-connection-status'); "
                                      "const rect = status?.getBoundingClientRect(); return {text: status?.textContent, "
                                      "visible: !!status && !status.hidden && document.querySelector('#island').classList.contains('expanded'), "
                                      "contained: !!rect && rect.width > 0 && rect.height > 0 && rect.left >= 0 && rect.top >= 0 && rect.right <= innerWidth && rect.bottom <= innerHeight, "
                                      "muted: document.querySelector('#header-mute')?.disabled, editors: document.querySelectorAll('.message-input').length, width: innerWidth, height: innerHeight};")
                result["last_observation"] = value
                return value if value.get("visible") and value.get("contained") and value.get("text") == "Conectando ao Open Island…" else None
            initial = wait_for(observation, children, timeout=20)
            check(initial["muted"], "offline_actions_are_disabled")
            check(initial["editors"] == 0, "offline_boot_has_no_invented_session")
            check(not (home / "daemon.sock").exists(), "qa_app_did_not_spawn_daemon")
            png, width, height = driver.screenshot()
            (out / "island.png").write_bytes(png)
            check(has_visible_content(png), "native_capture_is_not_blank")
            check(width == initial["width"] and height == initial["height"], "capture_matches_webview_geometry")
            result.update(png_sha256=hashlib.sha256(png).hexdigest(), png_size=[width, height], status="PASS")

            if args.case == "reconnect-empty":
                first_daemon = start_daemon(1)
                first = wait_for(lambda: hydrated(driver, 0), children, timeout=35)
                check(not first["snapshot"]["snapshot"]["sessions"], "first_daemon_hydrates_empty_session_list")
                first_daemon.stop()
                children.remove(first_daemon)
                wait_for(lambda: cache(driver).get("phase") == "reconnecting", children, timeout=15)
                (home / "daemon.sock").unlink(missing_ok=True)
                second_daemon = start_daemon(2)
                second = wait_for(lambda: hydrated(driver, 0), children, timeout=35)
                check(second["generation"] > first["generation"], "shell_connection_generation_advances")
                check(second["snapshot"]["snapshot"]["daemon_epoch"] != first["snapshot"]["snapshot"]["daemon_epoch"], "new_daemon_epoch_replaces_old_cache")
                result["connection_observations"] = [{"generation": first["generation"], "epoch": first["snapshot"]["snapshot"]["daemon_epoch"]},
                                                       {"generation": second["generation"], "epoch": second["snapshot"]["snapshot"]["daemon_epoch"]}]
                second_daemon.stop()
                children.remove(second_daemon)

            if args.case == "many-sessions":
                server = start_daemon(1, {"OPEN_ISLAND_QA_SESSIONS": "50"})
                state = wait_for(lambda: hydrated(driver, 50), children, timeout=45)
                observation = driver.script("const inputs = [...document.querySelectorAll('.message-input')]; "
                                            "const last = inputs[inputs.length - 1]; last.focus(); const list = document.querySelector('#session-list'); "
                                            "return {editors: inputs.length, active: document.activeElement === last, scrollHeight: list?.scrollHeight ?? 0, clientHeight: list?.clientHeight ?? 0};")
                check(len(state["snapshot"]["snapshot"]["sessions"]) == 50, "authoritative_snapshot_contains_fifty_sessions")
                check(observation["editors"] == 50 and observation["active"], "fifty_session_composers_and_focus_are_preserved")
                check(observation["scrollHeight"] >= observation["clientHeight"], "session_list_retains_bounded_scroll_region")
                driver.script("const list = document.querySelector('#session-list'); if (list) list.scrollTop = list.scrollHeight; return true;")
                png, width, height = driver.screenshot()
                (out / "many-sessions.png").write_bytes(png)
                check(has_visible_content(png), "many_sessions_native_capture_is_not_blank")
                result.update(session_count=50, capture_size=[width, height])
                server.stop()
                children.remove(server)

            if args.case == "render-burst":
                server = start_daemon(1, {"OPEN_ISLAND_QA_SESSIONS": "1"})
                wait_for(lambda: hydrated(driver, 1), children, timeout=35)
                draft = "Rascunho preservado durante a rajada."
                prepared = driver.script("const input = document.querySelector('.message-input'); "
                                         f"input.value = {json.dumps(draft)}; input.dispatchEvent(new InputEvent('input', {{bubbles: true, inputType: 'insertText'}})); "
                                         "input.focus(); return {value: input.value, active: document.activeElement === input};")
                check(prepared["value"] == draft and prepared["active"], "draft_is_ready_before_equal_snapshot_burst")
                observation = driver.script("const done = arguments[arguments.length - 1]; (async () => { "
                    "const surface = document.querySelector('#expanded-view'); let mutations = 0; let frames = 0; "
                    "const observer = new MutationObserver(records => { mutations += records.length; }); observer.observe(surface, {subtree: true, childList: true, attributes: true, characterData: true}); "
                    "const originalRaf = window.requestAnimationFrame; window.requestAnimationFrame = callback => { frames += 1; return originalRaf.call(window, callback); }; "
                    "const snapshot = structuredClone(await window.__TAURI__.core.invoke('get_daemon_ui_state')); const bursts = []; "
                    "for (let burst = 0; burst < 5; burst += 1) { const before = {mutations, frames}; const emits = []; "
                    "for (let index = 0; index < 100; index += 1) emits.push(window.__TAURI__.event.emitTo('main', 'daemon-ui-state', snapshot)); await Promise.all(emits); "
                    "await new Promise(resolve => originalRaf(() => originalRaf(resolve))); bursts.push({mutations: mutations - before.mutations, frames: frames - before.frames}); } "
                    "observer.disconnect(); window.requestAnimationFrame = originalRaf; const input = document.querySelector('.message-input'); "
                    "return {bursts, value: input.value, active: document.activeElement === input, sessionRows: document.querySelectorAll('.session-row').length}; "
                    "})().then(done).catch(error => done({error: String(error)}));", True)
                if "error" in observation:
                    raise RuntimeError(observation["error"])
                check(observation["value"] == draft and observation["active"], "draft_and_focus_survive_equal_snapshot_burst")
                check(observation["sessionRows"] == 1, "equal_snapshot_burst_keeps_one_session_row")
                check(sum(item["mutations"] for item in observation["bursts"]) == 0, "equal_snapshot_burst_has_no_dom_mutations")
                check(all(item["frames"] <= 4 for item in observation["bursts"]), "equal_snapshot_burst_coalesces_animation_frames")
                png, width, height = driver.screenshot()
                (out / "render-burst.png").write_bytes(png)
                check(has_visible_content(png), "render_burst_native_capture_is_not_blank")
                result.update(render_observation=observation, capture_size=[width, height])
                server.stop()
                children.remove(server)

            if args.case in {"reconnect-draft", "message-success", "message-failure", "message-unconfirmed"}:
                outcome = {"message-failure": "rejected", "message-unconfirmed": "lost_ack"}.get(args.case, "delivered")
                server = start_daemon(1, {"OPEN_ISLAND_QA_SESSIONS": "1", "OPEN_ISLAND_QA_DELIVERY": outcome})
                first = wait_for(lambda: hydrated(driver, 1), children, timeout=35)
                old_session = first["snapshot"]["snapshot"]["sessions"][0]
                draft = "Rascunho de QA: preservar antes de enviar."
                driver.script("const input = document.querySelector('.message-input'); "
                              f"input.value = {json.dumps(draft)}; input.dispatchEvent(new InputEvent('input', {{bubbles: true, inputType: 'insertText'}})); input.focus(); return true;")
                wait_for(lambda: driver.script("return document.querySelector('.message-input')?.value;") == draft, children)
                server.stop()
                children.remove(server)
                wait_for(lambda: cache(driver).get("phase") == "reconnecting", children, timeout=15)
                (home / "daemon.sock").unlink(missing_ok=True)
                server = start_daemon(2, {"OPEN_ISLAND_QA_SESSIONS": "1", "OPEN_ISLAND_QA_DELIVERY": outcome})
                second = wait_for(lambda: hydrated(driver, 1), children, timeout=35)
                new_session = second["snapshot"]["snapshot"]["sessions"][0]
                check(new_session["id"] == old_session["id"] and new_session["session_instance_id"] != old_session["session_instance_id"], "reconnect_replaces_session_guard_identity")
                check(driver.script("return document.querySelector('.message-input')?.value;") == draft, "draft_survives_rehydration")
                if args.case != "reconnect-draft":
                    submitted = driver.script("const input = document.querySelector('.message-input'); input.dispatchEvent(new KeyboardEvent('keydown', {key: 'Enter', bubbles: true})); return true;")
                    check(submitted, "explicit_enter_event_is_accepted_by_composer")
                    wait_for(lambda: next((item for item in cache(driver)["snapshot"]["snapshot"]["message_deliveries"] if item["state"] == "queued"), None), children, timeout=12)
                    socket_path = Path(env["OPEN_ISLAND_SOCKET"])
                    with socket.socket(socket.AF_UNIX) as channel:
                        channel.settimeout(3)
                        channel.connect(str(socket_path))
                        channel.sendall((json.dumps({"v": 1, "id": 1, "method": "hook_event", "params": {"agent": "claude", "session_id": new_session["id"], "event": "stop", "pid": new_session["pid"], "cwd": str(home)}}) + "\n").encode())
                        with channel.makefile("rb") as reader:
                            check(json.loads(reader.readline(262144)).get("ok") is True, "owned_session_stop_is_acknowledged")
                    terminal = "delivered" if args.case == "message-success" else ("failed" if args.case == "message-failure" else "unconfirmed")
                    record = wait_for(lambda: next((item for item in cache(driver)["snapshot"]["snapshot"]["message_deliveries"] if item["state"] == terminal), None), children, timeout=20)
                    check(record["text"] == draft, "terminal_delivery_preserves_text")
                    received = home / "qa-session-0.jsonl"
                    lines = received.read_text().splitlines() if received.exists() else []
                    check(len(lines) == (0 if terminal == "failed" else 1), "helper_receipt_matches_transport_outcome")
                    recovery = driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('get_message_recovery').then(done);", True)
                    check(len(recovery) == (0 if terminal == "delivered" else 1), "shell_recovery_matches_terminal_delivery")
                    if terminal != "delivered":
                        driver.script("window.qaBeforeReload = true; setTimeout(() => location.reload(), 0); return true;")
                        wait_for(lambda: driver.script("return window.qaBeforeReload ? null : document.querySelector('.shell-message-recovery textarea')?.value;") == draft, children, timeout=20)
                        check(driver.script("return document.querySelector('.shell-message-recovery textarea')?.readOnly === true;"), "recovered_text_is_read_only_after_reload")
                    png, width, height = driver.screenshot()
                    (out / "delivery.png").write_bytes(png)
                    check(has_visible_content(png), "delivery_native_capture_is_not_blank")
                result.update(status="PASS", case_outcome=outcome)
                server.stop()
                children.remove(server)

            if args.case in {"diagnostics-offline", "diagnostics-ready"}:
                ready = args.case == "diagnostics-ready"
                server = start_daemon(1) if ready else None
                if ready:
                    wait_for(lambda: (home / "daemon.sock").is_socket(), children)
                before = config_file.read_bytes()
                driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('open_settings').then(() => done(true));", True)
                settings = Driver(port)
                settings.connect("settings")
                wait_for(lambda: settings.script("return !!document.querySelector('[data-pane=about]');"), children, timeout=20)
                settings.script("document.querySelector('[data-pane=about]').click(); return true;")
                wait_for(lambda: settings.script("return [...document.querySelectorAll('button')].some(button => button.textContent === 'Atualizar diagnóstico');"), children)
                settings.script("[...document.querySelectorAll('button')].find(button => button.textContent === 'Atualizar diagnóstico').click(); return true;")
                expected = "Conectado" if ready else "Daemon indisponível"
                wait_for(lambda: settings.script("return [...document.querySelectorAll('.row-value')].map(node => node.textContent);").count(expected) == 1, children, timeout=15)
                check(True, "settings_displays_actual_daemon_diagnostic_state")
                settings.script("[...document.querySelectorAll('button')].find(button => button.textContent === 'Copiar relatório').click(); return true;")
                wait_for(lambda: settings.script("return document.querySelector('#toast')?.textContent;") == "Relatório copiado.", children, timeout=8)
                clipboard = subprocess.run(["pbpaste"], env=env, capture_output=True, timeout=5, check=False)
                report = json.loads(clipboard.stdout)
                check(report["daemon"]["state"] == ("ready" if ready else "unavailable"), "copied_report_matches_displayed_connection")
                check(str(home) not in clipboard.stdout.decode() and "transcript" not in report, "copied_report_excludes_private_paths_and_transcript")
                check(report["model"] == "unavailable" and report.get("audio", {}).get("capture_tested") is False, "diagnostics_distinguish_unchecked_audio_and_model")
                check(before == config_file.read_bytes(), "diagnostics_leave_configuration_unchanged")
                png, width, height = settings.screenshot()
                (out / "settings.png").write_bytes(png)
                check(has_visible_content(png), "settings_native_capture_is_not_blank")
                settings.request("DELETE", f"/session/{settings.session}")
                if server:
                    server.stop()
                    children.remove(server)

            driver.request("DELETE", f"/session/{driver.session}")
        except Exception as error:
            result.update(status="FAIL", error=type(error).__name__, detail=str(error)[:300])
        finally:
            for child in reversed(children):
                child.stop()
            result["children_reaped"] = all(child.process.poll() is not None for child in children)
            if not result["children_reaped"] or not result["assertions"]:
                result["status"] = "FAIL"
            (out / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--app", type=Path, required=True)
    parser.add_argument("--daemon", type=Path)
    parser.add_argument("--case", choices=FULL_CASES, required=True)
    parser.add_argument("--out", type=Path, required=True)
    raise SystemExit(run(parser.parse_args()))
