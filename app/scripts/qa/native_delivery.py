import json
import socket
import time
from pathlib import Path
from native_linux import Child, wait_for, has_visible_content


def explicit_delivery(driver, session, draft, cache, env, home, out, cleanup, children, result, check, restart):
    result["status"] = "FAIL"
    driver.script("window.qaInputEvents = []; return true;")
    rect = driver.script("const r = document.querySelector('.message-input').getBoundingClientRect(); return [innerWidth, r.x + r.width / 2, r.y + r.height / 2];")
    x, y = round((1280 - rect[0]) / 2 + rect[1]), round(rect[2])
    keyboard = Child(["wtype", "-s", "1000", "-k", "Return", "-s", "20000"], env, out / "submit-keyboard.log", cleanup)
    children.append(keyboard)
    time.sleep(0.2)
    pointer_path = Path(__file__).resolve().parents[2] / "target/qa-pointer/pointer"
    pointer = Child([str(pointer_path), str(x), str(y)], env, out / "submit-pointer.log", cleanup)
    children.append(pointer)
    pointer.process.wait(timeout=5)
    check(pointer.process.returncode == 0 and keyboard.process.poll() is None, "explicit_enter_uses_private_native_input")
    children.remove(pointer)
    result["submission_observation"] = driver.script("return {focused: document.hasFocus(), active: document.activeElement?.className, text: document.querySelector('.message-input').value, events: window.qaInputEvents, body: document.body.innerText};")

    def delivery(state):
        value = cache()
        records = value["snapshot"]["snapshot"]["message_deliveries"]
        result["last_delivery_observation"] = records
        return next((record for record in records if record["state"] == state), None)

    queued = wait_for(lambda: delivery("queued"), children, timeout=8)
    keyboard.stop()
    children.remove(keyboard)
    check(queued["text"] == draft and queued["session_id"] == session["id"], "queued_delivery_preserves_text_and_destination")
    check(driver.script("return document.querySelector('.message-input').value;") == "", "editor_clears_only_after_submission_admission")
    check(not list(home.glob("qa-session-*.jsonl")), "busy_helper_receives_nothing_before_idle")
    def recovery():
        return driver.script("const done = arguments[arguments.length - 1]; window.__TAURI__.core.invoke('get_message_recovery').then(done);", True)
    reserved = recovery()
    check(len(reserved) == 1 and reserved[0]["text"] == draft and reserved[0]["client_submission_id"] == queued["client_submission_id"], "shell_retains_admitted_text_before_confirmation")
    with socket.socket(socket.AF_UNIX) as channel:
        channel.settimeout(3)
        channel.connect(env["OPEN_ISLAND_SOCKET"])
        channel.sendall(json.dumps({"v": 1, "id": 1, "method": "hook_event", "params": {
            "agent": "claude", "session_id": session["id"], "event": "stop",
            "pid": session["pid"], "cwd": str(home)
        }}).encode() + b"\n")
        with channel.makefile("rb") as reader:
            response = json.loads(reader.readline(262144))
        check(response.get("id") == 1 and response.get("ok") is True, "owned_session_stop_is_acknowledged")
    if result["case"] != "message-success":
        failed_delivery(driver, draft, queued, delivery, recovery, home, out, children, result, check, restart)
        return
    delivered = wait_for(lambda: delivery("delivered"), children, timeout=15)
    wait_for(lambda: driver.script("return !!document.querySelector('.message-delivery[data-state=delivered]');"), children)
    check(True, "webview_renders_confirmed_delivery")
    records = [json.loads(line) for line in (home / "qa-session-0.jsonl").read_text().splitlines()]
    check(len(records) == 1 and records[0]["text"] == draft, "helper_receives_exact_text_once")
    check(records[0]["pid"] == session["pid"] and records[0].get("expected_process_identity") is not None, "bridge_receives_current_guarded_process_identity")
    check(delivered["message_id"] == queued["message_id"], "same_message_reaches_delivered_state")
    wait_for(lambda: recovery() == [], children)
    check(True, "confirmed_delivery_releases_shell_recovery_slot")
    (out / "helper-deliveries.json").write_text(json.dumps(records, indent=2) + "\n")
    driver.script("const done = arguments[arguments.length - 1]; Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect.getComputedTiming().endTime)).map(a => a.finished.catch(() => {}))).then(() => requestAnimationFrame(() => requestAnimationFrame(() => done(true))));", True)
    bounds = driver.script("const r = document.querySelector('.message-delivery[data-state=delivered]').getBoundingClientRect(); return [r.left + 3, r.top + 3, r.right - 3, r.bottom - 3];")
    png, width, height = driver.screenshot()
    (out / "delivered.png").write_bytes(png)
    check(has_visible_content(png, bounds), "native_capture_shows_delivery_confirmation")
    result["delivery_capture_size"] = [width, height]
    started = time.monotonic()
    wait_for(lambda: time.monotonic() - started >= 5.2, children, timeout=7)
    check(len((home / "qa-session-0.jsonl").read_text().splitlines()) == 1, "confirmed_delivery_is_not_repeated_after_two_poll_intervals")
    result.update(status="PASS", delivered_message_id=delivered["message_id"])


