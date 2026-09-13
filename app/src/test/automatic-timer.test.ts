import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const base = {
  id: "claude:known", agent: "claude", cwd: "/work", title: "work", pid: 42,
  terminal: "kitty" as const, attention: "working" as const,
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

class ControlledClock {
  now = 0;
  private next = 1;
  private timers = new Map<number, { at: number; callback: () => void }>();
  setTimeout = (callback: () => void, delay = 0): number => {
    const id = this.next++;
    this.timers.set(id, { at: this.now + delay, callback });
    return id;
  };
  clearTimeout = (id: number): void => { this.timers.delete(id); };
  advance(ms: number): void {
    this.now += ms;
    for (;;) {
      const due = [...this.timers.entries()]
        .filter(([, timer]) => timer.at <= this.now)
        .sort(([, left], [, right]) => left.at - right.at)[0];
      if (!due) return;
      this.timers.delete(due[0]);
      due[1].callback();
    }
  }
}

const island = document.getElementById("island")!;
const expanded = () => island.classList.contains("expanded");
const collapse = () => { if (expanded()) tauri.emit("island-toggle", {}); };

test("real listeners keep content silent and apply deadlines only to relevant new edges", async () => {
  const clock = new ControlledClock();
  const realSetTimeout = window.setTimeout;
  const realClearTimeout = window.clearTimeout;
  window.setTimeout = clock.setTimeout as typeof window.setTimeout;
  window.clearTimeout = clock.clearTimeout as typeof window.clearTimeout;
  try {
    // Tool/status content keeps the island compact even after a long running session.
    tauri.emit("sessions-updated", [{ ...base, current_tool: "Bash" }]);
    clock.advance(30_000);
    tauri.emit("sessions-updated", [{ ...base, current_tool: "Read" }]);
    expect(expanded()).toBe(false);

    // First observations, reorders, and removals are baseline maintenance, never activity.
    const fresh = { ...base, id: "claude:fresh", completion_id: "fresh-1", attention: "needs_attention" as const };
    tauri.emit("sessions-updated", [base, fresh]);
    tauri.emit("sessions-updated", [fresh, base]);
    tauri.emit("sessions-updated", [base]);
    expect(expanded()).toBe(false);

    // A completion seen while still working is consumed and cannot notify late.
    tauri.emit("sessions-updated", [{ ...base, completion_id: "working-1" }]);
    tauri.emit("sessions-updated", [{ ...base, completion_id: "working-1", attention: "needs_attention" }]);
    expect(expanded()).toBe(false);

    // Legacy snapshots only open for a known working -> needs_attention transition.
    tauri.emit("sessions-updated", [base]);
    tauri.emit("sessions-updated", [{ ...base, attention: "needs_attention" }]);
    expect(expanded()).toBe(true);
    clock.advance(2500);
    expect(expanded()).toBe(false);

    // Pending cards do not hold automatic expansion; a manual reopen does.
    tauri.emit("question-asked", {
      question_id: "timer-question", session_id: base.id, agent: "claude", answerable: false,
      questions: [{ question: "Continue?", options: [] }],
    });
    expect(expanded()).toBe(true);
    clock.advance(2500);
    expect(expanded()).toBe(false);
    tauri.emit("island-toggle", {});
    clock.advance(5000);
    expect(expanded()).toBe(true);
    collapse();

    // Same IDs are updates only; a new ID renews the single automatic deadline.
    tauri.emit("approval-requested", { approval_id: "a-1", session_id: base.id, tool_name: "Bash" });
    clock.advance(2400);
    tauri.emit("approval-requested", { approval_id: "a-1", session_id: base.id, tool_name: "Read" });
    clock.advance(100);
    expect(expanded()).toBe(false);
    tauri.emit("approval-requested", { approval_id: "a-2", session_id: base.id, tool_name: "Bash" });
    clock.advance(2400);
    tauri.emit("approval-requested", { approval_id: "a-3", session_id: base.id, tool_name: "Read" });
    clock.advance(2400);
    expect(expanded()).toBe(true);
    clock.advance(100);
    expect(expanded()).toBe(false);

    // Settling does not extend or retain the original deadline.
    tauri.emit("question-asked", {
      question_id: "settled", session_id: base.id, agent: "claude", answerable: false,
      questions: [{ question: "Done?", options: [] }],
    });
    clock.advance(1000);
    tauri.emit("question-resolved", { question_id: "settled", session_id: base.id, outcome: "answered" });
    clock.advance(1499);
    expect(expanded()).toBe(true);
    clock.advance(1);
    expect(expanded()).toBe(false);

    for (const id of ["a-1", "a-2", "a-3"]) {
      tauri.emit("approval-resolved", { approval_id: id, session_id: base.id, decision: "allow" });
    }

    // Resolving a focused card blurs it, releasing the hold into a full deadline.
    tauri.emit("approval-requested", { approval_id: "a-focused", session_id: base.id, tool_name: "Bash" });
    const allow = document.getElementById("approval-allow") as HTMLButtonElement;
    allow.focus();
    tauri.emit("approval-resolved", { approval_id: "a-focused", session_id: base.id, decision: "allow" });
    expect(document.activeElement).not.toBe(allow);
    await Promise.resolve();
    clock.advance(2500);
    expect(expanded()).toBe(false);

    // Without a hold, resolving does not renew the deadline already in progress.
    tauri.emit("approval-requested", { approval_id: "a-unfocused", session_id: base.id, tool_name: "Bash" });
    clock.advance(1000);
    tauri.emit("approval-resolved", { approval_id: "a-unfocused", session_id: base.id, decision: "allow" });
    clock.advance(1499);
    expect(expanded()).toBe(true);
    clock.advance(1);
    expect(expanded()).toBe(false);
  } finally {
    window.setTimeout = realSetTimeout;
    window.clearTimeout = realClearTimeout;
  }
});
