import { expect, mock, test } from "bun:test";
import { mountIsland, paintFrame } from "./dom";
import { tauriMock } from "./tauri";

interface Metrics { scale: number; compact_height: number | null; safe_top?: number; notch_width?: number; }
let controlled = false;
const pending: ((metrics: Metrics) => void)[] = [];
const tauri = tauriMock(({ command }): unknown => {
  if (command === "island_metrics") return controlled ? new Promise<Metrics>((resolve): void => { pending.push(resolve); }) : { scale: 1, compact_height: null };
  return {};
});
mock.module("@tauri-apps/api/core", (): object => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", (): object => ({ listen: tauri.listen }));
mountIsland({ reducedMotion: true });
const main = await import("../main");
await new Promise((resolve): void => { setTimeout(resolve, 30); });

test("a delayed metrics reply cannot apply geometry from a replaced configuration", async (): Promise<void> => {
  controlled = true;
  main.applyConfig({ display: { ui_scale: 1 } }); paintFrame();
  await new Promise((resolve): void => { setTimeout(resolve, 0); });
  expect(pending).toHaveLength(1);
  const previousZoom = document.getElementById("island")!.style.zoom;
  main.applyConfig({ display: { ui_scale: 2 } });
  pending.shift()!({ scale: 3, compact_height: 99, safe_top: 40, notch_width: 200 });
  await new Promise((resolve): void => { setTimeout(resolve, 0); });
  expect(document.body.classList.contains("has-notch")).toBe(false);
  expect(document.getElementById("island")!.style.zoom).toBe(previousZoom);
  paintFrame();
  await new Promise((resolve): void => { setTimeout(resolve, 0); });
  expect(pending).toHaveLength(1);
  pending.shift()!({ scale: 1, compact_height: null });
  await new Promise((resolve): void => { setTimeout(resolve, 0); });
  expect(document.getElementById("island")!.style.zoom).toBe("2");
});
