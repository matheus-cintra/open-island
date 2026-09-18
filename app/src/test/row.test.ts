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
  tauri.state.config({ display: values });
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

function metaText(li: HTMLLIElement): string {
  return li.querySelector(".row-meta")!.textContent ?? "";
}

function openTasks(li: HTMLLIElement): void {
  li.querySelector<HTMLButtonElement>('[data-chip="tasks"]')!.click();
}

test("the tasks chip counts done over total and opens the checklist on click", () => {
  display({ tasks: true });
  const li = main.createRow(session, false);
  const chip = li.querySelector<HTMLButtonElement>('[data-chip="tasks"]')!;
  expect(chip.hidden).toBe(false);
  expect(chip.querySelector(".chip-text")!.textContent).toBe("Tarefas 1/3");
  expect(chip.querySelector<HTMLElement>(".chip-fill")!.style.width).toBe("33%");
  const block = li.querySelector<HTMLElement>(".row-tasks")!;
  expect(block.hidden).toBe(true);
  openTasks(li);
  expect(block.hidden).toBe(false);
  expect(chip.getAttribute("aria-expanded")).toBe("true");
  expect(li.querySelectorAll(".task-item").length).toBe(3);
  openTasks(li);
  expect(block.hidden).toBe(true);
});

test("a cancelled task closes rather than staying open", () => {
  display({ tasks: true });
  const li = main.createRow(
    {
      ...session,
      id: "s-cancelled",
      tasks: [
        { content: "Ler o roadmap", status: "cancelled" as const },
        { content: "Provar a regressão", status: "pending" as const },
      ],
    },
    false,
  );
  expect(li.querySelector(".chip-text")!.textContent).toBe("Tarefas 1/2");
});

test("each task carries its own status mark", () => {
  display({ tasks: true });
  const li = main.createRow({ ...session, id: "s-marks" }, false);
  openTasks(li);
  const marks = [...li.querySelectorAll(".task-mark")].map((mark) => mark.className);
  expect(marks).toEqual([
    "task-mark is-completed",
    "task-mark is-in-progress",
    "task-mark is-pending",
  ]);
});

test("the tasks chip is hidden when display.tasks is off", () => {
  display({ tasks: false });
  const li = main.createRow({ ...session, id: "s-off" }, false);
  expect(li.querySelector<HTMLElement>('[data-chip="tasks"]')!.hidden).toBe(true);
  expect(li.querySelector<HTMLElement>(".row-chips")!.hidden).toBe(true);
  expect(li.querySelectorAll(".task-item").length).toBe(0);
});

test("a session with no tasks hides the chips even when the toggle is on", () => {
  display({ tasks: true });
  const li = main.createRow({ ...session, id: "s-none", tasks: undefined }, false);
  expect(li.querySelector<HTMLElement>(".row-chips")!.hidden).toBe(true);
});

test("the mascot is the agent sprite by default and its logo when asked", () => {
  display({ mascot: "sprite" });
  const li = main.createRow(session, false);
  const sprite = li.querySelector(".row-mascot")!;
  expect(sprite.tagName.toLowerCase()).toBe("svg");
  expect(sprite.getAttribute("data-agent")).toBe("claude");
  display({ mascot: "logo" });
  main.fillRow(li, session, false);
  const logo = li.querySelector(".row-mascot")!;
  expect(logo.tagName.toLowerCase()).toBe("img");
  display({ mascot: "sprite" });
});

test("an agent without a sprite of its own falls back to its logo, and without a logo to the generic sprite", () => {
  display({ mascot: "sprite" });
  const cursor = main.createRow({ ...session, id: "s-cursor", agent: "cursor" }, false);
  expect(cursor.querySelector(".row-mascot")!.tagName.toLowerCase()).toBe("img");
  const unknown = main.createRow({ ...session, id: "s-unknown", agent: "amazonq" }, false);
  const sprite = unknown.querySelector(".row-mascot")!;
  expect(sprite.tagName.toLowerCase()).toBe("svg");
  expect(sprite.getAttribute("data-agent")).toBe("unknown");
});

