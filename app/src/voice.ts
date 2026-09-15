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
const editors = new WeakMap<HTMLButtonElement, Editor>();
function buttons(): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>(".voice-toggle")) {
    const editor = editors.get(button);
    const own = sameVoiceTarget(editor?.target() ?? null, voice.state.target);
    const stoppable = voice.state.phase === "recording" || voice.state.phase === "requesting_permission";
    const disabled = voice.active() ? !own || voice.starting || !stoppable : !editor?.target();
    if (button.disabled !== disabled) button.disabled = disabled;
    const recording = own && voice.active() && stoppable;
    const symbol = recording ? "stop" : "microphone";
    if (button.dataset.symbol !== symbol) {
      button.innerHTML = recording ? STOP_RECORDING : MICROPHONE;
      button.dataset.symbol = symbol;
    }
    const label = recording ? strings.voice.stop : strings.voice.start;
    if (button.getAttribute("aria-label") !== label) button.setAttribute("aria-label", label);
    const pressed = String(recording);
    if (button.getAttribute("aria-pressed") !== pressed) button.setAttribute("aria-pressed", pressed);
    if (button.title !== label) button.title = label;
  }
}
export function attachVoice(box: HTMLElement, composer: MessageComposer, editor: Editor, actions: Actions): void {
  const button = document.createElement("button");
  button.type = "button"; button.className = "voice-toggle";
  editors.set(button, editor);
  const select = (): void => voice.select({ ...editor, target: (): VoiceTarget | null => box.isConnected ? editor.target() : null, insert: (text): boolean => box.isConnected && !!editor.target() && composer.insertTranscript(text) });
  box.querySelector("textarea")?.addEventListener("focus", select);
  button.addEventListener("click", (): void => {
    select();
    if (voice.active()) { void voice.stop().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); return; }
    const target = editor.target();
    if (!target || !box.isConnected) return;
    const revision = composer.draftRevision();
    void voice.start(target, (text): boolean => box.isConnected && sameVoiceTarget(target, editor.target()) && composer.insertTranscript(text, revision))
      .catch((error): void => actions.error(strings.voice.error(reasonOf(error))));
  });
  box.append(button);
  updateVoiceBox(box);
}
export function updateVoiceBox(box: HTMLElement): void {
  const button = box.querySelector<HTMLButtonElement>(".voice-toggle");
  if (!button) return;
  if (box.isConnected) {
    buttons();
    return;
  }
  const editor = editors.get(button);
  const disabled = !editor?.target() || voice.active();
  if (button.disabled !== disabled) button.disabled = disabled;
  const label = strings.voice.start;
  if (button.getAttribute("aria-label") !== label) button.setAttribute("aria-label", label);
  if (button.title !== label) button.title = label;
  if (button.dataset.symbol !== "microphone") {
    button.innerHTML = MICROPHONE;
    button.dataset.symbol = "microphone";
  }
}

export function initializeVoice(root: HTMLElement, actions: Actions & { isMac(): boolean }): void {
  root.classList.add("voice-panel");
  const details = document.createElement("details");
  const summary = document.createElement("summary"); summary.textContent = strings.voice.title;
  const explanation = document.createElement("p"); explanation.textContent = strings.voice.explanation;
  const model = document.createElement("button"); model.type = "button"; model.textContent = strings.voice.model;
  let picking = false;
  let modelRevision = 0;
  const modelStatus = document.createElement("span"); modelStatus.textContent = strings.voice.unavailable;
  const privacy = document.createElement("button"); privacy.type = "button"; privacy.textContent = strings.voice.privacy; privacy.hidden = true;
  details.append(summary, explanation, model, modelStatus, privacy);
  details.addEventListener("toggle", (): void => actions.resize());
  model.addEventListener("click", (): void => {
    picking = true; model.disabled = true;
    void invoke<{ configured: boolean } | null>("plugin:voice|voice_select_model").then((status): void => {
      if (status) { modelRevision += 1; modelStatus.textContent = status.configured ? strings.voice.configured : strings.voice.unavailable; }
    }).catch((error): void => actions.error(strings.voice.error(reasonOf(error))))
      .finally((): void => { picking = false; model.disabled = voice.active(); actions.resize(); });
  });
  privacy.addEventListener("click", (): void => {
    void invoke("plugin:voice|voice_open_microphone_settings").catch((error): void => actions.error(strings.voice.error(reasonOf(error))));
  });
  void invoke<{ configured: boolean }>("plugin:voice|voice_model_status").then((status): void => {
    if (modelRevision === 0) modelStatus.textContent = status.configured ? strings.voice.configured : strings.voice.unavailable;
  }).catch((): void => {});
  const live = document.createElement("section");
  const status = document.createElement("span"); status.setAttribute("aria-live", "polite");
  const time = document.createElement("span");
  const cancel = document.createElement("button"); cancel.type = "button"; cancel.textContent = strings.voice.cancel;
  const stop = document.createElement("button"); stop.type = "button"; stop.textContent = strings.voice.stop;
  stop.addEventListener("click", (): void => { void voice.stop().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); });
  cancel.addEventListener("click", (): void => { void voice.cancel().catch((error): void => actions.error(strings.voice.error(reasonOf(error)))); });
  live.append(status, time, stop, cancel);
  const list = document.createElement("div");
  root.append(details, live, list);
  const entries = new Map<string, { element: HTMLElement; input: HTMLTextAreaElement; label: HTMLElement; insert: HTMLButtonElement }>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let polling = false;
  const render = (): void => {
    buttons();
    model.disabled = voice.active() || picking;
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
