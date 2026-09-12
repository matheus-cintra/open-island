import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

let terminalMissing = false;
let pendingUpdate: Promise<unknown> | null = null;

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  if (command === "get_update") return null;
  if (command === "run_update" && terminalMissing) throw new Error("no terminal emulator found on PATH");
  if (command === "run_update" && pendingUpdate) return pendingUpdate;
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");
await new Promise((resolve) => setTimeout(resolve, 50));

const button = document.getElementById("header-update") as HTMLButtonElement;
const error = document.getElementById("jump-error")!;

test("the island asks the daemon for a pending update at start and keeps the icon hidden when there is none", () => {
  expect(tauri.calls.some((call) => call.command === "get_update")).toBe(true);
  expect(button.hidden).toBe(true);
});

test("an update-available event reveals the icon with the version in its tooltip", () => {
  tauri.emit("update-available", { version: "v0.9.9" });
  expect(button.hidden).toBe(false);
  expect(button.title).toContain("v0.9.9");
  expect(button.getAttribute("aria-label")).toBe(button.title);
});

test("clicking the icon asks the app to run the installer with the closing prompt", async () => {
  button.click();
  await new Promise((resolve) => setTimeout(resolve, 10));
  const call = tauri.calls.filter((entry) => entry.command === "run_update").pop();
  expect(call?.args).toEqual({ prompt: "Pressione Enter para fechar." });
  expect(error.hidden).toBe(true);
});

test("when no terminal can be opened the island says so and shows the command to run by hand", async () => {
  terminalMissing = true;
  button.click();
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(error.hidden).toBe(false);
  expect(error.textContent).toContain("no terminal emulator found on PATH");
  expect(error.textContent).toContain(
    "curl -fsSL https://raw.githubusercontent.com/matheus-cintra/open-island/master/install.sh | sh",
  );
});

test("an ongoing installation prevents repeated clicks and displays progress", async () => {
  terminalMissing = false;
  let finish!: () => void;
  pendingUpdate = new Promise<void>((resolve) => { finish = resolve; });
  const before = tauri.calls.filter((call) => call.command === "run_update").length;
  button.click();
  button.click();
  expect(button.disabled).toBe(true);
  expect(tauri.calls.filter((call) => call.command === "run_update").length).toBe(before + 1);
  tauri.emit("update-progress", "Verificando assinatura…");
  expect(button.title).toBe("Verificando assinatura…");
  finish();
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(button.disabled).toBe(false);
  expect(button.hasAttribute("aria-busy")).toBe(false);
  pendingUpdate = null;
});

test("macOS installation errors do not recommend the Linux installer", async () => {
  document.body.classList.add("platform-macos");
  terminalMissing = true;
  button.click();
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(error.textContent).toContain("Falha ao atualizar");
  expect(error.textContent).not.toContain("curl");
  document.body.classList.remove("platform-macos");
  terminalMissing = false;
});
