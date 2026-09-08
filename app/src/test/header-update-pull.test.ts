import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  if (command === "get_update") return { version: "v0.3.0" };
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");
await new Promise((resolve) => setTimeout(resolve, 50));

test("an island started after the daemon already knows the version shows the icon from the pull", () => {
  const button = document.getElementById("header-update") as HTMLButtonElement;
  expect(button.hidden).toBe(false);
  expect(button.title).toBe("v0.3.0 disponível. Clique para atualizar.");
});
