import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

type ConfigDocument = Record<string, Record<string, unknown>>;

let onDisk: ConfigDocument = {
  display: { content_font: 11 },
  sound: { quiet: false, volume: 0.85 },
};
const writes: ConfigDocument[] = [];

const tauri = tauriMock(({ command, args }) => {
  if (command === "get_config") return { config: structuredClone(onDisk) };
  if (command === "save_config") {
    onDisk = structuredClone(args.config as ConfigDocument);
    writes.push(structuredClone(onDisk));
    return null;
  }
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "list_sessions") return [];
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");
await new Promise((resolve) => setTimeout(resolve, 50));

const mute = document.getElementById("header-mute") as HTMLButtonElement;

function lastWrite(): ConfigDocument {
  return writes[writes.length - 1]!;
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 20));
}

test("muting writes quiet onto the document as it is on disk right now", async () => {
  onDisk.display.content_font = 15;
  mute.click();
  await settle();
  expect(lastWrite().sound.quiet).toBe(true);
  expect(lastWrite().display.content_font).toBe(15);
});

test("unmuting still leaves every key the island never touched alone", async () => {
  onDisk.sound.volume = 0.4;
  mute.click();
  await settle();
  expect(lastWrite().sound.quiet).toBe(false);
  expect(lastWrite().sound.volume).toBe(0.4);
});
