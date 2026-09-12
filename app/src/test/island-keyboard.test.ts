import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock(({ command }) => {
  if (command === "platform_capabilities") return { os: "macos" };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  return {};
});
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
await import("../main");
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
await settle();
const keyboard = () => tauri.calls.filter((call) => call.command === "island_keyboard").map((call) => call.args.active);
const root = document.getElementById("island")!;

test("expansion on macOS leaves keyboard in the other application", async () => {
  tauri.emit("island-toggle", {});
  await settle();
  expect(root.classList.contains("expanded")).toBe(true);
  expect(keyboard()).toEqual([]);
});

test("clicking a field takes keyboard; Escape releases it while the island stays open", async () => {
  const input = document.createElement("textarea");
  root.append(input);
  input.dispatchEvent(new window.Event("pointerdown", { bubbles: true }));
  input.focus();
  await settle();
  expect(keyboard()).toEqual([true]);
  input.blur(); // Message input's Escape handler blurs the field.
  await settle();
  expect(keyboard()).toEqual([true, false]);
  expect(root.classList.contains("expanded")).toBe(true);
  input.remove();
});

test("removing the focused question releases keyboard without waiting for collapse", async () => {
  const input = document.createElement("input");
  root.append(input);
  input.focus();
  await settle();
  expect(keyboard().pop()).toBe(true);
  input.remove();
  await settle();
  expect(keyboard().pop()).toBe(false);
});

test("collapsing clears DOM focus and reopening does not recapture keyboard", async () => {
  const input = document.createElement("textarea");
  root.append(input);
  input.focus();
  await settle();
  tauri.emit("island-toggle", {});
  await settle();
  expect(document.activeElement).not.toBe(input);
  expect(keyboard().pop()).toBe(false);
  const count = keyboard().length;
  tauri.emit("island-toggle", {});
  await settle();
  expect(keyboard().length).toBe(count);
  input.remove();
});
