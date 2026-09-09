import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

let available: string[] = ["claude", "opencode"];
let terminalMissing = false;

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  if (command === "get_update") return null;
  if (command === "agents_available") return available;
  if (command === "open_session" && terminalMissing) {
    throw new Error("no terminal emulator found on PATH");
  }
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
await import("../main");
await new Promise((resolve) => setTimeout(resolve, 50));

const island = document.getElementById("island")!;
const button = document.getElementById("header-new-session") as HTMLButtonElement;
const card = document.getElementById("new-session-card")!;
const close = document.getElementById("new-session-close") as HTMLButtonElement;
const error = document.getElementById("jump-error")!;

function tiles(): HTMLButtonElement[] {
  return Array.from(card.querySelectorAll<HTMLButtonElement>(".new-session-agent"));
}

function tile(agent: string): HTMLButtonElement {
  return tiles().find((entry) => entry.dataset.agent === agent)!;
}

function calls(command: string) {
  return tauri.calls.filter((entry) => entry.command === command);
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 20));
}

test("the card starts hidden and the header button carries its label", () => {
  expect(card.hidden).toBe(true);
  expect(button.title).toBe("Abrir uma sessão");
  expect(button.getAttribute("aria-label")).toBe("Abrir uma sessão");
});

test("the header button shows three tiles and disables the agent missing from PATH", async () => {
  button.click();
  await settle();
  expect(card.hidden).toBe(false);
  expect(tiles().map((entry) => entry.dataset.agent)).toEqual(["claude", "codex", "opencode"]);
  expect(tiles().map((entry) => entry.textContent)).toEqual(["Claude", "Codex", "OpenCode"]);
  expect(tile("claude").disabled).toBe(false);
  expect(tile("codex").disabled).toBe(true);
  expect(tile("codex").title).toBe("não encontrado no PATH");
  expect(tile("opencode").disabled).toBe(false);
  expect(tile("claude").style.getPropertyValue("--agent-bg")).toBe("var(--agent-claude-bg)");
  expect(tile("claude").style.getPropertyValue("--agent-fg")).toBe("var(--agent-claude-fg)");
});

test("clicking a tile hides the card and asks for the folder with the PT-BR labels", async () => {
  tile("claude").click();
  await settle();
  expect(card.hidden).toBe(true);
  expect(calls("pick_session_folder").pop()?.args).toEqual({
    agent: "claude",
    title: "Escolha a pasta para o Claude",
    accept: "Abrir aqui",
    cancel: "Cancelar",
  });
});

test("a chosen folder opens the session there", async () => {
  tauri.emit("session-folder", { agent: "claude", path: "/home/x/proj" });
  await settle();
  expect(calls("open_session").pop()?.args).toEqual({ agent: "claude", folder: "/home/x/proj" });
});

test("a cancelled dialog opens nothing", async () => {
  const before = calls("open_session").length;
  tauri.emit("session-folder", { agent: "claude", path: null });
  await settle();
  expect(calls("open_session").length).toBe(before);
  expect(error.hidden).toBe(true);
});

test("a failure to open the session lands in the error line with its reason", async () => {
  terminalMissing = true;
  tauri.emit("session-folder", { agent: "claude", path: "/home/x/proj" });
  await settle();
  expect(error.hidden).toBe(false);
  expect(error.textContent).toContain("Não deu para abrir a sessão");
  expect(error.textContent).toContain("no terminal emulator found on PATH");
  terminalMissing = false;
});

test("the close button hides the card", async () => {
  button.click();
  await settle();
  expect(card.hidden).toBe(false);
  expect(close.textContent).toBe("Fechar");
  close.click();
  await settle();
  expect(card.hidden).toBe(true);
});

test("the hotkey does not collapse the island while the card is open", async () => {
  button.click();
  await settle();
  expect(island.classList.contains("expanded")).toBe(true);
  tauri.emit("island-toggle", {});
  await settle();
  expect(island.classList.contains("expanded")).toBe(true);
  close.click();
  await settle();
  tauri.emit("island-toggle", {});
  await settle();
  expect(island.classList.contains("expanded")).toBe(false);
});

test("with no agent on PATH every tile is disabled", async () => {
  available = [];
  button.click();
  await settle();
  expect(tiles().every((entry) => entry.disabled)).toBe(true);
  expect(tiles().every((entry) => entry.title === "não encontrado no PATH")).toBe(true);
  close.click();
});
