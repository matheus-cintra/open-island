import { expect, mock, test } from "bun:test";
import { mountIsland, setScreenHeight } from "./dom";
import { tauriMock } from "./tauri";

const tauri = tauriMock();
mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");

const list = document.getElementById("session-list")!;

test("the ceiling is a third of the screen", () => {
  setScreenHeight(1080);
  expect(main.maxExpandedHeight()).toBe(360);
});

test("the ceiling never passes the configured panel maximum", () => {
  setScreenHeight(2400);
  expect(main.maxExpandedHeight()).toBe(720);
});

test("the ceiling never falls under the floor a short screen would give", () => {
  setScreenHeight(600);
  expect(main.maxExpandedHeight()).toBe(316);
});

test("an empty list floors at the base expanded height", () => {
  setScreenHeight(1080);
  list.replaceChildren();
  expect(main.expandedSize()).toEqual({ w: 664, h: 158 });
});

test("a list with a row floors at the compact height instead", () => {
  setScreenHeight(1080);
  const row = document.createElement("li");
  list.replaceChildren(row);
  expect(main.expandedSize().h).toBe(46);
  list.replaceChildren();
});

test("a row on its way out does not hold the floor up", () => {
  setScreenHeight(1080);
  const row = document.createElement("li");
  row.className = "is-leaving";
  list.replaceChildren(row);
  expect(main.expandedSize().h).toBe(158);
  list.replaceChildren();
});
