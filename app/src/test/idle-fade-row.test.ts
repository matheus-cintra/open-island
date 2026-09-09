import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";
import { strings } from "../strings";

type ConfigDocument = Record<string, Record<string, unknown>>;

let onDisk: ConfigDocument = { island: { idle_fade_ms: 30000 } };
const writes: ConfigDocument[] = [];

const tauri = tauriMock(({ command, args }) => {
  if (command === "get_config") return { config: structuredClone(onDisk), env_locked: {} };
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

const copy = strings.settings.general;

function islandCard(): HTMLElement {
  return [...document.querySelectorAll<HTMLElement>(".section")].find(
    (section) => section.querySelector(".section-title")?.textContent === copy.island,
  )!;
}

function delayRow(): HTMLElement {
  return [...islandCard().querySelectorAll<HTMLElement>(".row")].find(
    (row) => row.querySelector(".row-label")?.textContent === copy.idleFadeAfter,
  )!;
}

function fadeSwitch(): HTMLInputElement {
  return document.getElementById("control-island.idle_fade") as HTMLInputElement;
}

test("the delay row is hidden while the switch is off", () => {
  expect(fadeSwitch().checked).toBe(false);
  expect(delayRow().hidden).toBe(true);
});

test("flipping the switch reveals the delay row", () => {
  const input = fadeSwitch();
  input.checked = true;
  input.dispatchEvent(new window.Event("change"));

  expect(delayRow().hidden).toBe(false);
});

test("flipping the switch writes island.idle_fade and the delay together", async () => {
  const delay = delayRow().querySelector<HTMLInputElement>("input.field")!;
  delay.value = "45";
  delay.dispatchEvent(new window.Event("change"));
  await settings.flushSave();

  const written = writes[writes.length - 1]!;
  expect(written.island.idle_fade).toBe(true);
  expect(written.island.idle_fade_ms).toBe(45000);
});
