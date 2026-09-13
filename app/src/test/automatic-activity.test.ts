import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const base = {
  id: "claude:one",
  agent: "claude",
  cwd: "/work",
  title: "work",
  pid: 42,
  terminal: "kitty" as const,
  attention: "needs_attention" as const,
  completion_id: "daemon-a-1",
};

const tauri = tauriMock((call) => {
  if (call.command === "get_config") return { config: {} };
  if (call.command === "list_sessions") return [base];
  if (call.command === "get_usage") return { providers: [] };
  if (call.command === "get_update") return null;
  return {};
});
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");
await new Promise((resolve) => setTimeout(resolve, 30));

const island = document.getElementById("island")!;

test("the baseline and content-only updates stay compact, then a new known-session completion opens", () => {
  expect(island.classList.contains("expanded")).toBe(false);
  tauri.emit("sessions-updated", [{ ...base, summary: "fresh content" }]);
  expect(island.classList.contains("expanded")).toBe(false);
  tauri.emit("sessions-updated", [{ ...base, completion_id: "daemon-a-2" }]);
  expect(island.classList.contains("expanded")).toBe(true);
});

test("duplicate approval delivery after resolution neither reopens the card nor the island", () => {
  tauri.emit("approval-requested", {
    approval_id: "approval-1", session_id: "claude:one", tool_name: "Bash",
  });
  tauri.emit("approval-resolved", {
    approval_id: "approval-1", session_id: "claude:one", decision: "allow",
  });
  tauri.emit("island-toggle", {});
  expect(island.classList.contains("expanded")).toBe(false);
  tauri.emit("approval-requested", {
    approval_id: "approval-1", session_id: "claude:one", tool_name: "Bash",
  });
  expect(island.classList.contains("expanded")).toBe(false);
  expect(document.getElementById("approval-card")!.hidden).toBe(true);
});

test("a pending card is indicated compactly before there is a session row", () => {
  tauri.emit("question-asked", {
    question_id: "question-1", session_id: "claude:missing", agent: "claude",
    answerable: false, questions: [{ question: "Continue?", options: [] }],
  });
  tauri.emit("sessions-updated", []);
  const pending = document.querySelector<HTMLElement>(".compact-pending");
  expect(pending?.hidden).toBe(false);
  expect(document.querySelector(".compact-count")?.textContent).toBe("");
  expect(document.querySelector<HTMLElement>(".compact-label")?.hidden).toBe(true);
});

test("a custom question draft and its focus survive duplicate delivery and ordinary snapshots", () => {
  const payload = {
    question_id: "question-custom", session_id: "claude:one", agent: "claude",
    answerable: true, questions: [{ question: "Describe it", custom: true, options: [] }],
  };
  tauri.emit("question-asked", payload);
  const input = document.querySelector<HTMLInputElement>(".question-custom")!;
  input.value = "keep this draft";
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
  input.focus();

  tauri.emit("question-asked", payload);
  let restored = document.querySelector<HTMLInputElement>(".question-custom")!;
  expect(restored.value).toBe("keep this draft");
  expect(document.activeElement).toBe(restored);

  tauri.emit("sessions-updated", [base]);
  restored = document.querySelector<HTMLInputElement>(".question-custom")!;
  expect(restored.value).toBe("keep this draft");
  expect(document.activeElement).toBe(restored);
});

test("a focused question option survives an ordinary session snapshot", () => {
  tauri.emit("question-asked", {
    question_id: "question-option", session_id: "claude:one", agent: "claude",
    answerable: true,
    questions: [{ question: "Continue?", options: [{ label: "Yes" }, { label: "No" }] }],
  });
  const option = document.querySelector<HTMLButtonElement>(".question-option")!;
  option.focus();
  tauri.emit("sessions-updated", [base]);
  expect(document.activeElement).toBe(option);
});
