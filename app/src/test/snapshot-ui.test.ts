import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";
import type { UiCache } from "../daemon-state";
let settleApproval: (() => void) | undefined;
let settleQuestion: (() => void) | undefined;
const tauri = tauriMock(({ command }): unknown => {
  if (command === "get_config") return { config: {} };
  if (command === "get_usage") return { providers: [] };
  if (command === "get_update") return null;
  if (command === "list_sessions" || command === "get_message_recovery") return [];
  if (command === "get_daemon_ui_state") return { phase: "connecting", generation: 1, snapshot: null };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "send_message_v2") return { message_id: 1, delivered: false, client_submission_id: "submission" };
  if (command === "resolve_approval_v2") return new Promise<void>((resolve): void => { settleApproval = resolve; });
  if (command === "answer_question_v2") return new Promise<void>((resolve): void => { settleQuestion = resolve; });
  return {};
});
mock.module("@tauri-apps/api/core", (): object => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", (): object => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
const main = await import("../main");
await new Promise((resolve): void => { setTimeout(resolve, 30); });
function cache(generation: number, epoch: string): UiCache {
  return { phase: "connected", generation, snapshot: { generation, snapshot: {
    schema_version: 1, daemon_epoch: epoch, publication_revision: 1,
    sessions: [{ id: "claude:s", agent: "claude", cwd: "/tmp/project", title: "project", pid: 42, terminal: "kitty", send_channel: "tmux", attention: "working", session_instance_id: "instance" }],
    approvals: [], questions: [], message_deliveries: [], config: {}, usage: { providers: [] }, update: null,
    quiet_scenes: { active: false, focus_mode: false, screen_off: false },
  } } };
}
test("before hydration legacy events cannot populate the island or authorize actions", (): void => {
  const legacy = ["list_sessions", "get_config", "get_usage", "get_update", "jump", "resolve_approval", "answer_question"];
  expect(tauri.calls.filter((call): boolean => legacy.includes(call.command))).toEqual([]);
  tauri.emit("sessions-updated", cache(1, "legacy").snapshot!.snapshot.sessions);
  tauri.emit("approval-requested", { approval_id: "legacy", session_id: "claude:s", tool_name: "Bash" });
  tauri.emit("update-available", { version: "v99" });
  expect(main.sessions).toEqual([]);
  expect(document.getElementById("approval-card")!.hidden).toBe(true);
  expect(document.getElementById("header-update")!.hidden).toBe(true);
  expect(document.querySelector<HTMLButtonElement>("#header-mute")!.disabled).toBe(true);
  const session = cache(1, "legacy").snapshot!.snapshot.sessions[0];
  main.jumpTo({ ...session, action_identity: { daemon_epoch: "legacy", session_instance_id: "instance" } }, document.createElement("button"));
  main.jumpToId(session.id);
  expect(tauri.calls.some((call): boolean => call.command === "jump_v2")).toBe(false);
});
test("initial discovery is distinct from a connected empty session list", () => {
  const initial = cache(1, "initial");
  initial.snapshot!.snapshot.sessions = [];
  initial.snapshot!.snapshot.discovering = true;
  tauri.emit("daemon-ui-state", initial);
  const status = document.querySelector<HTMLElement>(".daemon-connection-status")!;
  expect(status.hidden).toBe(false);
  expect(status.textContent).toBe("Identificando as sessões locais…");
  initial.snapshot!.snapshot.discovering = false;
  initial.snapshot!.snapshot.publication_revision += 1;
  tauri.emit("daemon-ui-state", initial);
  expect(status.hidden).toBe(true);
});
test("authoritative snapshot feeds guarded editor and disconnect preserves draft", async (): Promise<void> => {
  if (!main.expanded) tauri.emit("island-toggle", {});
  tauri.emit("daemon-ui-state", cache(2, "epoch"));
  const input = document.querySelector<HTMLTextAreaElement>(".message-input")!;
  expect(input).not.toBeNull();
  expect(input.disabled).toBe(false);
  tauri.emit("sessions-updated", []);
  expect(main.sessions.length).toBe(1);
  input.value = "preservar";
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
  tauri.emit("daemon-ui-state", { phase: "reconnecting", generation: 2, snapshot: null });
  expect(main.sessions.length).toBe(1);
  expect(input.value).toBe("preservar");
  expect(input.disabled).toBe(true);
  document.querySelector<HTMLButtonElement>(".session-row")?.click();
  expect(tauri.calls.some((call): boolean => call.command === "jump_v2")).toBe(false);
  input.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await Promise.resolve();
  expect(tauri.calls.some((call): boolean => call.command === "send_message_v2")).toBe(false);
  tauri.emit("daemon-ui-state", cache(3, "next-epoch"));
  expect(document.querySelector(".message-input")).toBe(input);
  expect(input.value).toBe("preservar");
  expect(input.disabled).toBe(false);
  input.dispatchEvent(new window.KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise((resolve): void => { setTimeout(resolve, 10); });
  const request = tauri.calls.find((call): boolean => call.command === "send_message_v2");
  expect(request?.args).toEqual({ id: "claude:s", text: "preservar", identity: { daemon_epoch: "next-epoch", session_instance_id: "instance" } });
  expect(tauri.calls.some((call): boolean => call.command === "send_message")).toBe(false);
  expect(input.value).toBe("");
  tauri.emit("daemon-ui-state", cache(2, "epoch"));
  expect(main.sessions[0]?.action_identity?.daemon_epoch).toBe("next-epoch");
  document.querySelector<HTMLButtonElement>(".session-row")?.click();
  await Promise.resolve();
  const jump = tauri.calls.find((call): boolean => call.command === "jump_v2");
  expect(jump?.args).toEqual({ id: "claude:s", identity: { daemon_epoch: "next-epoch", session_instance_id: "instance" } });
});

test("child navigation captures its own identity while family stays one row", async (): Promise<void> => {
  const state = cache(4, "family-epoch");
  const snapshot = state.snapshot!.snapshot;
  snapshot.sessions = [{ ...snapshot.sessions[0]!, id: "opencode:parent", agent: "opencode", session_instance_id: "parent-instance", subagents: [{ id: "child", kind: "opencode", done: false }] }];
  snapshot.child_sessions = [{ ...snapshot.sessions[0]!, id: "opencode:child", session_instance_id: "child-instance", subagents: undefined }];
  tauri.emit("daemon-ui-state", state);
  expect(main.sessions.length).toBe(1);
  main.jumpToId("opencode:child");
  await Promise.resolve();
  const jumps = tauri.calls.filter((call): boolean => call.command === "jump_v2");
  const jump = jumps[jumps.length - 1];
  expect(jump?.args).toEqual({ id: "opencode:child", identity: { daemon_epoch: "family-epoch", session_instance_id: "child-instance" } });
  const before = tauri.calls.length;
  main.jumpToId("opencode:missing");
  expect(tauri.calls.length).toBe(before);
});

test("delivery cards route old-instance discard without touching the current editor", async (): Promise<void> => {
  const state = cache(5, "delivery-epoch");
  state.snapshot!.snapshot.message_deliveries = [
    { message_id: 7, session_id: "claude:s", text: "current text", queued_at_ms: 1, state: "sending", identity: { daemon_epoch: "delivery-epoch", session_instance_id: "instance" } },
    { message_id: 8, session_id: "claude:s", text: "old text", queued_at_ms: 1, state: "unconfirmed", identity: { daemon_epoch: "delivery-epoch", session_instance_id: "old-instance" } },
  ];
  tauri.emit("daemon-ui-state", state);
  const current = document.querySelector<HTMLElement>('.message-delivery[data-state="sending"]')!;
  const old = document.querySelector<HTMLElement>('.message-delivery[data-state="unconfirmed"]')!;
  expect(current.querySelectorAll("button")[1].disabled).toBe(true);
  expect(old.querySelector("textarea")!.value).toBe("old text");
  const editor = document.querySelector<HTMLTextAreaElement>(".message-input")!;
  editor.value = "new draft";
  editor.dispatchEvent(new window.Event("input", { bubbles: true }));
  old.querySelectorAll("button")[1].click();
  await Promise.resolve();
  const requests = tauri.calls.filter((call): boolean => call.command === "cancel_message_v2");
  expect(requests[requests.length - 1]?.args).toEqual({ id: "claude:s", messageId: 8, identity: { daemon_epoch: "delivery-epoch", session_instance_id: "old-instance" } });
  expect(editor.value).toBe("new draft");
  tauri.emit("daemon-ui-state", { phase: "reconnecting", generation: 5, snapshot: null });
  expect(old.querySelectorAll("button")[1].disabled).toBe(true);
  expect(old.querySelector("textarea")!.value).toBe("old text");
  expect(editor.value).toBe("new draft");
});

test("late approval and answer responses cannot close replacement pending generations", async (): Promise<void> => {
  const state = cache(6, "pending-epoch");
  const snapshot = state.snapshot!.snapshot;
  snapshot.approvals = [{ approval_id: "reused", session_id: "claude:s", session_instance_id: "instance", pending_generation: 1, tool_name: "Bash" }];
  snapshot.questions = [{ question_id: "reused", session_id: "claude:s", session_instance_id: "instance", pending_generation: 1, agent: "claude", answerable: true, questions: [{ question: "Escolha", options: [{ label: "Sim" }], custom: true, multi_select: false }] }];
  tauri.emit("daemon-ui-state", structuredClone(state));
  document.querySelector<HTMLButtonElement>("#approval-allow")!.click();
  document.querySelector<HTMLButtonElement>(".question-option")!.click();
  document.querySelector<HTMLButtonElement>(".question-submit")!.click();
  expect(settleApproval).toBeDefined();
  expect(settleQuestion).toBeDefined();
  for (const command of ["resolve_approval_v2", "answer_question_v2"]) {
    const call = tauri.calls.filter((call): boolean => call.command === command).pop();
    expect(call?.args.identity).toEqual({ daemon_epoch: "pending-epoch", session_instance_id: "instance" });
    expect(call?.args.pendingGeneration).toBe(1);
  }
  snapshot.publication_revision = 2;
  snapshot.approvals[0].pending_generation = 2;
  snapshot.questions[0].pending_generation = 2;
  tauri.emit("daemon-ui-state", structuredClone(state));
  const input = document.querySelector<HTMLInputElement>(".question-custom")!;
  input.value = "resposta nova";
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
  settleApproval?.(); settleQuestion?.();
  await new Promise((resolve): void => { setTimeout(resolve, 0); });
  expect(document.getElementById("approval-card")!.hidden).toBe(false);
  expect(document.querySelector<HTMLButtonElement>("#approval-allow")!.disabled).toBe(false);
  expect(document.querySelector<HTMLInputElement>(".question-custom")!.value).toBe("resposta nova");
  expect(document.querySelector<HTMLButtonElement>(".question-submit")!.disabled).toBe(false);
  tauri.emit("daemon-ui-state", { phase: "incompatible", generation: 6, snapshot: null });
  expect(document.querySelector<HTMLButtonElement>("#approval-allow")!.disabled).toBe(true);
  expect(document.querySelector<HTMLButtonElement>(".question-submit")!.disabled).toBe(true);
  expect(document.querySelector<HTMLInputElement>(".question-custom")!.value).toBe("resposta nova");
});

test("a leaving row and a stale question cannot navigate to a replacement instance", (): void => {
  const state = cache(7, "navigation-epoch");
  const snapshot = state.snapshot!.snapshot;
  snapshot.questions = [{ question_id: "navigate", session_id: "claude:s", session_instance_id: "instance", pending_generation: 1, agent: "claude", answerable: false, questions: [] }];
  tauri.emit("daemon-ui-state", structuredClone(state));
  const oldRow = document.querySelector<HTMLButtonElement>(".session-row")!;
  const oldQuestion = document.querySelector<HTMLButtonElement>(".question-jump")!;
  snapshot.sessions = []; snapshot.publication_revision += 1;
  tauri.emit("daemon-ui-state", structuredClone(state));
  snapshot.sessions = [{ ...cache(7, "navigation-epoch").snapshot!.snapshot.sessions[0], session_instance_id: "replacement" }];
  snapshot.publication_revision += 1;
  tauri.emit("daemon-ui-state", structuredClone(state));
  const count = tauri.calls.filter((call): boolean => call.command === "jump_v2").length;
  oldRow.click(); oldQuestion.click();
  document.querySelector<HTMLButtonElement>(".question-jump")!.click();
  expect(tauri.calls.filter((call): boolean => call.command === "jump_v2")).toHaveLength(count);
  document.querySelector<HTMLButtonElement>(".session-row")!.click();
  const request = tauri.calls.filter((call): boolean => call.command === "jump_v2").pop();
  expect(request?.args.identity).toEqual({ daemon_epoch: "navigation-epoch", session_instance_id: "replacement" });
});
