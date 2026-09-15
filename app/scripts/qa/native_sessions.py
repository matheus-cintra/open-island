from pathlib import Path
import json
import socket
import hashlib
from native_linux import Child, Driver, wait_for, has_visible_content
from native_diagnostics import focus_settings


def many_sessions(driver, daemon, env, home, out, cleanup, children, result, check):
    """Hydrate the real UI with the maximum QA helper family and retain focus."""
    result["status"] = "FAIL"
    qa_env = {**env, "OPEN_ISLAND_QA_SESSIONS": "50"}

    def start():
        child = Child([str(daemon), "--socket", qa_env["OPEN_ISLAND_SOCKET"]], qa_env,
                      out / "daemon-many.log", cleanup)
        children.append(child)
        return child

    def cache():
        return driver.script(
            "const done = arguments[arguments.length - 1]; "
            "window.__TAURI__.core.invoke('get_daemon_ui_state').then(done);", True)

    def hydrated():
        value = cache()
        snapshot = value.get("snapshot")
        if value.get("phase") != "connected" or not snapshot:
            return None
        sessions = snapshot["snapshot"]["sessions"]
        editors = driver.script(
            "return {count: document.querySelectorAll('.message-input').length, "
            "height: document.querySelector('#session-list')?.getBoundingClientRect().height ?? 0};")
        return value if len(sessions) == 50 and editors["count"] == 50 and editors["height"] > 0 else None

    daemon_process = start()
    hydrated_state = wait_for(hydrated, children, timeout=35)
    sessions = hydrated_state["snapshot"]["snapshot"]["sessions"]
    identities = [session.get("session_instance_id") for session in sessions]
    check(len(sessions) == 50, "authoritative_snapshot_contains_fifty_sessions")
    check(len(set(session["id"] for session in sessions)) == 50, "fifty_session_ids_are_unique")
    check(None not in identities and len(set(identities)) == 50, "fifty_action_identities_are_unique")
    observation = driver.script(
        "const inputs = [...document.querySelectorAll('.message-input')]; "
        "const target = inputs[inputs.length - 1]; target.focus(); "
        "return {editors: inputs.length, active: document.activeElement === target, "
        "scrollHeight: document.querySelector('#session-list')?.scrollHeight ?? 0, "
        "clientHeight: document.querySelector('#session-list')?.clientHeight ?? 0};")
    check(observation["editors"] == 50, "fifty_session_composers_are_rendered")
    check(observation["active"], "last_session_composer_accepts_focus")
    check(observation["scrollHeight"] >= observation["clientHeight"], "session_list_retains_bounded_scroll_region")
    for _ in range(3):
        cache()
    check(driver.script("return document.activeElement?.classList.contains('message-input') === true;"),
          "focus_survives_authoritative_reads")
    driver.script(
        "const done = arguments[arguments.length - 1]; "
        "const list = document.querySelector('#session-list'); "
        "if (list) list.scrollTop = list.scrollHeight; "
        "Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect?.getComputedTiming()?.endTime)).map(a => a.finished.catch(() => {})))"
        ".then(() => requestAnimationFrame(() => requestAnimationFrame(() => done(true))));", True)
    visible = driver.script(
        "const rows = [...document.querySelectorAll('.session-row')]; "
        "const visible = rows.filter(row => { const r = row.getBoundingClientRect(); return r.bottom > 0 && r.top < innerHeight && r.width > 0 && r.height > 0; }); "
        "return {count: visible.length, text: visible[visible.length - 1]?.textContent ?? '', "
        "bounds: visible[visible.length - 1]?.getBoundingClientRect().toJSON() ?? null};")
    check(visible["count"] > 0 and visible["text"], "last_sessions_are_visible_after_scroll")
    png, width, height = driver.screenshot()
    (out / "many-sessions.png").write_bytes(png)
    check(has_visible_content(png), "many_sessions_native_capture_is_not_blank")
    bounds = visible["bounds"]
    if bounds is not None:
        check(has_visible_content(png, (max(0, round(bounds["left"])), max(0, round(bounds["top"])),
                                      min(width, round(bounds["right"])), min(height, round(bounds["bottom"])))),
              "native_capture_shows_a_scrolled_session")
    result.update(status="PASS", session_count=50, editor_count=observation["editors"],
                  list_scroll=[observation["scrollHeight"], observation["clientHeight"]],
                  capture_size=[width, height])
    daemon_process.stop()
    children.remove(daemon_process)


