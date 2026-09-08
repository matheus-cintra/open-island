import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock();
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

interface Child {
  id: string;
  kind: string;
  done: boolean;
}

function session(id: string, subagents: Child[] = [], raisePid?: number): Parameters<
  typeof main.subagentEdge
>[0][number] {
  return {
    id,
    agent: "claude",
    cwd: "/work",
    title: "work",
    pid: 100,
    terminal: "kitty",
    subagents,
    raise_pid: raisePid,
  };
}

function configure(patch: Record<string, unknown>): void {
  main.applyConfig({
    island: { smart_suppression: true },
    notifications: {},
    ...patch,
  });
}

test("the completion and question switches gate their own path and never the approval", () => {
  configure({ notifications: { expand_on_completion: false, expand_on_question: false } });
  expect(main.admitsExpansion("completion")).toBe(false);
  expect(main.admitsExpansion("question")).toBe(false);
  expect(main.admitsExpansion("approval")).toBe(true);

  configure({ notifications: { expand_on_completion: true, expand_on_question: true } });
  expect(main.admitsExpansion("completion")).toBe(true);
  expect(main.admitsExpansion("question")).toBe(true);
});

test("a quiet scene stops every path, approvals included", () => {
  configure({});
  expect(main.admitsExpansion("approval")).toBe(true);
  tauri.emit("quiet-scenes", { active: true });
  expect(main.admitsExpansion("completion")).toBe(false);
  expect(main.admitsExpansion("question")).toBe(false);
  expect(main.admitsExpansion("approval")).toBe(false);
  tauri.emit("quiet-scenes", { active: false });
  expect(main.admitsExpansion("approval")).toBe(true);
});

test("smart suppression matches the window the jump would raise, not the agent", () => {
  const list = [session("one", [], 900)];
  tauri.emit("island-focus", { pid: null });
  expect(main.terminalIsFocused(list)).toBe(false);
  tauri.emit("island-focus", { pid: 100 });
  expect(main.terminalIsFocused(list)).toBe(false);
  tauri.emit("island-focus", { pid: 900 });
  expect(main.terminalIsFocused(list)).toBe(true);
});

test("the default timing never expands for a subagent", () => {
  configure({ notifications: { subagent_timing: "root_responses" } });
  main.subagentEdge([session("s", [{ id: "a", kind: "explore", done: false }])]);
  expect(main.subagentEdge([session("s", [{ id: "a", kind: "explore", done: true }])])).toBe(
    false,
  );
});

test("every completion fires once per subagent and not once per snapshot", () => {
  configure({ notifications: { subagent_timing: "every_completion" } });
  const running = [session("s", [{ id: "a", kind: "explore", done: false }])];
  const finished = [session("s", [{ id: "a", kind: "explore", done: true }])];
  main.subagentEdge(running);
  expect(main.subagentEdge(finished)).toBe(true);
  expect(main.subagentEdge(finished)).toBe(false);
  expect(main.subagentEdge(finished)).toBe(false);
});

test("all finished waits for the last one", () => {
  configure({ notifications: { subagent_timing: "all_finished" } });
  const two = (first: boolean, second: boolean): ReturnType<typeof session>[] => [
    session("s", [
      { id: "a", kind: "explore", done: first },
      { id: "b", kind: "explore", done: second },
    ]),
  ];
  main.subagentEdge(two(false, false));
  expect(main.subagentEdge(two(true, false))).toBe(false);
  expect(main.subagentEdge(two(true, true))).toBe(true);
});

test("a session seen for the first time never counts as an edge", () => {
  configure({ notifications: { subagent_timing: "every_completion" } });
  expect(main.subagentEdge([session("fresh", [{ id: "a", kind: "explore", done: true }])])).toBe(
    false,
  );
});
