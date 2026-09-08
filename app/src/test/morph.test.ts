import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock();
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

const noop = (): void => {};
const island = document.getElementById("island")!;

function morph(height: number): string {
  main.morphTo({ w: 664, h: height }, noop, noop);
  return island.style.getPropertyValue("--morph");
}

test("opening from the compact pill runs the cross-fade", () => {
  expect(morph(46)).toBe("0.000");
  expect(morph(360)).toBe("1.000");
});

test("shrinking between two expanded sizes leaves the panel fully open", () => {
  morph(46);
  morph(360);
  expect(morph(200)).toBe("1");
});

test("closing to the compact pill still cross-fades back", () => {
  morph(46);
  morph(360);
  morph(200);
  expect(morph(46)).toBe("0.000");
});
