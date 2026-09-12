import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

let sendFails: string | null = null;

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  if (command === "get_update") return null;
  if (command === "send_message") {
    if (sendFails !== null) throw new Error(sendFails);
    return { message_id: 1, delivered: false };
  }
  return {};
});

mock.module("@tauri-apps/api/core", () => ({ invoke: tauri.invoke }));
mock.module("@tauri-apps/api/event", () => ({ listen: tauri.listen }));

mountIsland({ reducedMotion: true });
const main = await import("../main");
await new Promise((resolve) => setTimeout(resolve, 50));

const base = {
  id: "claude:s1",
  agent: "claude",
  cwd: "/home/user/dev/open-island",
  title: "open-island",
  pid: 4242,
  terminal: "kitty" as const,
  attention: "working" as const,
};

function keydown(target: HTMLElement, key: string, shiftKey = false): boolean {
  return target.dispatchEvent(new window.KeyboardEvent("keydown", { key, shiftKey, cancelable: true }));
}

function calls(command: string) {
  return tauri.calls.filter((call) => call.command === command);
}

test("a session with a channel gets an enabled field and no hint", () => {
  const li = main.createRow({ ...base, send_channel: "tmux" }, false);
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  expect(input.disabled).toBe(false);
  expect(input.placeholder).toBe(
    "Mensagem para o agente. Enter envia, Shift+Enter quebra a linha.",
  );
  expect(li.querySelector<HTMLElement>(".message-hint")!.hidden).toBe(true);
});

test("a blocked session shows the field disabled with the reason in Portuguese", () => {
  const li = main.createRow({ ...base, send_blocked: "kitty_remote_control_off" }, false);
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  expect(input.disabled).toBe(true);
  const hint = li.querySelector<HTMLElement>(".message-hint")!;
  expect(hint.hidden).toBe(false);
  expect(hint.textContent).toBe(
    "Ligue allow_remote_control e listen_on no kitty para escrever daqui.",
  );
  expect(input.title).toBe(hint.textContent ?? "");
});

test("Enter sends the text, Shift+Enter keeps typing, Escape drops the focus", async () => {
  const li = main.createRow({ ...base, send_channel: "kitty" }, false);
  document.getElementById("session-list")!.append(li);
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  input.focus();
  expect(document.activeElement).toBe(input);

  input.value = "oi tudo bem";
  expect(keydown(input, "Enter", true)).toBe(true);
  expect(calls("send_message").length).toBe(0);

  expect(keydown(input, "Enter")).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 10));
  const sent = calls("send_message");
  expect(sent.length).toBe(1);
  expect(sent[0]!.args).toEqual({ id: "claude:s1", text: "oi tudo bem" });
  expect(input.value).toBe("");

  input.value = "   ";
  keydown(input, "Enter");
  expect(calls("send_message").length).toBe(1);

  keydown(input, "Escape");
  expect(document.activeElement).not.toBe(input);
  li.remove();
});

test("queued messages show a badge with the count and a cancel button each", async () => {
  const li = main.createRow(
    {
      ...base,
      send_channel: "tmux",
      queued_messages: [
        { id: 7, text: "primeira", queued_at_ms: 1 },
        { id: 8, text: "segunda", queued_at_ms: 2 },
      ],
    },
    false,
  );
  const badge = li.querySelector<HTMLElement>(".badge-queue")!;
  expect(badge.textContent).toBe("2");
  expect(badge.title).toBe("2 mensagens na fila");
  const items = li.querySelectorAll<HTMLElement>(".message-queued");
  expect(items.length).toBe(2);
  expect(items[1]!.querySelector(".message-queued-text")!.textContent).toBe("segunda");
  items[1]!.querySelector<HTMLButtonElement>(".message-cancel")!.click();
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(calls("cancel_message").pop()!.args).toEqual({ id: "claude:s1", messageId: 8 });

  main.fillRow(li, { ...base, send_channel: "tmux" }, false);
  expect(li.querySelector(".badge-queue")).toBeNull();
  expect(li.querySelectorAll(".message-queued").length).toBe(0);
});

test("a refused send shows the mapped reason and keeps the text", async () => {
  sendFails = "host_unsupported";
  const li = main.createRow({ ...base, send_channel: "kitty" }, false);
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  input.value = "vai falhar";
  keydown(input, "Enter");
  await new Promise((resolve) => setTimeout(resolve, 10));
  const error = document.getElementById("jump-error")!;
  expect(error.hidden).toBe(false);
  expect(error.textContent).toBe(
    "Não deu para mandar a mensagem: Este terminal não tem como receber texto pela ilha.",
  );
  expect(input.value).toBe("vai falhar");
  sendFails = null;
});

test("expanding the island asks for keyboard focus on demand and collapsing gives it back", async () => {
  tauri.emit("island-toggle", {});
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(calls("island_keyboard").pop()!.args).toEqual({ active: true });
  tauri.emit("island-toggle", {});
  await new Promise((resolve) => setTimeout(resolve, 10));
  expect(calls("island_keyboard").pop()!.args).toEqual({ active: false });
});

test("an unsupported host hides the composer and restores it when a channel becomes available", () => {
  const li = main.createRow({ ...base, send_blocked: "host_unsupported" }, false);
  const box = li.querySelector<HTMLElement>(".row-message")!;
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  expect(box.hidden).toBe(true);
  expect(input.hidden).toBe(true);
  main.fillRow(li, { ...base, send_channel: "tmux" }, false);
  expect(box.hidden).toBe(false);
  expect(input.hidden).toBe(false);
  expect(input.disabled).toBe(false);
});

test("losing host support keeps pending messages visible for cancellation", () => {
  const li = main.createRow({
    ...base,
    send_blocked: "host_unsupported",
    queued_messages: [{ id: 9, text: "pendente", queued_at_ms: 1 }],
  }, false);
  expect(li.querySelector<HTMLElement>(".row-message")!.hidden).toBe(false);
  expect(li.querySelector<HTMLElement>(".message-input")!.hidden).toBe(true);
  expect(li.querySelector<HTMLElement>(".message-hint")!.hidden).toBe(true);
  expect(li.querySelector(".message-cancel")).not.toBeNull();
});
