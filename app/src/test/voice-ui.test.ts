import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";
import type { VoiceState, VoiceTarget } from "../voice-controller";
import { strings } from "../strings";

let revision = 0; let jobs = 0;
let modelConfigured = true;
let backend: VoiceState = { revision, phase: "idle", job_id: null, target: null, recorded_ms: 0, worker_active: false, transcript: null, error: null };
const tauri = tauriMock(({ command, args }) => {
  if (command === "get_message_recovery" || command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  if (command === "plugin:voice|voice_model_status") return { configured: modelConfigured, error: modelConfigured ? null : "model_unavailable" };
  if (command === "plugin:voice|voice_start") {
    backend = { ...backend, revision: ++revision, job_id: `job-${++jobs}`, target: args.target as VoiceTarget, phase: "recording", worker_active: true, transcript: null };
    return backend;
  }
  if (command === "plugin:voice|voice_state") return backend;
  if (command === "plugin:voice|voice_cancel") {
    backend = { ...backend, revision: ++revision, phase: "cancelled", worker_active: false, transcript: null };
    return backend;
  }
  return {};
});
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
const main = await import("../main");
const { voice, voiceLevel } = await import("../voice");
const settle = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 20));
await settle();
function row(id: string) {
  tauri.state.sessions([{ id, agent: "codex", cwd: "/fixture", title: id, pid: 10, terminal: "kitty", send_channel: "tmux",
    action_identity: { daemon_epoch: "fixture-epoch", session_instance_id: `${id}-instance` } }]);
  if (!main.expanded) tauri.emit("island-toggle", {});
  const element = document.querySelector<HTMLLIElement>("#session-list > li")!;
  return { element, input: element.querySelector<HTMLTextAreaElement>(".message-input")!, button: element.querySelector<HTMLButtonElement>(".voice-toggle")! };
}
async function ready(text: string, target_unavailable = false): Promise<void> {
  backend = { ...backend, revision: ++revision, phase: "ready", worker_active: false, transcript: text, target_unavailable };
  tauri.emit("voice-state", backend);
  await Promise.resolve();
}
async function clear(): Promise<void> { for (const result of [...voice.results]) await voice.discard(result.id); }

test("microphone activation captures displayed identity, keeps island open, and never sends", async () => {
  const editor = row("child");
  editor.input.value = "Pedido:"; editor.input.dispatchEvent(new window.Event("input"));
  expect(editor.button.type).toBe("button"); expect(editor.button.disabled).toBe(false);
  editor.button.focus(); editor.button.click(); await settle();
  const start = tauri.calls.find((call) => call.command === "plugin:voice|voice_start")!;
  expect(start.args.target).toEqual({ session_id: "child", session_instance_id: "child-instance", daemon_epoch: "fixture-epoch" });
  expect(editor.button.getAttribute("aria-pressed")).toBe("true");
  tauri.emit("island-toggle", {}); await settle();
  expect(main.expanded).toBe(true);
  await ready("verifique os testes");
  expect(editor.input.value).toBe("Pedido: verifique os testes");
  expect(tauri.calls.filter((call) => call.command === "send_message_v2")).toHaveLength(0);
  expect(document.querySelector<HTMLTextAreaElement>(".voice-result textarea")!.value).toBe("verifique os testes");
  editor.element.remove(); await clear();
});

test("removed destination preserves text and requires selection plus explicit insertion", async () => {
  const old = row("old"); old.button.click(); await settle(); old.element.classList.add("is-leaving");
  await ready("texto para revisar");
  expect(old.input.value).toBe(""); old.element.remove();
  const result = document.querySelector<HTMLElement>(".voice-result")!;
  const insert = Array.from(result.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.includes("composer"))!;
  expect(insert.disabled).toBe(true);
  const selected = row("selected"); selected.input.focus();
  expect(insert.disabled).toBe(false); expect(insert.textContent).toBe("Inserir em selected");
  insert.click(); expect(selected.input.value).toBe("texto para revisar");
  expect(tauri.calls.filter((call) => call.command === "send_message_v2")).toHaveLength(0);
  selected.element.remove(); await clear();
});

test("Escape cancels the current job before the composer blur handler runs", async () => {
  const editor = row("escape"); editor.button.click(); await settle(); editor.button.focus();
  const event = new window.KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  editor.button.dispatchEvent(event); await settle();
  expect(event.defaultPrevented).toBe(true);
  expect(backend.phase).toBe("cancelled");
  expect(tauri.calls.filter((call) => call.command === "plugin:voice|voice_cancel").pop()?.args.jobId).toBe(backend.job_id);
  editor.element.remove();
});

