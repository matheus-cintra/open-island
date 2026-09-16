import { expect, mock, test } from "bun:test";
import { mountIsland, paintFrame } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock();
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
const main = await import("../main");
const parent = {
  id: "opencode:root", agent: "opencode", cwd: "/work", title: "work", name: "Main",
  pid: 42, terminal: "kitty" as const, attention: "waiting_for_input" as const,
  subagents: [
    { id: "a", kind: "opencode", description: "Research", tool: "Read", done: false },
    { id: "b", kind: "opencode", description: "Review", done: true },
  ],
};

test("one family row shows both children and honors subagent visibility", () => {
  if (!main.expanded) tauri.emit("island-toggle", {});
  tauri.state.sessions([parent]);
  tauri.state.children(["a", "b"].map((id) => ({ ...parent, id: `opencode:${id}`, subagents: undefined })));
  expect(document.querySelectorAll(".session-row")).toHaveLength(1);
  expect(document.querySelectorAll(".agent-item")).toHaveLength(2);
  expect(document.querySelector(".row-agents")?.textContent).toContain("Research");
  tauri.state.config({ display: { subagents: false } });
  expect(document.querySelector<HTMLElement>(".agents-list")?.hidden).toBe(true);
  tauri.state.config({ display: { subagents: true } });
});

test("child navigation and questions retain the child target identity", async () => {
  main.jumpToId("opencode:b");
  expect(tauri.calls[tauri.calls.length - 1]).toEqual({ command: "jump_v2", args: { id: "opencode:b", identity: { daemon_epoch: "fixture-epoch", session_instance_id: "instance:opencode:b" } } });
  for (const id of ["a", "b"]) {
    tauri.state.question({
      question_id: `q-${id}`, session_id: `opencode:${id}`, agent: "opencode",
      answerable: true, questions: [{ question: `Question ${id}`, options: [{ label: "Yes" }] }],
    });
  }
  expect(document.getElementById("question-kicker")?.textContent).toContain("Research");
  (document.querySelector(".question-option") as HTMLButtonElement).click();
  (document.querySelector(".question-submit") as HTMLButtonElement).click();
  await Promise.resolve();
  await Promise.resolve();
  paintFrame();
  expect(tauri.calls).toContainEqual({ command: "answer_question_v2", args: { questionId: "q-a", answers: [["Yes"]], identity: { daemon_epoch: "fixture-epoch", session_instance_id: "instance:opencode:a" }, pendingGeneration: 1 } });
  tauri.state.resolveQuestion("q-a");
  expect(document.getElementById("question-kicker")?.textContent).toContain("Review");
  (document.querySelector(".question-option") as HTMLButtonElement).click();
  (document.querySelector(".question-submit") as HTMLButtonElement).click();
  await Promise.resolve();
  expect(tauri.calls).toContainEqual({ command: "answer_question_v2", args: { questionId: "q-b", answers: [["Yes"]], identity: { daemon_epoch: "fixture-epoch", session_instance_id: "instance:opencode:b" }, pendingGeneration: 1 } });
});

test("simultaneous approvals remain answerable in order with the child's name", async () => {
  for (const id of ["a", "b"]) tauri.state.approval({
    approval_id: `p-${id}`, session_id: `opencode:${id}`, tool_name: "Bash", tool_input: { command: id },
  });
  expect(document.getElementById("approval-summary")?.textContent).toContain("Research");
  (document.getElementById("approval-allow") as HTMLButtonElement).click();
  await Promise.resolve();
  await Promise.resolve();
  paintFrame();
  expect(tauri.calls).toContainEqual({ command: "resolve_approval_v2", args: { approvalId: "p-a", decision: "allow", identity: { daemon_epoch: "fixture-epoch", session_instance_id: "instance:opencode:a" }, pendingGeneration: 1 } });
  expect(document.getElementById("approval-summary")?.textContent).toContain("Review");
  (document.getElementById("approval-deny") as HTMLButtonElement).click();
  await Promise.resolve();
  expect(tauri.calls).toContainEqual({ command: "resolve_approval_v2", args: { approvalId: "p-b", decision: "deny", identity: { daemon_epoch: "fixture-epoch", session_instance_id: "instance:opencode:b" }, pendingGeneration: 1 } });
});
