import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { reasonOf } from "./json";
import type { MessageComposer } from "./message-controller";
import { strings } from "./strings";
import { MICROPHONE, STOP_RECORDING } from "./voice-icons";
import { VoiceController, sameVoiceTarget, type VoiceState, type VoiceTarget } from "./voice-controller";

export const voice = new VoiceController(
  <T>(command: string, args?: Record<string, unknown>): Promise<T> => invoke<T>(`plugin:voice|${command}`, args),
  (callback): Promise<() => void> => listen<VoiceState>("voice-state", (event): void => callback(event.payload)),
);
interface Actions { resize(): void; error(message: string): void; }
interface Editor { target(): VoiceTarget | null; label(): string; }
interface Composer { editor: Editor; canvas: HTMLCanvasElement; input: HTMLTextAreaElement; }
const composers = new WeakMap<HTMLButtonElement, Composer>();
let modelConfigured: boolean | null = null;
const levels = { job: "", value: 0 };
export function voiceLevel(job: string | null): number {
  return job !== null && levels.job === job && voice.state.phase === "recording" ? levels.value : 0;
}
function buttons(): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>(".voice-toggle")) {
    const composer = composers.get(button);
    const own = sameVoiceTarget(composer?.editor.target() ?? null, voice.state.target);
    const stoppable = voice.state.phase === "recording" || voice.state.phase === "requesting_permission";
    const unconfigured = !voice.active() && modelConfigured === false;
    const disabled = voice.active() ? !own || voice.starting || !stoppable : !composer?.editor.target();
    if (button.disabled !== disabled) button.disabled = disabled;
    button.classList.toggle("is-unconfigured", unconfigured);
    if (button.getAttribute("aria-disabled") !== String(unconfigured)) button.setAttribute("aria-disabled", String(unconfigured));
    const recording = own && voice.active() && stoppable;
    const symbol = recording ? "stop" : "microphone";
    if (button.dataset.symbol !== symbol) {
      button.innerHTML = recording ? STOP_RECORDING : MICROPHONE;
      button.dataset.symbol = symbol;
    }
    const label = unconfigured ? strings.voice.modelRequired : recording ? strings.voice.stop : strings.voice.start;
    if (button.getAttribute("aria-label") !== label) button.setAttribute("aria-label", label);
    const pressed = String(recording);
    if (button.getAttribute("aria-pressed") !== pressed) button.setAttribute("aria-pressed", pressed);
    if (button.title !== label) button.title = label;
  }
}
export function attachVoice(box: HTMLElement, composer: MessageComposer, editor: Editor, actions: Actions): void {
  const button = document.createElement("button");
  button.type = "button"; button.className = "voice-toggle";
  const canvas = document.createElement("canvas");
  canvas.className = "voice-wave"; canvas.setAttribute("aria-hidden", "true"); canvas.hidden = true;
  const input = box.querySelector<HTMLTextAreaElement>("textarea")!;
  const field = box.querySelector<HTMLElement>(".message-field")!;
  composers.set(button, { editor, canvas, input });
  const select = (): void => voice.select({ ...editor, target: (): VoiceTarget | null => box.isConnected ? editor.target() : null, insert: (text): boolean => box.isConnected && !!editor.target() && composer.insertTranscript(text) });
  input.addEventListener("focus", select);
  button.addEventListener("click", (): void => {
    select();
    if (voice.active()) { void voice.stop().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); return; }
    if (modelConfigured === false) {
      void invoke("open_settings", { pane: "voice" }).catch((error): void => actions.error(strings.voice.error(reasonOf(error))));
      return;
    }
    const target = editor.target();
    if (!target || !box.isConnected) return;
    const revision = composer.draftRevision();
    void voice.start(target, (text): boolean => box.isConnected && sameVoiceTarget(target, editor.target()) && composer.insertTranscript(text, revision))
      .catch((error): void => actions.error(strings.voice.error(reasonOf(error))));
  });
  field.append(canvas, button);
  updateVoiceBox(box);
}
export function updateVoiceBox(box: HTMLElement): void {
  const button = box.querySelector<HTMLButtonElement>(".voice-toggle");
  if (!button) return;
  const composer = composers.get(button);
  const unconfigured = !voice.active() && modelConfigured === false;
  button.classList.toggle("is-unconfigured", unconfigured);
  if (button.getAttribute("aria-disabled") !== String(unconfigured)) button.setAttribute("aria-disabled", String(unconfigured));
  if (box.isConnected) {
    buttons();
    return;
  }
  const disabled = !composer?.editor.target() || voice.active();
  if (button.disabled !== disabled) button.disabled = disabled;
  const label = unconfigured ? strings.voice.modelRequired : strings.voice.start;
  if (button.getAttribute("aria-label") !== label) button.setAttribute("aria-label", label);
  if (button.title !== label) button.title = label;
  if (button.dataset.symbol !== "microphone") {
    button.innerHTML = MICROPHONE;
    button.dataset.symbol = "microphone";
  }
}