def reconnect_draft(driver, daemon, env, home, out, cleanup, children, result, check):
    result["status"] = "FAIL"
    env = {**env, "OPEN_ISLAND_QA_SESSIONS": "1",
        "OPEN_ISLAND_QA_DELIVERY": {"message-failure": "rejected", "message-unconfirmed": "lost_ack"}.get(result["case"], "delivered")}
    result["helper_outcome"] = env["OPEN_ISLAND_QA_DELIVERY"]
    def start(number):
        child = Child([str(daemon), "--socket", env["OPEN_ISLAND_SOCKET"]], env, out / f"daemon-{number}.log", cleanup)
        children.append(child)
        return child

    def cache():
        return driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('get_daemon_ui_state').then(done);", True)

    def hydrated():
        value = cache()
        snapshot = value.get("snapshot")
        if value["phase"] != "connected" or not snapshot:
            return None
        sessions = snapshot["snapshot"]["sessions"]
        result["last_session_observation"] = [{key: session.get(key) for key in ["id", "pid", "send_channel", "send_blocked", "session_instance_id"]} for session in sessions]
        usable = driver.script("const input = document.querySelector('.message-input'); return !!input && !input.disabled && input.getBoundingClientRect().height > 0;")
        return value if len(sessions) == 1 and usable else None

    first_daemon = start(1)
    first = wait_for(hydrated, children, timeout=25)
    session = first["snapshot"]["snapshot"]["sessions"][0]
    check(session["pid"] != first_daemon.process.pid, "session_belongs_to_owned_helper_not_daemon")
    check(Path(f"/proc/{first_daemon.process.pid}/task/{first_daemon.process.pid}/children").read_text().split().count(str(session["pid"])) == 1, "session_helper_is_direct_daemon_child")
    with socket.socket(socket.AF_UNIX) as bridge:
        bridge.settimeout(2)
        bridge.connect(str(home / "qa-session-0.sock"))
        bridge.sendall(json.dumps({"pid": session["pid"], "text": "must reject an unguarded request"}).encode() + b"\n")
        with bridge.makefile("rb") as reply:
            response = json.loads(reply.readline(65536))
        check(response["outcome"] == "rejected" and response["error_code"] == "stale_session", "helper_rejects_delivery_without_process_guard")
    draft = "Rascunho de QA: preservar antes de enviar."
    driver.script("window.qaInputEvents = []; for (const name of ['pointerdown','pointerup','focusin','focusout','keydown']) document.addEventListener(name, e => { if (window.qaInputEvents.length < 20) window.qaInputEvents.push([name, e.target.className, e.clientX, e.clientY]); }, true); return true;")
    last = None
    stable = 0
    def editor_position():
        nonlocal last, stable
        position = driver.script("const r = document.querySelector('.message-input').getBoundingClientRect(); return [innerWidth, innerHeight, r.x + r.width / 2, r.y + r.height / 2];")
        stable = stable + 1 if position == last else 1
        last = position
        return position if stable >= 3 else None
    width, height, x, y = wait_for(editor_position, children)
    x = round((1280 - width) / 2 + x)
    y = round(y)
    check(0 <= x < 1280 and 0 <= y < min(height, 720), "editor_pointer_target_inside_private_output")
    keyboard = Child(["wtype", "-s", "1000", draft, "-s", "20000"], env, out / "typing.log", cleanup)
    children.append(keyboard)
    import time
    time.sleep(0.2)
    pointer_path = Path(__file__).resolve().parents[2] / "target/qa-pointer/pointer"
    result["pointer_sha256"] = hashlib.sha256(pointer_path.read_bytes()).hexdigest()
    pointer = Child([str(pointer_path), str(x), str(y)], env, out / "pointer.log", cleanup)
    children.append(pointer)
    pointer.process.wait(timeout=5)
    check(pointer.process.returncode == 0, "editor_clicked_using_private_wayland_pointer")
    children.remove(pointer)
    result["pointer_target"] = [x, y]
    result["typing_observation"] = driver.script("const input = document.querySelector('.message-input'); return {focused: document.hasFocus(), active: document.activeElement === input, text: input.value, events: window.qaInputEvents};")
    wait_for(lambda: driver.script("return document.querySelector('.message-input')?.value;") == draft, children)
    check(sum(event[0] == "pointerdown" for event in result["typing_observation"]["events"]) == 1, "draft_accepts_typing_after_one_pointer_click")
    driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('open_settings').then(() => done(true));", True)
    settings = Driver(int(driver.url.rsplit(":", 1)[1]))
    settings.connect("settings")
    focus_settings(env["XDG_RUNTIME_DIR"])
    wait_for(lambda: settings.script("return document.hasFocus();"), children, timeout=5)
    check(not driver.script("return document.hasFocus();"), "island_releases_keyboard_to_another_window")
    settings.request("DELETE", f"/session/{settings.session}")
    keyboard.stop()
    children.remove(keyboard)
    first_daemon.stop()
    children.remove(first_daemon)
    wait_for(lambda: cache()["phase"] == "reconnecting" and driver.script("return document.querySelector('.message-input')?.disabled;"), children)
    check(driver.script("return document.querySelector('.message-input')?.value;") == draft, "draft_survives_daemon_disconnect")
    check(not Path(f"/proc/{session['pid']}").exists(), "first_helper_reaped_with_daemon")
    (home / "daemon.sock").unlink(missing_ok=True)
    second_daemon = start(2)
    second = wait_for(hydrated, children, timeout=25)
    next_session = second["snapshot"]["snapshot"]["sessions"][0]
    check(second["generation"] > first["generation"], "reconnect_generation_advances")
    check(second["snapshot"]["snapshot"]["daemon_epoch"] != first["snapshot"]["snapshot"]["daemon_epoch"], "reconnect_epoch_changes")
    check(next_session["id"] == session["id"] and next_session["session_instance_id"] != session["session_instance_id"], "same_logical_session_has_new_guard_identity")
    check(driver.script("return document.querySelector('.message-input')?.value;") == draft, "draft_survives_rehydration_with_new_identity")
    check(not list(home.glob("qa-session-*.jsonl")), "draft_is_never_automatically_delivered")
    driver.script("const done = arguments[arguments.length - 1]; Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect.getComputedTiming().endTime)).map(a => a.finished.catch(() => {}))).then(() => requestAnimationFrame(() => requestAnimationFrame(() => done(true))));", True)
    bounds = driver.script("const r = document.querySelector('.message-input').getBoundingClientRect(); return [r.left + 4, r.top + 3, r.right - 4, r.bottom - 3];")
    png, width, height = driver.screenshot()
    (out / "draft.png").write_bytes(png)
    check(has_visible_content(png), "draft_native_capture_is_not_blank")
    check(has_visible_content(png, bounds), "retained_draft_is_visible_in_native_capture")
    result.update(status="PASS", draft_capture_size=[width, height], typing_source="private Wayland virtual keyboard")
    if result["case"] in ["message-success", "message-failure", "message-unconfirmed"]:
        from native_delivery import explicit_delivery
        def restart():
            second_daemon.stop()
            children.remove(second_daemon)
            wait_for(lambda: cache()["phase"] == "reconnecting", children)
            check(not Path(f"/proc/{next_session['pid']}").exists(), "delivery_helper_reaped_on_restart")
            (home / "daemon.sock").unlink(missing_ok=True)
            start(3)
            return wait_for(hydrated, children, timeout=25)
        explicit_delivery(driver, next_session, draft, cache, env, home, out, cleanup, children, result, check, restart)