test("unconfigured microphone opens the voice settings pane and follows model events", async () => {
  modelConfigured = false;
  tauri.emit("voice-model-status", { configured: false, error: "model_unavailable" });
  const editor = row("setup"); await settle();
  expect(editor.button.classList.contains("is-unconfigured")).toBe(true);
  expect(editor.button.getAttribute("aria-disabled")).toBe("true");
  expect(editor.button.title).toBe(strings.voice.modelRequired);
  const starts = tauri.calls.filter((call) => call.command === "plugin:voice|voice_start").length;
  editor.button.click(); await settle();
  expect(tauri.calls.find((call) => call.command === "open_settings")!.args).toEqual({ pane: "voice" });
  expect(tauri.calls.filter((call) => call.command === "plugin:voice|voice_start")).toHaveLength(starts);
  modelConfigured = true;
  tauri.emit("voice-model-status", { configured: true, error: null }); await settle();
  expect(editor.button.classList.contains("is-unconfigured")).toBe(false);
  expect(editor.button.getAttribute("aria-disabled")).toBe("false");
  editor.element.remove();
});

test("recording takes the field over with the voice wave and restores the draft", async () => {
  modelConfigured = true;
  tauri.emit("voice-model-status", { configured: true, error: null });
  const editor = row("wave");
  editor.input.value = "rascunho"; editor.input.dispatchEvent(new window.Event("input"));
  editor.button.click(); await settle();
  const canvas = editor.element.querySelector<HTMLCanvasElement>(".voice-wave")!;
  expect(canvas.hidden).toBe(false);
  expect(editor.input.classList.contains("is-recording")).toBe(true);
  tauri.emit("voice-level", { job_id: backend.job_id, level: 0.7 });
  expect(voiceLevel(backend.job_id)).toBe(0.7);
  tauri.emit("voice-level", { job_id: backend.job_id, level: 0.4 });
  expect(parseFloat(canvas.dataset.level!)).toBeGreaterThan(0);
  expect(parseFloat(canvas.dataset.level!)).toBeLessThan(0.7);
  await ready("ditado pronto");
  expect(canvas.hidden).toBe(true);
  expect(editor.input.classList.contains("is-recording")).toBe(false);
  expect(editor.input.value).toBe("rascunho ditado pronto");
  expect(voiceLevel(backend.job_id)).toBe(0);
  editor.element.remove(); await clear();
});

test("cancelled inference explains cleanup and prevents restart until the worker returns", async () => {
  const editor = row("cancelling"); editor.button.click(); await settle();
  backend = { ...backend, revision: ++revision, phase: "cancelled", worker_active: true, transcript: null };
  tauri.emit("voice-state", backend); await settle();
  const panel = document.querySelector<HTMLElement>(".voice-panel")!;
  expect(panel.textContent).toContain(strings.voice.cancelling);
  expect(editor.button.disabled).toBe(true);
  const controls = Array.from(panel.querySelectorAll<HTMLButtonElement>("button"));
  expect(controls.find((button) => button.textContent === strings.voice.cancel)!.hidden).toBe(true);
  expect(controls.find((button) => button.textContent === strings.voice.stop)!.hidden).toBe(true);
  expect(panel.querySelector(".voice-result")).toBeNull();
  backend = { ...backend, revision: ++revision, worker_active: false };
  tauri.emit("voice-state", backend); await settle();
  expect(editor.button.disabled).toBe(false);
  expect(panel.textContent).not.toContain(strings.voice.cancelling);
  editor.element.remove();
});

test("unavailable native target is explained and keeps insertion disabled until selection", async (): Promise<void> => {
  const editor = row("unavailable");
  editor.button.click(); await settle();
  await ready("texto conservado", true);
  expect(editor.input.value).toBe("");
  const result = document.querySelector<HTMLElement>(".voice-result")!;
  expect(result.textContent).toContain(strings.voice.targetUnavailable);
  const insert = Array.from(result.querySelectorAll<HTMLButtonElement>("button")).find((button): boolean => button.textContent === strings.voice.chooseTarget)!;
  expect(insert.disabled).toBe(true);
  editor.input.focus();
  expect(insert.disabled).toBe(false);
  insert.click();
  expect(editor.input.value).toBe("texto conservado");
  expect(tauri.calls.filter((call): boolean => call.command === "send_message_v2")).toHaveLength(0);
  editor.element.remove(); await clear();
});
