import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

let safeTop = 32;
const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return {
    // Reproduce the value sent by the original native macOS backend.
    scale: 1, compact_height: 46, safe_top: safeTop, notch_width: safeTop ? 220 : 0,
  };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  return {};
});
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
await import("../main");
const settle = () => new Promise((resolve) => setTimeout(resolve, 50));
await settle();
const sizes = () => tauri.calls.filter((call) => call.command === "set_island_size");

test("a notch reserves the camera and uses a slim compact strip", () => {
  expect(document.body.classList.contains("has-notch")).toBe(true);
  expect(document.documentElement.style.getPropertyValue("--camera-top")).toBe("32px");
  expect(sizes().pop()?.args).toEqual({ width: 232, height: 30 });
});

test("moving to a screen without a notch restores the usual compact geometry", async () => {
  safeTop = 0;
  tauri.emit("island-screen-changed", {});
  await settle();
  expect(document.body.classList.contains("has-notch")).toBe(false);
  expect(document.documentElement.style.getPropertyValue("--camera-top")).toBe("0px");
  expect(sizes().pop()?.args).toEqual({ width: 232, height: 46 });
});
