import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";

type ConfigDocument = Record<string, Record<string, unknown>>;

let onDisk: ConfigDocument = {
  display: { completion_card_height: 100, content_font: 11, panel_max_height: 720 },
  island: { hover_dwell_ms: 250 },
  sound: { quiet: false },
};
const writes: ConfigDocument[] = [];
let readFails = false;

const tauri = tauriMock(({ command, args }) => {
  if (command === "get_config") {
    if (readFails) throw new Error("daemon is down");
    return { config: structuredClone(onDisk), env_locked: {} };
  }
  if (command === "save_config") {
    onDisk = structuredClone(args.config as ConfigDocument);
    writes.push(structuredClone(onDisk));
    return null;
  }
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountSettings();
const settings = await import("../settings");
await new Promise((resolve) => setTimeout(resolve, 50));

function lastWrite(): ConfigDocument {
  return writes[writes.length - 1]!;
}

test("a save carries the touched path onto the document as it is on disk right now", async () => {
  settings.setValue("display.content_font", 13);
  onDisk.display.completion_card_height = 170;
  await settings.flushSave();
  expect(lastWrite().display.content_font).toBe(13);
  expect(lastWrite().display.completion_card_height).toBe(170);
});

test("a path the window never touched is never written back", async () => {
  onDisk.island.hover_dwell_ms = 900;
  settings.setValue("sound.quiet", true);
  await settings.flushSave();
  expect(lastWrite().island.hover_dwell_ms).toBe(900);
  expect(lastWrite().sound.quiet).toBe(true);
});

test("two edits in one debounce window both land", async () => {
  settings.setValue("display.content_font", 15);
  settings.setValue("display.panel_max_height", 480);
  await settings.flushSave();
  expect(lastWrite().display.content_font).toBe(15);
  expect(lastWrite().display.panel_max_height).toBe(480);
});

test("the dirty set is cleared, so the next save carries nothing stale", async () => {
  settings.setValue("display.content_font", 17);
  await settings.flushSave();
  onDisk.display.content_font = 12;
  await settings.flushSave();
  expect(lastWrite().display.content_font).toBe(12);
});

test("a failed read writes nothing at all", async () => {
  const before = writes.length;
  settings.setValue("display.content_font", 19);
  readFails = true;
  await expect(settings.flushSave()).rejects.toThrow("daemon is down");
  readFails = false;
  expect(writes.length).toBe(before);
});
