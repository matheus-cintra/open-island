import hashlib
import json
import socket
import struct
from pathlib import Path
from native_linux import Child, Driver, has_visible_content, wait_for


def focus_settings(runtime):
    compositor_command(runtime, b'[title="^Open Island Settings$"] focus')


def compositor_command(runtime, command):
    address = next(Path(runtime).glob("sway-ipc.*.sock"))
    with socket.socket(socket.AF_UNIX) as channel:
        channel.settimeout(2)
        channel.connect(str(address))
        channel.sendall(b"i3-ipc" + struct.pack("<II", len(command), 0) + command)
        def read(length):
            data = bytearray()
            while len(data) < length:
                block = channel.recv(length - len(data))
                if not block:
                    raise RuntimeError("private_compositor_closed_reply")
                data.extend(block)
            return bytes(data)
        header = read(14)
        length, kind = struct.unpack("<II", header[6:])
        if header[:6] != b"i3-ipc" or kind != 0 or length > 65536:
            raise RuntimeError("invalid_private_compositor_reply")
        reply = json.loads(read(length))
        if not reply or not all(entry.get("success") is True for entry in reply):
            raise RuntimeError("private_compositor_focus_failed")


def diagnostics_case(main, port, daemon, env, home, out, cleanup, children, result, check):
    result["status"] = "FAIL"
    result["settings_focus_source"] = "private Sway IPC command"
    result["copy_input_source"] = "private Wayland virtual keyboard"
    ready = result["case"] == "diagnostics-ready"
    if ready:
        server = Child([str(daemon), "--socket", env["OPEN_ISLAND_SOCKET"]], env, out / "daemon.log", cleanup)
        children.append(server)
        wait_for(lambda: (home / "daemon.sock").is_socket(), children)
    before = (home / "config.json").read_bytes()
    main.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('open_settings').then(() => done(true));", True)
    settings = Driver(port)
    settings.connect("settings")
    wait_for(lambda: settings.script("return !!document.querySelector('[data-pane=about]');"), children, timeout=20)
    settings.script("document.querySelector('[data-pane=about]').click(); return true;")
    wait_for(lambda: settings.script("return [...document.querySelectorAll('button')].some(b => b.textContent === 'Atualizar diagnóstico');"), children)
    settings.script("[...document.querySelectorAll('button')].find(b => b.textContent === 'Atualizar diagnóstico').click(); return true;")
    expected = "Conectado" if ready else "Daemon indisponível"
    wait_for(lambda: settings.script("return [...document.querySelectorAll('.row-value')].map(n => n.textContent);").count(expected) == 1, children, timeout=15)
    check(True, "settings_displays_actual_daemon_diagnostic_state")
    settings.script("[...document.querySelectorAll('button')].find(b => b.textContent === 'Copiar relatório').focus(); return true;")
    keyboard = Child(["wtype", "-s", "1000", "-k", "Return", "-s", "250"], env, out / "keyboard.log", cleanup)
    children.append(keyboard)
    import time
    time.sleep(0.2)
    focus_settings(env["XDG_RUNTIME_DIR"])
    wait_for(lambda: settings.script("return document.hasFocus();"), children, timeout=2)
    check(True, "settings_has_focus_before_copy_key")
    keyboard.process.wait(timeout=5)
    check(keyboard.process.returncode == 0, "copy_activated_by_private_wayland_keyboard")
    children.remove(keyboard)
    result["copy_keyboard_observation"] = settings.script("return {focused: document.hasFocus(), active: document.activeElement?.textContent, toast: document.querySelector('#toast')?.textContent};")
    wait_for(lambda: settings.script("return document.querySelector('#toast')?.textContent;") == "Relatório copiado.", children)
    clipboard = Child(["wl-paste", "--no-newline", "--type", "text"], env, out / "clipboard.json", cleanup)
    children.append(clipboard)
    clipboard.process.wait(timeout=5)
    check(clipboard.process.returncode == 0, "private_wayland_clipboard_read_succeeds")
    children.remove(clipboard)
    raw = (out / "clipboard.json").read_text()
    report = json.loads(raw)
    check(report["schema_version"] == 1 and report["daemon"]["state"] == ("ready" if ready else "unavailable"), "copied_report_matches_displayed_connection")
    check(str(home) not in raw and str(daemon) not in raw and "transcript" not in report, "copied_report_excludes_private_paths_and_transcript")
    check(report["app_version"] is not None and report["model"] == "unavailable", "shell_reports_app_and_unconfigured_model")
    check(report.get("audio") == {"alsa_capture_devices": None, "capture_tested": False}, "qa_diagnostic_does_not_probe_host_audio_or_claim_capture")
    check(settings.script("return [...document.querySelectorAll('.row-value')].some(n => n.textContent === 'Entrada e captura ainda não verificadas');"), "settings_distinguishes_unchecked_audio_from_available_input")
    if ready:
        check(report["daemon"]["pid"] == server.process.pid, "report_identifies_owned_running_daemon")
        check(len(report["daemon"]["capabilities"]) == 4 and report["daemon"]["epoch"] is not None, "report_contains_running_protocol_capabilities")
    else:
        check(not (home / "daemon.sock").exists(), "offline_settings_diagnostics_do_not_spawn_daemon")
    check(before == (home / "config.json").read_bytes(), "diagnostics_leave_configuration_unchanged")
    settings.script("[...document.querySelectorAll('button')].find(b => b.textContent === 'Atualizar diagnóstico').closest('.section').scrollIntoView({block:'center'}); return true;")
    settings.script("const done = arguments[arguments.length - 1]; requestAnimationFrame(() => requestAnimationFrame(() => done(true)));", True)
    png, width, height = settings.screenshot()
    (out / "settings.png").write_bytes(png)
    check(has_visible_content(png), "settings_native_capture_is_not_blank")
    result["settings_capture"] = {"sha256": hashlib.sha256(png).hexdigest(), "size": [width, height]}
    result["diagnostic_clipboard_sha256"] = hashlib.sha256(raw.encode()).hexdigest()
    settings.request("DELETE", f"/session/{settings.session}")
    result["status"] = "PASS"
