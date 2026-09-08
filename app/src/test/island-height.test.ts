import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

let compositorHeight: number | null = null;

const tauri = tauriMock((call) =>
  call.command === "island_metrics" ? { scale: 1, compact_height: compositorHeight } : {},
);
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

interface IslandSize {
  width: number;
  height: number;
}

async function settle(): Promise<void> {
  for (let tick = 0; tick < 8; tick += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

async function applyIslandHeight(
  islandHeight: number,
  reservedCompactHeight: number | null,
): Promise<IslandSize> {
  compositorHeight = reservedCompactHeight;
  main.applyConfig({
    island: {},
    notifications: {},
    display: { ui_scale: 1, island_height: islandHeight },
  });
  await settle();
  const sized = tauri.calls.filter((call) => call.command === "set_island_size");
  const last = sized[sized.length - 1];
  if (last === undefined) throw new Error("the island was never sized");
  return last.args as unknown as IslandSize;
}

test("zero leaves the compositor-derived height untouched", async () => {
  expect(await applyIslandHeight(0, 36)).toEqual({ width: 232, height: 36 });
});

test("a non-zero island height wins over the compositor height", async () => {
  expect(await applyIslandHeight(60, 36)).toEqual({ width: 232, height: 60 });
});

test("going back to zero returns the compositor height", async () => {
  expect(await applyIslandHeight(0, 36)).toEqual({ width: 232, height: 36 });
});

test("zero without a compositor height still falls back to the base compact height", async () => {
  expect(await applyIslandHeight(0, null)).toEqual({ width: 232, height: 46 });
});

test("a non-zero island height replaces the base compact fallback too", async () => {
  expect(await applyIslandHeight(60, null)).toEqual({ width: 232, height: 60 });
});
