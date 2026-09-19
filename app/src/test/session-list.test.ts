import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const TERMINAL_ICON = "asset://kitty.png";

const tauri = tauriMock(({ command }) => (command === "terminal_icon" ? TERMINAL_ICON : {}));
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

const sessionA = {
  id: "s1",
  agent: "claude",
  cwd: "/home/user/dev/open-island",
  title: "open-island",
  pid: 4242,
  terminal: "kitty" as const,
  model: "claude-sonnet-4-5-20250929",
  attention: "working" as const,
};

const sessionB = {
  id: "s2",
  agent: "codex",
  cwd: "/home/user/dev/outra-ilha",
  title: "outra-ilha",
  pid: 4343,
  terminal: "ghostty" as const,
  model: "gpt-5-codex",
  attention: "needs_attention" as const,
};

const list = document.getElementById("session-list")!;

function rows(): HTMLLIElement[] {
  return [...list.children] as HTMLLIElement[];
}

function activityText(li: HTMLLIElement): string {
  return li.querySelector<HTMLElement>(".row-activity")!.textContent ?? "";
}

test("a sessions-updated event renders one row per session, in the emitted order", () => {
  tauri.state.sessions([sessionA, sessionB]);
  expect(rows().map((li) => li.dataset.sessionId)).toEqual(["s1", "s2"]);
});

test("each rendered row carries its project, its meta and its state", () => {
  if (!main.expanded) tauri.emit("island-toggle", {});
  tauri.state.sessions([sessionA, sessionB]);
  expect(rows()[0].querySelector(".row-project")!.textContent).toBe("open-island");
  expect(rows()[0].querySelector(".meta-model")!.textContent).toBe("Sonnet 4.5");
  expect(rows()[0].querySelector<HTMLImageElement>(".meta-terminal-icon")!.src).toBe(TERMINAL_ICON);
  expect(rows()[0].querySelector(".row-state")).toBeNull();
  expect(rows()[1].querySelector(".meta-model")!.textContent).toBe("gpt-5-codex");
  expect(rows()[1].querySelector(".row-state")!.textContent).toBe("concluído");
});

test("every rendered row keeps an activity line", () => {
  tauri.state.sessions([sessionA, sessionB]);
  expect(activityText(rows()[0])).not.toBe("");
  expect(activityText(rows()[1])).not.toBe("");
});

test("a session missing from the next event loses its row", () => {
  tauri.state.sessions([sessionA, sessionB]);
  expect(rows()).toHaveLength(2);
  tauri.state.sessions([sessionA]);
  expect(rows().map((li) => li.dataset.sessionId)).toEqual(["s1"]);
});
