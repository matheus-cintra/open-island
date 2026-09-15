"""Native render-budget probes for the isolated Linux WebView."""
import hashlib
import json
from pathlib import Path

from native_linux import Child, Driver, has_visible_content, wait_for


def render_burst(driver, daemon, env, home, out, cleanup, children, result, check):
    """Send repeated equal snapshots through the real event path.

    The daemon fixture owns one session so this also checks that a draft and
    caret survive the burst. The event payload is captured once and reused;
    no synthetic session or direct DOM paint is involved.
    """
    result["status"] = "FAIL"
    qa_env = {**env, "OPEN_ISLAND_QA_SESSIONS": "1"}

    def start():
        child = Child([str(daemon), "--socket", qa_env["OPEN_ISLAND_SOCKET"]], qa_env,
                      out / "daemon-render.log", cleanup)
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
        if len(snapshot["snapshot"]["sessions"]) != 1:
            return None
        return value if driver.script(
            "const input = document.querySelector('.message-input'); "
            "return !!input && !input.disabled && input.getBoundingClientRect().height > 0;"
        ) else None

    server = start()
    wait_for(lambda: (home / "daemon.sock").is_socket(), children)
    state = wait_for(hydrated, children, timeout=30)
    draft = "Rascunho preservado durante a rajada."
    prepared = driver.script(
        "const input = document.querySelector('.message-input'); "
        f"input.value = {json.dumps(draft)}; "
        "input.dispatchEvent(new InputEvent('input', {bubbles: true, inputType: 'insertText'})); "
        "input.focus(); input.setSelectionRange(8, 17); "
        "return {value: input.value, start: input.selectionStart, end: input.selectionEnd, "
        "active: document.activeElement === input};"
    )
    check(prepared["value"] == draft and prepared["active"], "draft_is_ready_before_equal_snapshot_burst")

    # Let the real row/badge enter animations and the expanded window morph
    # settle before measuring the equal-state path. Those are the visual edges
    # of the preceding hydration, not work caused by the burst itself.
    driver.script(
        "const done = arguments[arguments.length - 1]; setTimeout(() => done(true), 1200);",
        True,
    )

    observation = driver.script(
        "const done = arguments[arguments.length - 1]; "
        "(async () => { "
        "  const surface = document.querySelector('#expanded-view'); "
        "  const input = document.querySelector('.message-input'); "
        "  let mutations = 0; let frames = 0; const mutationKinds = {}; const mutationDetails = []; "
        "  const observer = new MutationObserver(records => { "
        "    mutations += records.length; "
        "    for (const record of records) { const key = record.type + ':' + (record.attributeName || record.target.nodeName); mutationKinds[key] = (mutationKinds[key] || 0) + 1; "
        "      if (mutationDetails.length < 80) { const target = record.target.nodeType === Node.ELEMENT_NODE ? record.target : record.target.parentElement; mutationDetails.push({type: record.type, attribute: record.attributeName, oldValue: record.oldValue, target: target?.outerHTML?.slice(0, 280)}); } "
        "    } "
        "  }); "
        "  observer.observe(surface, {subtree: true, childList: true, attributes: true, attributeOldValue: true, characterData: true, characterDataOldValue: true}); "
        "  const originalRaf = window.requestAnimationFrame; "
        "  window.requestAnimationFrame = callback => { frames += 1; return originalRaf.call(window, callback); }; "
        "  const snapshot = structuredClone(await window.__TAURI__.core.invoke('get_daemon_ui_state')); "
        "  const bursts = []; "
        "  for (let burst = 0; burst < 5; burst += 1) { "
        "    const before = {mutations, frames}; "
        "    const deliveries = []; "
        "    for (let index = 0; index < 100; index += 1) "
        "      deliveries.push(window.__TAURI__.event.emitTo('main', 'daemon-ui-state', snapshot)); "
        "    await Promise.all(deliveries); "
        "    await new Promise(resolve => originalRaf(() => originalRaf(resolve))); "
        "    bursts.push({mutations: mutations - before.mutations, frames: frames - before.frames}); "
        "  } "
        "  await new Promise(resolve => originalRaf(() => originalRaf(resolve))); "
        "  observer.disconnect(); window.requestAnimationFrame = originalRaf; "
        "  return {bursts, mutations, mutationKinds, frames, value: input.value, start: input.selectionStart, "
        "    end: input.selectionEnd, active: document.activeElement === input, mutationDetails, "
        "    sessionRows: document.querySelectorAll('.session-row').length}; "
        "})().then(done).catch(error => done({error: String(error)}));",
        True,
    )
    if "error" in observation:
        raise RuntimeError(observation["error"])
    result["render_observation"] = observation
    check(observation["value"] == draft, "draft_survives_five_hundred_equal_snapshots")
    check(observation["start"] == prepared["start"] and observation["end"] == prepared["end"],
          "caret_selection_survives_equal_snapshot_burst")
    check(observation["active"], "editor_focus_survives_equal_snapshot_burst")
    check(observation["sessionRows"] == 1, "equal_snapshot_burst_keeps_one_session_row")
    check(sum(burst["mutations"] for burst in observation["bursts"]) == 0,
          "equal_snapshot_burst_has_no_dom_mutations")
    check(all(burst["frames"] <= 4 for burst in observation["bursts"]),
          "equal_snapshot_burst_coalesces_animation_frames")

    png, width, height = driver.screenshot()
    (out / "render-burst.png").write_bytes(png)
    check(has_visible_content(png), "render_burst_native_capture_is_not_blank")
    result.update(
        status="PASS",
        burst_count=5,
        snapshots_per_burst=100,
        render_observation=observation,
        capture_size=[width, height],
        capture_sha256=hashlib.sha256(png).hexdigest(),
    )
    server.stop()
    children.remove(server)
