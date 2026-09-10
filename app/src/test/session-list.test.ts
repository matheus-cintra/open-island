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

function renderedBadgeKeys(li: HTMLLIElement): string[] {
  return [...li.querySelectorAll<HTMLElement>("[data-badge]")].map((element) =>
    String(element.dataset.badge),
  );
}

function activityText(li: HTMLLIElement): string {
  return li.querySelector<HTMLElement>(".row-activity")!.textContent ?? "";
}

test("a sessions-updated event renders one row per session, in the emitted order", () => {
  tauri.emit("sessions-updated", [sessionA, sessionB]);
  expect(rows().map((li) => li.dataset.sessionId)).toEqual(["s1", "s2"]);
});

test("each rendered row carries the badges its session asks for", () => {
  tauri.emit("sessions-updated", [sessionA, sessionB]);
  expect(renderedBadgeKeys(rows()[0])).toEqual(main.badgeSpec(sessionA).map(([key]) => key));
  expect(renderedBadgeKeys(rows()[1])).toEqual(main.badgeSpec(sessionB).map(([key]) => key));
});

test("every rendered row keeps an activity line", () => {
  tauri.emit("sessions-updated", [sessionA, sessionB]);
  expect(activityText(rows()[0])).not.toBe("");
  expect(activityText(rows()[1])).not.toBe("");
});

test("a session missing from the next event loses its row", () => {
  tauri.emit("sessions-updated", [sessionA, sessionB]);
  expect(rows()).toHaveLength(2);
  tauri.emit("sessions-updated", [sessionA]);
  expect(rows().map((li) => li.dataset.sessionId)).toEqual(["s1"]);
});