test("the terminal shows as text until the icon lookup answers, then as the icon", async () => {
  display({ terminal_icons: true, model: false });
  const li = main.createRow({ ...session, id: "s-terminal" }, false);
  await Promise.resolve();
  expect(tauri.calls.some((call) => call.command === "terminal_icon")).toBe(true);
  main.fillRow(li, { ...session, id: "s-terminal" }, false);
  expect(li.querySelector<HTMLImageElement>(".meta-terminal-icon")!.src).toBe(TERMINAL_ICON);
  display({ terminal_icons: false, model: false });
  main.fillRow(li, { ...session, id: "s-terminal" }, false);
  expect(li.querySelector(".meta-terminal-icon")).toBeNull();
  expect(metaText(li)).toBe("kitty");
});

test("the meta line reads branch, model and terminal separated by dots", () => {
  display({ terminal_icons: false, model: true, worktree: true });
  const li = main.createRow({ ...session, id: "s-meta", branch: "feat/x" }, false);
  expect(metaText(li)).toBe("feat/x·Sonnet 4.5·kitty");
  main.fillRow(li, { ...session, id: "s-meta", branch: "feat/x", model: "claude-opus-4-1" }, false);
  expect(metaText(li)).toBe("feat/x·Opus 4.1·kitty");
});

test("the bypass chip only appears while permissions are bypassed", () => {
  display({});
  const li = main.createRow({ ...session, id: "s-bypass" }, false);
  expect(li.querySelector(".row-state.bypass")).toBeNull();
  main.fillRow(li, { ...session, id: "s-bypass", mode: "bypassPermissions" }, false);
  expect(li.querySelector(".row-state.bypass")!.textContent).toBe("BYPASS");
});

test("a working row has no state chip; waiting, finished and idle rows do", () => {
  display({});
  const li = main.createRow({ ...session, id: "s-state" }, false);
  expect(li.querySelector(".row-tail .row-state")).toBeNull();
  main.fillRow(li, { ...session, id: "s-state", attention: "waiting_for_input" }, false);
  expect(li.querySelector(".row-tail .row-state")!.textContent).toBe("esperando você");
  main.fillRow(li, { ...session, id: "s-state", attention: "needs_attention" }, false);
  expect(li.querySelector(".row-tail .row-state")!.textContent).toBe("concluído");
  main.fillRow(li, { ...session, id: "s-state", attention: "idle" }, false);
  expect(li.querySelector(".row-tail .row-state")!.textContent).toBe("parada");
});

test("the second line shows the prompt, or the shortened path when there is none", () => {
  display({});
  const li = main.createRow({ ...session, id: "s-prompt" }, false);
  const prompt = li.querySelector<HTMLElement>(".row-prompt")!;
  expect(prompt.textContent).toBe("~/dev/open-island");
  expect(prompt.classList.contains("is-cwd")).toBe(true);
  main.fillRow(li, { ...session, id: "s-prompt", summary: "consertar o fade" }, false);
  expect(prompt.textContent).toBe("Você: consertar o fade");
  expect(prompt.classList.contains("is-cwd")).toBe(false);
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

test("a settled permission gives the activity back to the tool alone", () => {
  display({ activity: true });
  const li = main.createRow(
    { ...session, current_tool: "Bash", summary: "rm -rf build", permission_state: "allowed" },
    false,
  );
  expect(li.querySelector(".row-activity-label")!.textContent).toBe("Bash");
});

test("a blocked session shows its activity even with the activity detail off", () => {
  display({ activity: false });
  const li = main.createRow({ ...session, permission_state: "pending" }, false);
  const activity = li.querySelector<HTMLElement>(".row-activity")!;
  expect(activity.hidden).toBe(false);
  expect(li.querySelector(".row-activity-label")!.textContent).toBe("esperando sua permissão");
});

test("the activity detail toggle still hides the tool of a session nobody is waiting on", () => {
  display({ activity: false });
  const li = main.createRow({ ...session, current_tool: "Bash" }, false);
  expect(li.querySelector<HTMLElement>(".row-activity")!.hidden).toBe(true);
});
