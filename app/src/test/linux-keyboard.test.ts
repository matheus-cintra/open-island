import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock((call) => {
  if (call.command === "platform_capabilities") return { os: "linux" };
  if (call.command === "list_sessions") return [];
  if (call.command === "get_usage") return { providers: [] };
  if (call.command === "get_update") return null;
  return {};
});
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
await import("../main");
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
await settle();

const root = document.getElementById("island")!;
const keyboard = () => tauri.calls
  .filter((call) => call.command === "island_keyboard")
  .map((call) => call.args.active);

test("Linux manual focus releases a focused field on collapse and activates cleanly on reopen", async () => {
  tauri.emit("island-toggle", {});
  await settle();
  const input = document.createElement("textarea");
  root.append(input);
  input.focus();
  tauri.emit("island-toggle", {});
  await settle();
  expect(document.activeElement).not.toBe(input);
  expect(keyboard()).toEqual([true, false]);

  tauri.emit("island-toggle", {});
  await settle();
  expect(keyboard()).toEqual([true, false, true]);
  expect(document.activeElement).not.toBe(input);
  input.remove();
});
