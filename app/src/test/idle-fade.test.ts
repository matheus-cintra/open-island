import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock();
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

const island = document.getElementById("island") as HTMLElement;

const wait = (milliseconds: number): Promise<void> =>
  new Promise<void>((resolve) => setTimeout(resolve, milliseconds));

test("the island stays visible when the config carries no idle fade switch", async () => {
  main.applyConfig({ island: { idle_fade_ms: 20 } });
  await wait(80);
  expect(island.style.opacity).toBe("1");
});

test("the island fades out with the switch on and comes back on pointer motion", async () => {
  main.applyConfig({ island: { idle_fade: true, idle_fade_ms: 20, hide_when_idle: false } });
  await wait(80);
  expect(island.style.opacity).toBe("0");
  window.dispatchEvent(new window.Event("pointermove"));
  expect(island.style.opacity).toBe("1");
  await wait(80);
  expect(island.style.opacity).toBe("0");
});

test("turning the switch off brings a faded island back and keeps it back", async () => {
  main.applyConfig({ island: { idle_fade: true, idle_fade_ms: 20, hide_when_idle: false } });
  await wait(80);
  expect(island.style.opacity).toBe("0");
  main.applyConfig({ island: { idle_fade: false, idle_fade_ms: 20, hide_when_idle: false } });
  expect(island.style.opacity).toBe("1");
  await wait(80);
  expect(island.style.opacity).toBe("1");
});