const wave = { canvas: null as HTMLCanvasElement | null, level: 0, frame: 0 };
function reducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}
function startWave(): void {
  if (wave.frame === 0) wave.frame = requestAnimationFrame(tick);
}
function stopWave(): void {
  if (wave.frame !== 0) cancelAnimationFrame(wave.frame);
  wave.frame = 0;
  wave.canvas = null;
  wave.level = 0;
}
function tick(now: number): void {
  const canvas = wave.canvas;
  if (canvas === null || canvas.hidden) {
    wave.frame = 0;
    return;
  }
  const target = voice.state.job_id !== null && levels.job === voice.state.job_id ? levels.value : 0;
  wave.level += (target - wave.level) * (target > wave.level ? 0.4 : 0.1);
  canvas.dataset.level = wave.level.toFixed(3);
  draw(now);
  wave.frame = requestAnimationFrame(tick);
}
function draw(now: number): void {
  const canvas = wave.canvas;
  if (canvas === null) return;
  const context = canvas.getContext("2d");
  if (context === null) return;
  const scale = window.devicePixelRatio || 1;
  const width = canvas.clientWidth || canvas.parentElement?.clientWidth || 0;
  const height = canvas.clientHeight || canvas.parentElement?.clientHeight || 0;
  if (width < 4 || height < 4) return;
  if (canvas.width !== Math.round(width * scale)) canvas.width = Math.round(width * scale);
  if (canvas.height !== Math.round(height * scale)) canvas.height = Math.round(height * scale);
  context.setTransform(scale, 0, 0, scale, 0, 0);
  context.clearRect(0, 0, width, height);
  const level = Math.min(Math.max(wave.level, 0), 1);
  const middle = height / 2;
  const reach = middle - 2;
  const motion = reducedMotion() ? 0 : (now / 1000) * 2.2;
  const accent = getComputedStyle(document.documentElement).getPropertyValue("--accent").trim() || "#0a84ff";
  context.strokeStyle = accent;
  context.lineWidth = 1.4;
  context.lineCap = "round";
  context.beginPath();
  const span = width - 2;
  for (let x = 0; x <= span; x += 2) {
    const position = x / span;
    const envelope = Math.sin(Math.PI * position);
    const ripple = Math.sin(position * 9.4 + motion) * 0.55 + Math.sin(position * 21.6 - motion * 1.7) * 0.45;
    const y = middle + ripple * envelope * level * reach;
    if (x === 0) context.moveTo(x, y); else context.lineTo(x, y);
  }
  context.stroke();
}
function syncWaves(): void {
  let active: HTMLCanvasElement | null = null;
  for (const button of document.querySelectorAll<HTMLButtonElement>(".voice-toggle")) {
    const composer = composers.get(button);
    if (!composer) continue;
    const live = composer.canvas.isConnected
      && sameVoiceTarget(composer.editor.target(), voice.state.target)
      && voice.state.phase === "recording" && !voice.starting && voice.state.worker_active;
    composer.canvas.hidden = !live;
    composer.input.classList.toggle("is-recording", live);
    if (live) active = composer.canvas;
  }
  if (active === null) {
    stopWave();
    return;
  }
  if (wave.canvas !== active) wave.canvas = active;
  startWave();
}

