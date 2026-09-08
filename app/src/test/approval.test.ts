import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock();
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");

const card = document.getElementById("approval-card")!;
const tool = document.getElementById("approval-tool")!;
const summary = document.getElementById("approval-summary")!;
const always = document.getElementById("approval-always") as HTMLButtonElement;

const PLAN = [
  "# Context",
  "",
  "`README.md` has a single line.",
  "",
  "# Plan",
  "",
  "Append a greeting to it.",
].join("\n");

function request(toolName: string, toolInput: unknown): void {
  tauri.emit("approval-requested", {
    approval_id: `approval-${toolName}`,
    session_id: "claude:abc123",
    tool_name: toolName,
    tool_input: toolInput,
  });
}

test("a plan is labelled Plano, not by the tool that carries it", () => {
  request("ExitPlanMode", { plan: PLAN, planFilePath: "/tmp/p.md" });
  expect(card.hidden).toBe(false);
  expect(tool.textContent).toBe("Plano");
});

test("the summary is the plan's first real line, not its first heading", () => {
  request("ExitPlanMode", { plan: PLAN, planFilePath: "/tmp/p.md" });
  expect(summary.textContent).toBe("`README.md` has a single line.");
});

test("a plan that is nothing but headings still says what is being asked", () => {
  request("ExitPlanMode", { plan: "# Plan\n\n## Steps\n" });
  expect(summary.textContent).toBe("O agente quer sair do modo plano");
});

test("Sempre is hidden on a plan, where it would repeat Permitir", () => {
  request("ExitPlanMode", { plan: PLAN });
  expect(always.hidden).toBe(true);
});

test("Sempre still shows on an ordinary tool from an agent that supports it", () => {
  request("Bash", { command: "pwd" });
  expect(always.hidden).toBe(false);
  expect(tool.textContent).toBe("Bash");
  expect(summary.textContent).toBe("pwd");
});
