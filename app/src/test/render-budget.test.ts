import { expect, mock, test } from "bun:test";
import { mountIsland, paintFrame } from "./dom";
import { tauriMock } from "./tauri";
import { UsageLane } from "../usage-lane";
import type { UiCache } from "../daemon-state";
const tauri = tauriMock();
mock.module("@tauri-apps/api/core", (): object => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", (): object => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
const main = await import("../main");
await new Promise((resolve): void => { setTimeout(resolve, 30); });

test("200 equal snapshots preserve usage, delivery nodes and the draft selection", (): void => {
  const cache: UiCache = { phase: "connected", generation: 2, snapshot: { generation: 2, snapshot: {
    schema_version: 1, daemon_epoch: "budget", publication_revision: 1,
    sessions: [{ id: "session", agent: "codex", cwd: "/fixture", title: "fixture", pid: 42, terminal: "kitty", send_channel: "tmux", session_instance_id: "instance" }],
    approvals: [], questions: [], config: {}, update: null,
    message_deliveries: [{ message_id: 1, session_id: "session", text: "fila", queued_at_ms: 1, state: "queued", identity: { daemon_epoch: "budget", session_instance_id: "instance" } }],
    usage: { providers: [{ provider: "codex", detected: true, checked_at_ms: Date.now(), snapshot: { provider: "codex", fetched_at_ms: Date.now(), windows: [{ key: "primary", label: "7d", percent: 7 }], credits: { balance: 10, unlimited: false } } }] },
    quiet_scenes: { active: false, focus_mode: false, screen_off: false },
  } } };
  tauri.emit("daemon-ui-state", structuredClone(cache));
  tauri.emit("island-toggle", {});
  const input = document.querySelector<HTMLTextAreaElement>(".message-input")!;
  input.value = "rascunho preservado"; input.dispatchEvent(new window.Event("input"));
  input.focus(); input.setSelectionRange(2, 8);
  const usage = document.getElementById("header-usage")!;
  const model = document.getElementById("header-usage-model")!;
  const delivery = document.querySelector(".message-delivery")!;
  const first = usage.firstElementChild;
  const observer = new window.MutationObserver((): void => {});
  for (const root of [usage, model, delivery]) observer.observe(root, { subtree: true, attributes: true, childList: true, characterData: true });
  for (let i = 0; i < 200; i += 1) tauri.emit("daemon-ui-state", structuredClone(cache));
  expect(observer.takeRecords()).toHaveLength(0); observer.disconnect();
  expect(usage.firstElementChild).toBe(first);
  expect(document.querySelector(".message-delivery")).toBe(delivery);
  expect(document.querySelector(".message-input")).toBe(input);
  expect(document.activeElement).toBe(input);
  expect(input.value).toBe("rascunho preservado");
  expect([input.selectionStart, input.selectionEnd]).toEqual([2, 8]);
});

test("usage values update and reorder by key without replacing retained nodes", (): void => {
  const root = document.createElement("div"); const view = new UsageLane(root);
  const first = { key: "a", label: "5h", value: "10%", reset: "1h" };
  const second = { key: "b", label: "7d", value: "20%" };
  view.update([first, second], false);
  const a = root.children[0]; const b = root.children[2];
  view.update([second, { ...first, value: "30%", reset: "59m", severity: "is-high" }], true);
  expect(root.children[0]).toBe(b); expect(root.children[2]).toBe(a);
  expect(a.querySelector(".usage-percent")!.textContent).toBe("30%");
  expect(a.querySelector(".usage-reset")!.textContent).toBe("59m");
  expect(a.querySelector(".usage-percent")!.classList.contains("is-high")).toBe(true);
  view.update([first, { ...first, label: "duplicate" }], false);
  expect(root.querySelectorAll(".usage-window")).toHaveLength(2);
  view.update([], false); expect(root.children.length).toBe(0); expect(root.hidden).toBe(true);
});

test("a burst commits every state but paints the latest state once at the frame boundary", (): void => {
  const cache = tauri.state.read();
  cache.generation = 3; cache.snapshot!.generation = 3;
  const snapshot = cache.snapshot!.snapshot;
  snapshot.sessions = [{ id: "burst", agent: "codex", cwd: "/fixture", title: "before", pid: 42, terminal: "kitty", send_channel: "tmux", session_instance_id: "instance" }];
  tauri.emit("daemon-ui-state", structuredClone(cache));
  const input = document.querySelector<HTMLTextAreaElement>(".message-input")!;
  input.focus();
  const header = document.querySelector(".row-head")!;
  const observer = new window.MutationObserver((): void => {});
  observer.observe(header, { childList: true });
  const monitorCalls = tauri.calls.filter((call): boolean => call.command === "set_island_monitor").length;
  for (let i = 0; i < 200; i += 1) {
    snapshot.publication_revision += 1; snapshot.sessions[0].title = `event ${i}`;
    tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
    if (i % 20 === 0) { input.value += "x"; input.dispatchEvent(new window.Event("input")); }
  }
  input.setSelectionRange(2, 5);
  expect(document.querySelector(".row-project")!.textContent).toBe("before");
  expect(observer.takeRecords()).toHaveLength(0);
  paintFrame();
  expect(observer.takeRecords().filter((record): boolean => record.addedNodes.length > 0)).toHaveLength(1);
  expect(document.querySelector(".row-project")!.textContent).toBe("event 199");
  expect(document.querySelector(".message-input")).toBe(input);
  expect(document.activeElement).toBe(input);
  expect(input.value).toBe("xxxxxxxxxx");
  expect([input.selectionStart, input.selectionEnd]).toEqual([2, 5]);
  expect(tauri.calls.filter((call): boolean => call.command === "set_island_monitor")).toHaveLength(monitorCalls);
  paintFrame(); expect(observer.takeRecords()).toHaveLength(0);
  observer.disconnect();
});

test("old controls cannot authorize or edit replacement state before its paint", async (): Promise<void> => {
  const cache = tauri.state.read(); cache.generation = 4; cache.snapshot!.generation = 4;
  const snapshot = cache.snapshot!.snapshot;
  snapshot.sessions = [{ id: "guard", agent: "codex", cwd: "/fixture", title: "guard", pid: 42, terminal: "kitty", send_channel: "tmux", session_instance_id: "old" }];
  snapshot.approvals = [{ approval_id: "approval", session_id: "guard", session_instance_id: "old", pending_generation: 1, tool_name: "Bash" }];
  snapshot.questions = [{ question_id: "question", session_id: "guard", session_instance_id: "old", pending_generation: 1, agent: "codex", answerable: true, questions: [{ question: "Old", options: [{ label: "Sim" }], custom: true, multi_select: false }] }];
  tauri.emit("daemon-ui-state", structuredClone(cache));
  const option = document.querySelector<HTMLButtonElement>(".question-option")!;
  option.click();
  const submit = document.querySelector<HTMLButtonElement>(".question-submit")!;
  const custom = document.querySelector<HTMLInputElement>(".question-custom")!;
  const input = document.querySelector<HTMLTextAreaElement>(".message-input")!;
  const count = (): number => tauri.calls.filter((call): boolean => ["send_message_v2", "jump_v2", "answer_question_v2", "resolve_approval_v2", "plugin:voice|voice_start"].includes(call.command)).length;
  const before = count();
  snapshot.publication_revision += 1;
  snapshot.sessions[0].session_instance_id = "new";
  snapshot.approvals[0].pending_generation = 2; snapshot.approvals[0].session_instance_id = "new";
  snapshot.questions[0].pending_generation = 2; snapshot.questions[0].session_instance_id = "new";
  snapshot.questions[0].questions[0].question = "New";
  tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  option.click(); custom.value = "old answer"; custom.dispatchEvent(new window.Event("input")); submit.click();
  document.querySelector<HTMLButtonElement>("#approval-allow")!.click();
  document.querySelector<HTMLButtonElement>(".session-row")!.click();
  document.querySelector<HTMLButtonElement>(".voice-toggle")!.click();
  input.value = "retained draft";
  input.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter" }));
  await Promise.resolve(); await Promise.resolve();
  expect(count()).toBe(before);
  paintFrame();
  expect(document.querySelector<HTMLInputElement>(".question-custom")!.value).toBe("");
  expect(document.querySelector<HTMLButtonElement>(".question-submit")!.disabled).toBe(true);
  expect(document.querySelector(".question-text")!.textContent).toBe("New");
  expect(input.value).toBe("retained draft");
});

test("configuration bursts apply only their final visual values and monitor request", (): void => {
  const cache = tauri.state.read(); cache.generation = 5; cache.snapshot!.generation = 5;
  const snapshot = cache.snapshot!.snapshot;
  tauri.emit("daemon-ui-state", structuredClone(cache));
  const calls = (): number => tauri.calls.filter((call): boolean => call.command === "set_island_monitor").length;
  const before = calls(); const previous = document.documentElement.style.getPropertyValue("--content-font");
  for (let i = 0; i < 200; i += 1) {
    snapshot.publication_revision += 1;
    snapshot.config = { display: { content_font: 11 + i % 5, monitor: `fixture-${i}` } };
    tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  }
  expect(calls()).toBe(before);
  expect(document.documentElement.style.getPropertyValue("--content-font")).toBe(previous);
  paintFrame();
  expect(calls()).toBe(before + 1);
  expect(tauri.calls.filter((call): boolean => call.command === "set_island_monitor").pop()!.args).toEqual({ name: "fixture-199" });
  expect(document.documentElement.style.getPropertyValue("--content-font")).toBe("15px");
});

test("a completion edge is retained when later working state arrives before paint", (): void => {
  const cache = tauri.state.read(); cache.generation = 6; cache.snapshot!.generation = 6;
  const snapshot = cache.snapshot!.snapshot;
  snapshot.sessions = [{ id: "completion", agent: "codex", cwd: "/fixture", title: "completion", pid: 42, terminal: "kitty", session_instance_id: "instance", attention: "working" }];
  tauri.emit("daemon-ui-state", structuredClone(cache));
  if (main.expanded) tauri.emit("island-toggle", {});
  snapshot.publication_revision += 1;
  snapshot.sessions[0].attention = "needs_attention"; snapshot.sessions[0].completion_id = "frame-completion";
  tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  snapshot.publication_revision += 1; snapshot.sessions[0].attention = "working";
  tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  expect(main.expanded).toBe(false);
  expect(main.sessions[0].attention).toBe("working");
  paintFrame(); expect(main.expanded).toBe(true);
  tauri.emit("island-toggle", {});
  snapshot.publication_revision += 1;
  snapshot.sessions[0].attention = "needs_attention"; snapshot.sessions[0].completion_id = "superseded-completion";
  tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  cache.generation += 1; cache.snapshot!.generation = cache.generation;
  snapshot.daemon_epoch = "replacement-daemon";
  tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  paintFrame(); expect(main.expanded).toBe(false);
});

test("collapsed updates skip row fills and expanding catches up once", (): void => {
  if (main.expanded) tauri.emit("island-toggle", {});
  const cache = tauri.state.read();
  cache.generation = 7; cache.snapshot!.generation = 7;
  const snapshot = cache.snapshot!.snapshot;
  snapshot.sessions = [{ id: "deferred", agent: "codex", cwd: "/fixture", title: "antes", pid: 42, terminal: "kitty", send_channel: "tmux", session_instance_id: "instance" }];
  tauri.emit("daemon-ui-state", structuredClone(cache));
  const row = document.querySelector(".session-row")!;
  const observer = new window.MutationObserver((): void => {});
  observer.observe(row, { subtree: true, attributes: true, childList: true, characterData: true });
  for (let i = 0; i < 200; i += 1) {
    snapshot.publication_revision += 1;
    snapshot.sessions[0].title = `depois ${i}`;
    tauri.emitBeforePaint("daemon-ui-state", structuredClone(cache));
  }
  paintFrame();
  expect(observer.takeRecords()).toHaveLength(0);
  expect(document.querySelector(".row-project")!.textContent).toBe("antes");
  tauri.emit("island-toggle", {});
  expect(document.querySelector(".row-project")!.textContent).toBe("depois 199");
  observer.disconnect();
});
