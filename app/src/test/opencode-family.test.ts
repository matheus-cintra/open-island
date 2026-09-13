import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
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
  tauri.emit("sessions-updated", [parent]);
  expect(document.querySelectorAll(".session-row")).toHaveLength(1);
  expect(document.querySelectorAll(".agent-item")).toHaveLength(2);
  expect(document.querySelector(".row-agents")?.textContent).toContain("Research");
  tauri.emit("config-changed", { config: { display: { subagents: false } } });
  expect(document.querySelector<HTMLElement>(".agents-list")?.hidden).toBe(true);
  tauri.emit("config-changed", { config: { display: { subagents: true } } });
});

test("child navigation uses the parent terminal and questions retain original request IDs", async () => {
  main.jumpToId("opencode:b");
  expect(tauri.calls[tauri.calls.length - 1]).toEqual({ command: "jump", args: { id: "opencode:root" } });
  for (const id of ["a", "b"]) {
    tauri.emit("question-asked", {
      question_id: `q-${id}`, session_id: `opencode:${id}`, agent: "opencode",
      answerable: true, questions: [{ question: `Question ${id}`, options: [{ label: "Yes" }] }],
    });
  }
  expect(document.getElementById("question-kicker")?.textContent).toContain("Research");
  (document.querySelector(".question-option") as HTMLButtonElement).click();
  (document.querySelector(".question-submit") as HTMLButtonElement).click();
  await Promise.resolve();
  await Promise.resolve();
  expect(tauri.calls).toContainEqual({ command: "answer_question", args: { questionId: "q-a", answers: [["Yes"]] } });
  expect(document.getElementById("question-kicker")?.textContent).toContain("Review");
  (document.querySelector(".question-option") as HTMLButtonElement).click();
  (document.querySelector(".question-submit") as HTMLButtonElement).click();
  await Promise.resolve();
  expect(tauri.calls).toContainEqual({ command: "answer_question", args: { questionId: "q-b", answers: [["Yes"]] } });
});

test("simultaneous approvals remain answerable in order with the child's name", async () => {
  for (const id of ["a", "b"]) tauri.emit("approval-requested", {
    approval_id: `p-${id}`, session_id: `opencode:${id}`, tool_name: "Bash", tool_input: { command: id },
  });
  expect(document.getElementById("approval-summary")?.textContent).toContain("Research");
  (document.getElementById("approval-allow") as HTMLButtonElement).click();
  await Promise.resolve();
  await Promise.resolve();
  expect(tauri.calls).toContainEqual({ command: "resolve_approval", args: { approvalId: "p-a", decision: "allow" } });
  expect(document.getElementById("approval-summary")?.textContent).toContain("Review");
  (document.getElementById("approval-deny") as HTMLButtonElement).click();
  await Promise.resolve();
  expect(tauri.calls).toContainEqual({ command: "resolve_approval", args: { approvalId: "p-b", decision: "deny" } });
});