export function initializeVoice(root: HTMLElement, actions: Actions & { isMac(): boolean }): void {
  root.classList.add("voice-panel");
  const live = document.createElement("section");
  live.className = "voice-live";
  const status = document.createElement("span"); status.setAttribute("aria-live", "polite");
  const time = document.createElement("span");
  const cancel = document.createElement("button"); cancel.type = "button"; cancel.textContent = strings.voice.cancel;
  const stop = document.createElement("button"); stop.type = "button"; stop.textContent = strings.voice.stop;
  stop.addEventListener("click", (): void => { void voice.stop().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); });
  cancel.addEventListener("click", (): void => { void voice.cancel().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); });
  const privacy = document.createElement("button"); privacy.type = "button"; privacy.textContent = strings.voice.privacy; privacy.hidden = true;
  privacy.addEventListener("click", (): void => {
    void invoke("plugin:voice|voice_open_microphone_settings").catch((error): void => actions.error(strings.voice.error(reasonOf(error))));
  });
  live.append(status, time, cancel, stop, privacy);
  const list = document.createElement("div");
  root.append(live, list);
  void invoke<{ configured: boolean }>("plugin:voice|voice_model_status").then((status): void => {
    modelConfigured = status.configured;
    render();
  }).catch((): void => {});
  void listen<{ configured: boolean }>("voice-model-status", (event): void => {
    modelConfigured = event.payload.configured;
    render();
  });
  void listen<{ job_id: string; level: number }>("voice-level", (event): void => {
    if (typeof event.payload?.job_id === "string" && Number.isFinite(event.payload.level)) {
      levels.job = event.payload.job_id;
      levels.value = Math.min(Math.max(event.payload.level, 0), 1);
    }
  });
  const entries = new Map<string, { element: HTMLElement; input: HTMLTextAreaElement; label: HTMLElement; insert: HTMLButtonElement }>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let polling = false;
  const render = (): void => {
    buttons();
    syncWaves();
    live.hidden = (voice.state.phase === "idle" || voice.state.phase === "cancelled") && !voice.active();
    const cancelling = voice.state.phase === "cancelled" && voice.state.worker_active;
    const label = cancelling ? strings.voice.cancelling : voice.state.error ? strings.voice.error(voice.state.error) : strings.voice.phase[voice.starting ? "requesting_permission" : voice.state.phase];
    const caption = voice.state.target ? ` — ${voice.state.target.session_id}` : "";
    if (status.textContent !== label + caption) status.textContent = label + caption;
    const seconds = Math.floor(voice.state.recorded_ms / 1000);
    const elapsed = voice.state.phase === "recording" ? ` ${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}` : "";
    if (time.textContent !== elapsed) time.textContent = elapsed;
    cancel.hidden = !voice.active() || cancelling;
    stop.hidden = !voice.active() || (voice.state.phase !== "requesting_permission" && voice.state.phase !== "recording");
    privacy.hidden = !actions.isMac() || voice.state.error !== "microphone_permission_denied";
    for (const [id, entry] of entries) if (!voice.results.some((result) => result.id === id)) { entry.element.remove(); entries.delete(id); }
    for (const result of voice.results) {
      let entry = entries.get(result.id);
      if (!entry) {
        const element = document.createElement("section"); element.className = "voice-result";
        const label = document.createElement("p");
        const input = document.createElement("textarea"); input.readOnly = true; input.setAttribute("aria-label", strings.voice.text); input.value = result.text;
        const copy = document.createElement("button"); copy.type = "button"; copy.textContent = strings.session.messageCopy;
        copy.addEventListener("click", (): void => { void navigator.clipboard.writeText(result.text).catch((): void => { input.focus(); input.select(); actions.error(strings.session.messageCopyFailed); }); });
        const insert = document.createElement("button"); insert.type = "button";
        insert.addEventListener("click", (): void => { if (!voice.insert(result.id)) actions.error(strings.voice.chooseTarget); });
        const discard = document.createElement("button"); discard.type = "button"; discard.textContent = strings.voice.dismiss;
        discard.addEventListener("click", (): void => { void voice.discard(result.id).catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); });
        element.append(label, input, copy, insert, discard); list.append(element);
        entry = { element, input, label, insert }; entries.set(result.id, entry);
      }
      entry.label.textContent = `${result.target.session_id}: ${result.applied ? strings.voice.applied : result.target_unavailable ? strings.voice.targetUnavailable : strings.voice.separate}`;
      entry.insert.disabled = !voice.destination?.target();
      entry.insert.textContent = voice.destination?.target() ? strings.voice.insert(voice.destination.label()) : strings.voice.chooseTarget;
    }
    actions.resize();
    if (!voice.active() && timer !== undefined) { clearTimeout(timer); timer = undefined; }
    if (voice.active() && timer === undefined && !polling) {
      timer = setTimeout((): void => {
        timer = undefined; polling = true;
        void voice.refresh().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))).finally((): void => { polling = false; render(); });
      }, voice.state.phase === "recording" ? 1000 : 5000);
    }
  };
  voice.subscribe(render);
  document.addEventListener("keydown", (event): void => {
    if (event.key !== "Escape" || !voice.active()) return;
    event.preventDefault(); event.stopImmediatePropagation();
    void voice.cancel().catch((error): void => actions.error(strings.voice.error(reasonOf(error))));
  }, true);
  render();
  void voice.connect().catch((): void => {});
}