def failed_delivery(driver, draft, queued, delivery, recovery, home, out, children, result, check, restart):
    state = "failed" if result["case"] == "message-failure" else "unconfirmed"
    terminal = wait_for(lambda: delivery(state), children, timeout=15)
    check(terminal["message_id"] == queued["message_id"] and terminal["text"] == draft, "unsuccessful_delivery_preserves_message_identity_and_text")
    wait_for(lambda: driver.script(f"return !!document.querySelector('.message-delivery[data-state={state}]');"), children)
    check(True, "webview_renders_unsuccessful_delivery")
    retained = wait_for(lambda: next((r for r in recovery() if r["last_state"] == state), None), children)
    check(retained["text"] == draft and retained["client_submission_id"] == queued["client_submission_id"], "shell_preserves_failed_or_uncertain_text")
    def received():
        path = home / "qa-session-0.jsonl"
        return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    expected_count = 0 if state == "failed" else 1
    records = received()
    check(len(records) == expected_count, "helper_receipt_matches_injected_transport_outcome")
    if records:
        check(records[0]["text"] == draft, "lost_ack_occurs_after_helper_receives_exact_text")
    (out / "helper-deliveries.json").write_text(json.dumps(records, indent=2) + "\n")
    (out / "recovery-before-reload.json").write_text(json.dumps(recovery(), indent=2) + "\n")
    # Reload the actual WebView, preserving the native shell and real connection.
    driver.script("window.qaBeforeReload = true; setTimeout(() => location.reload(), 0); return true;")
    def reloaded():
        try:
            return driver.script("const el = document.querySelector('.shell-message-recovery textarea'); return window.qaBeforeReload ? null : el?.value;") == draft
        except (RuntimeError, OSError):
            return False
    wait_for(reloaded, children, timeout=15)
    after = recovery()
    check(after == [retained], "webview_reload_reads_same_shell_recovery_record")
    check(driver.script("return document.querySelector('.shell-message-recovery textarea').readOnly;"), "recovered_text_is_read_only")
    check(not driver.script("return !!document.querySelector('.message-delivery[data-state=delivered]');"), "uncertain_or_failed_delivery_never_claims_success")
    driver.script("const done = arguments[arguments.length - 1]; if (!document.querySelector('#island').classList.contains('expanded')) window.__TAURI__.event.emitTo('main', 'island-toggle', {}).then(() => done(true)); else done(true);", True)
    driver.script("document.querySelector('.shell-message-recovery textarea').scrollIntoView({block: 'center', behavior: 'instant'}); return true;")
    driver.script("const done = arguments[arguments.length - 1]; Promise.all(document.getAnimations().filter(a => Number.isFinite(a.effect.getComputedTiming().endTime)).map(a => a.finished.catch(() => {}))).then(() => requestAnimationFrame(() => requestAnimationFrame(() => done(true))));", True)
    bounds = driver.script("const r = document.querySelector('.shell-message-recovery textarea').getBoundingClientRect(); return [r.left + 4, r.top + 3, r.right - 4, r.bottom - 3];")
    result["recovery_capture_scroll"] = "scrollIntoView on recovery text inside bounded support panel"
    result["recovery_text_bounds"] = bounds
    png, width, height = driver.screenshot()
    (out / "recovery.png").write_bytes(png)
    check(has_visible_content(png, bounds), "native_capture_shows_recoverable_text_after_reload")
    started = time.monotonic()
    wait_for(lambda: time.monotonic() - started >= 5.2, children, timeout=7)
    check(received() == records, "failed_or_uncertain_delivery_is_not_retried_after_reload")
    next_cache = restart()
    next_epoch = next_cache["snapshot"]["snapshot"]["daemon_epoch"]
    previous = recovery()
    check(next_epoch != retained["origin_epoch"], "recovery_reconnect_has_new_daemon_epoch")
    check(len(previous) == 1 and previous[0]["text"] == draft and previous[0]["client_submission_id"] == retained["client_submission_id"], "restart_preserves_exact_recovery_text_and_submission")
    check(previous[0]["local"] and previous[0]["previous_epoch"] and previous[0]["previous_state"] == state and previous[0]["last_state"] == "unconfirmed", "previous_connection_record_is_local_and_never_rebound")
    check(not next_cache["snapshot"]["snapshot"]["message_deliveries"], "new_daemon_does_not_adopt_previous_delivery")
    wait_for(lambda: driver.script("return document.querySelector('.shell-message-recovery textarea')?.value;") == draft, children)
    started = time.monotonic()
    wait_for(lambda: time.monotonic() - started >= 5.2, children, timeout=7)
    check(received() == records, "restart_never_replays_failed_or_uncertain_delivery")
    (out / "recovery-after-restart.json").write_text(json.dumps(previous, indent=2) + "\n")
    result.update(status="PASS", recovery_capture_size=[width, height], terminal_state=state)
