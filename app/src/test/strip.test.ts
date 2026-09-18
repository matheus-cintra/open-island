import { expect, test } from "bun:test";
import { mountIsland } from "./dom";

mountIsland({ reducedMotion: true });
const { StateStrip } = await import("../strip");

type Attention = "waiting_for_input" | "needs_attention" | "working" | "idle";
const session = (id: string, attention?: Attention) => ({
  id, agent: "claude", cwd: "/w", title: id, pid: 1, terminal: "kitty" as const, attention,
});

test("one cell per session, coloured by attention, in list order", () => {
  const root = document.createElement("span");
  const strip = new StateStrip(root);
  strip.update([session("a", "waiting_for_input"), session("b"), session("c", "idle")], false);
  expect([...root.children].map((cell) => cell.className)).toEqual([
    "strip-cell waiting_for_input", "strip-cell working", "strip-cell idle",
  ]);
  expect(root.getAttribute("aria-label")).toBe("3 sessões");
  expect(root.hidden).toBe(false);
});

test("reordering keeps the same nodes and removing drops them", () => {
  const root = document.createElement("span");
  const strip = new StateStrip(root);
  strip.update([session("a"), session("b")], false);
  const [a, b] = [...root.children];
  strip.update([session("b", "needs_attention"), session("a")], false);
  expect(root.children[0]).toBe(b);
  expect(root.children[1]).toBe(a);
  expect(b.className).toBe("strip-cell needs_attention");
  strip.update([session("a")], false);
  expect(root.children.length).toBe(1);
  expect(root.children[0]).toBe(a);
  strip.update([], false);
  expect(root.hidden).toBe(true);
});

test("more than twelve sessions cap at twelve cells plus an overflow mark", () => {
  const root = document.createElement("span");
  const strip = new StateStrip(root);
  strip.update(Array.from({ length: 15 }, (_, i) => session(`s${i}`)), false);
  expect(root.children.length).toBe(13);
  expect(root.lastElementChild!.className).toBe("strip-cell more");
  strip.update(Array.from({ length: 3 }, (_, i) => session(`s${i}`)), true);
  expect(root.children.length).toBe(3);
  expect(root.classList.contains("is-off")).toBe(true);
});
