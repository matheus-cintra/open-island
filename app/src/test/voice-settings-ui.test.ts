import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";
import { linuxCapabilities } from "../platform-capabilities";
import { strings } from "../strings";

let configured = true;
const tauri = tauriMock(({ command }) => {
  if (command === "platform_capabilities") return linuxCapabilities;
  if (command === "get_config") return { config: {}, env_locked: {} };
  if (command === "sound_theme_files" || command === "list_monitors" || command === "list_sessions") return [];
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "user_sound_dir") return "";
  if (command === "get_update") return null;
  if (command === "plugin:voice|voice_model_status") return { configured, error: null };
  if (command === "plugin:voice|voice_clear_model") { configured = false; return { configured, error: "model_unavailable" }; }
  return {};
});
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));
mountSettings();
await import("../settings");
const settle = (): Promise<void> => new Promise((resolve) => setTimeout(resolve, 30));
await settle();

test("voice pane reads local status and confirms removal without daemon config writes", async () => {
  document.querySelector<HTMLButtonElement>('[data-pane="voice"]')!.click(); await settle();
  expect(document.body.textContent).toContain(strings.voice.configured);
  const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>(".row-button"));
  expect(buttons.some((button) => button.textContent === strings.voice.privacy)).toBe(false);
  const remove = buttons.find((button) => button.textContent === strings.voice.removeModel)!;
  remove.click(); await settle();
  expect(remove.textContent).toBe(strings.voice.removeConfirm);
  expect(tauri.calls.filter((call) => call.command === "plugin:voice|voice_clear_model")).toHaveLength(0);
  remove.click(); await settle();
  expect(tauri.calls.filter((call) => call.command === "plugin:voice|voice_clear_model")).toHaveLength(1);
  expect(tauri.calls.filter((call) => call.command === "save_config")).toHaveLength(0);
  expect(document.body.textContent).toContain(strings.voice.error("model_unavailable"));
});
