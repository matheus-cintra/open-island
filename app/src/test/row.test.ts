import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const TERMINAL_ICON = "asset://kitty.png";

const tauri = tauriMock(({ command }) => (command === "terminal_icon" ? TERMINAL_ICON : {}));
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

function display(values: Record<string, unknown>): void {
  tauri.emit("config-changed", { config: { display: values } });
}

const session = {
  id: "s1",
  agent: "claude",
  cwd: "/home/user/dev/open-island",
  title: "open-island",
  pid: 4242,
  terminal: "kitty" as const,
  model: "claude-sonnet-4-5-20250929",
  attention: "working" as const,
  tasks: [
    { content: "Ler o roadmap", status: "completed" as const },
    { content: "Escrever o harness", status: "in_progress" as const },
    { content: "Provar a regressão", status: "pending" as const },
  ],
};

function keys(): string[] {
  return main.badgeSpec(session).map(([key]) => key);
}

test("the checklist renders one item per task under the three status counts", () => {
  display({ tasks: true });
  const li = main.createRow(session, false);
  const block = li.querySelector<HTMLElement>(".row-tasks")!;
  expect(block.hidden).toBe(false);
  expect(li.querySelectorAll(".task-item").length).toBe(3);
  expect(li.querySelector(".tasks-label")!.textContent).toBe("Tarefas");
  expect(li.querySelector(".tasks-count")!.textContent).toBe(
    "(1 concluídas, 1 em andamento, 1 em aberto)",
  );
});

test("a cancelled task closes rather than staying open", () => {
  display({ tasks: true });
  const li = main.createRow(
    {
      ...session,
      tasks: [
        { content: "Ler o roadmap", status: "cancelled" as const },
        { content: "Provar a regressão", status: "pending" as const },
      ],
    },
    false,
  );
  expect(li.querySelector(".tasks-count")!.textContent).toBe(
    "(1 concluídas, 0 em andamento, 1 em aberto)",
  );
});

test("each task carries its own status mark", () => {
  display({ tasks: true });
  const li = main.createRow(session, false);
  const marks = [...li.querySelectorAll(".task-mark")].map((mark) => mark.className);
  expect(marks).toEqual([
    "task-mark is-completed",
    "task-mark is-in-progress",
    "task-mark is-pending",
  ]);
});

test("the checklist is hidden when display.tasks is off", () => {
  display({ tasks: false });
  const li = main.createRow(session, false);
  expect(li.querySelector<HTMLElement>(".row-tasks")!.hidden).toBe(true);
  expect(li.querySelectorAll(".task-item").length).toBe(0);
});

test("a session with no tasks hides the checklist even when the toggle is on", () => {
  display({ tasks: true });
  const li = main.createRow({ ...session, tasks: undefined }, false);
  expect(li.querySelector<HTMLElement>(".row-tasks")!.hidden).toBe(true);
});

test("the agent badge is a text pill or an icon, never both", () => {
  display({ agent_icons: false });
  expect(keys()).toContain("agent");
  expect(keys()).not.toContain("session-icon");
  display({ agent_icons: true });
  expect(keys()).toContain("session-icon");
  expect(keys()).not.toContain("agent");
});

test("an agent with no icon of its own keeps the text pill", () => {
  display({ agent_icons: true });
  const specs = main.badgeSpec({ ...session, agent: "amazonq" });
  expect(specs.map(([key]) => key)).toContain("agent");
});

test("the terminal badge swaps once the icon lookup has answered", async () => {
  display({ terminal_icons: true });
  main.createRow(session, false);
  await Promise.resolve();
  expect(tauri.calls.some((call) => call.command === "terminal_icon")).toBe(true);
  expect(keys()).toContain("terminal-icon");
  display({ terminal_icons: false });
  expect(keys()).toContain("terminal");
});

test("a second fill patches the badges in place instead of rebuilding them", () => {
  display({ agent_icons: false, terminal_icons: false, model: true });
  const badges = document.createElement("span");
  main.fillBadges(badges, session);
  const model = badges.querySelector('[data-badge="model"]');
  expect(model!.textContent).toBe("Sonnet 4.5");
  main.fillBadges(badges, { ...session, model: "claude-opus-4-1" });
  expect(badges.querySelector('[data-badge="model"]')).toBe(model);
  expect(model!.textContent).toBe("Opus 4.1");
});

test("a badge whose data went away is removed, and the survivors keep their order", () => {
  display({ agent_icons: false, terminal_icons: false, model: true });
  const badges = document.createElement("span");
  main.fillBadges(badges, { ...session, mode: "bypassPermissions" });
  expect([...badges.children].map((pill) => (pill as HTMLElement).dataset.badge)).toEqual([
    "mode",
    "agent",
    "model",
    "terminal",
  ]);
  main.fillBadges(badges, { ...session, model: undefined });
  expect([...badges.children].map((pill) => (pill as HTMLElement).dataset.badge)).toEqual([
    "agent",
    "terminal",
  ]);
});

test("the mode badge only appears while permissions are bypassed", () => {
  display({});
  expect(main.badgeSpec(session).map(([key]) => key)).not.toContain("mode");
  const bypassed = main.badgeSpec({ ...session, mode: "bypassPermissions" });
  expect(bypassed[0]).toEqual(["mode", "BYPASS", "bypass"]);
});

test("a session holding a pending permission says so on its own row", () => {
  display({ activity: true });
  const li = main.createRow(
    { ...session, current_tool: "Bash", summary: "rm -rf build", permission_state: "pending" },
    false,
  );
  expect(li.querySelector(".row-activity-label")!.textContent).toBe("esperando sua permissão");
});

test("a pending question outranks a pending permission on the same row", () => {
  display({ activity: true });
  const li = main.createRow(
    { ...session, permission_state: "pending", question_state: "pending" },
    false,
  );
  expect(li.querySelector(".row-activity-label")!.textContent).toBe("esperando sua resposta");
});

test("a settled permission gives the row's third line back to the tool", () => {
  display({ activity: true });
  const li = main.createRow(
    { ...session, current_tool: "Bash", summary: "rm -rf build", permission_state: "allowed" },
    false,
  );
  expect(li.querySelector(".row-activity-label")!.textContent).toBe("Bash rm -rf build");
});

test("a blocked session shows its third line even with the activity detail off", () => {
  display({ activity: false });
  const li = main.createRow({ ...session, permission_state: "pending" }, false);
  const activity = li.querySelector<HTMLElement>(".row-activity")!;
  expect(activity.hidden).toBe(false);
  expect(li.querySelector(".row-activity-label")!.textContent).toBe("esperando sua permissão");
});

test("the activity detail toggle still hides the line of a session nobody is waiting on", () => {
  display({ activity: false });
  const li = main.createRow({ ...session, current_tool: "Bash" }, false);
  expect(li.querySelector<HTMLElement>(".row-activity")!.hidden).toBe(true);
});
