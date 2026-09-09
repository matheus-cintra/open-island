import { expect, mock, test } from "bun:test";
import { mountSettings } from "./dom";
import { tauriMock } from "./tauri";

type ConfigDocument = Record<string, Record<string, unknown>>;

let update: { version: string } | null = null;
let checkResult: { version: string | null } = { version: null };
let checkEnabled = true;
const writes: ConfigDocument[] = [];

const tauri = tauriMock(({ command, args }) => {
  if (command === "get_config") {
    return { config: { updates: { check_enabled: checkEnabled } }, env_locked: {} };
  }
  if (command === "get_update") return update;
  if (command === "check_update") return checkResult;
  if (command === "save_config") {
    writes.push(structuredClone(args.config as ConfigDocument));
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

function aboutPane(): HTMLElement {
  document.querySelector<HTMLElement>('[data-pane="about"]')!.click();
  return document.getElementById("pane-about")!;
}

function labels(pane: HTMLElement): string[] {
  return [...pane.querySelectorAll(".row-label")].map((label) => label.textContent ?? "");
}

function checkSwitch(pane: HTMLElement): HTMLInputElement {
  return pane.querySelector<HTMLInputElement>("#control-updates\\.check_enabled")!;
}

test("with no update available the notice row is absent and the switch is on", () => {
  const pane = aboutPane();
  expect(labels(pane)).not.toContain("Versão nova disponível");
  expect(labels(pane)).toContain("Avisar quando sair uma versão nova");
  expect(checkSwitch(pane).checked).toBe(true);
});

test("an update-available event makes the notice row appear with the new version", () => {
  tauri.emit("update-available", { version: "v0.2.0" });
  const pane = document.getElementById("pane-about")!;
  expect(labels(pane)[0]).toBe("Versão nova disponível");
  expect(pane.querySelector(".row-value")!.textContent).toBe("v0.2.0");
  expect(pane.querySelector(".row-hint")!.textContent).toContain(
    "curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh",
  );
});

test("flipping the switch writes updates.check_enabled", async () => {
  const pane = document.getElementById("pane-about")!;
  const input = checkSwitch(pane);
  input.checked = false;
  input.dispatchEvent(new window.Event("change"));
  await settings.flushSave();
  expect(writes[writes.length - 1]!.updates.check_enabled).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 300));
});

test("the switch reads off when the config on disk says so", async () => {
  const before = checkSwitch(document.getElementById("pane-about")!);
  before.checked = true;
  checkEnabled = false;
  tauri.emit("config-changed", {});
  await new Promise((resolve) => setTimeout(resolve, 50));
  const after = checkSwitch(document.getElementById("pane-about")!);
  expect(after).not.toBe(before);
  expect(after.checked).toBe(false);
});

test("a pane opened after the daemon already knows about the update shows it from the pull", async () => {
  update = { version: "v0.3.0" };
  tauri.emit("settings-revealed", {});
  await new Promise((resolve) => setTimeout(resolve, 50));
  const pane = document.getElementById("pane-about")!;
  expect(pane.querySelector(".row-value")!.textContent).toBe("v0.3.0");
});

function checkButton(pane: HTMLElement): HTMLButtonElement {
  return [...pane.querySelectorAll<HTMLButtonElement>("button.row-button")].find(
    (button) => button.textContent === "Buscar agora",
  )!;
}

test("the check-now action asks the daemon and shows the newer version it found", async () => {
  update = null;
  tauri.emit("settings-revealed", {});
  await new Promise((resolve) => setTimeout(resolve, 50));
  checkResult = { version: "v0.4.0" };
  checkButton(document.getElementById("pane-about")!).click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const pane = document.getElementById("pane-about")!;
  expect(tauri.calls.filter((call) => call.command === "check_update").length).toBe(1);
  expect(pane.querySelector(".row-value")!.textContent).toBe("v0.4.0");
  expect(document.getElementById("toast")!.textContent).toBe("Versão v0.4.0 disponível.");
});

test("the check-now action says so when the running build is already the newest", async () => {
  checkResult = { version: null };
  checkButton(document.getElementById("pane-about")!).click();
  await new Promise((resolve) => setTimeout(resolve, 50));
  const pane = document.getElementById("pane-about")!;
  expect(labels(pane)).not.toContain("Versão nova disponível");
  expect(document.getElementById("toast")!.textContent).toBe("Você já está na versão mais recente.");
});
