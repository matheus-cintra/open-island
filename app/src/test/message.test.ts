import { expect, mock, test } from "bun:test";
import { mountIsland } from "./dom";
import { tauriMock } from "./tauri";

let sendFails: string | null = null;

const tauri = tauriMock(({ command }) => {
  if (command === "get_config") return { config: {} };
  if (command === "island_metrics") return { scale: 1, compact_height: null };
  if (command === "get_message_recovery") return [];
  if (command === "list_sessions") return [];
  if (command === "get_usage") return { providers: [] };
  if (command === "get_update") return null;
  if (command === "send_message_v2") {
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
tauri.state.config({ display: { composer: "always" } });

const identity = { daemon_epoch: "fixture-epoch", session_instance_id: "session-instance" };
const base = {
  action_identity: identity,
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

test("a target the daemon has not verified yet explains itself in Portuguese", () => {
  const li = main.createRow({ ...base, send_blocked: "unverified_target" }, false);
  const hint = li.querySelector<HTMLElement>(".message-hint")!;
  expect(hint.hidden).toBe(false);
  expect(hint.textContent).toBe(
    "A ilha ainda não confirmou o destino desta sessão. Tente de novo em alguns segundos.",
  );
});

test("a blocked code the island does not know never leaks as raw text", () => {
  const li = main.createRow({ ...base, send_blocked: "some_future_code" }, false);
  const hint = li.querySelector<HTMLElement>(".message-hint")!;
  expect(hint.textContent).toBe("Não dá para enviar daqui agora. Confira a sessão no terminal.");
  expect(hint.textContent).not.toContain("some_future_code");
});

test("Enter sends the text, Shift+Enter keeps typing, Escape drops the focus", async () => {
  tauri.state.sessions([{ ...base, send_channel: "kitty" }]);
  if (!main.expanded) tauri.emit("island-toggle", {});
  const li = document.querySelector<HTMLLIElement>("#session-list > li")!;
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  input.focus();
  expect(document.activeElement).toBe(input);

  input.value = "oi tudo bem";
  expect(keydown(input, "Enter", true)).toBe(true);
  expect(calls("send_message_v2").length).toBe(0);

  expect(keydown(input, "Enter")).toBe(false);
  await new Promise((resolve) => setTimeout(resolve, 10));
  const sent = calls("send_message_v2");
  expect(sent.length).toBe(1);
  expect(sent[0]!.args).toEqual({ id: "claude:s1", text: "oi tudo bem", identity });
  expect(input.value).toBe("");

  input.value = "   ";
  keydown(input, "Enter");
  expect(calls("send_message_v2").length).toBe(1);

  keydown(input, "Escape");
  expect(document.activeElement).not.toBe(input);
  li.remove();
  tauri.emit("island-toggle", {});
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
  expect(calls("cancel_message_v2").pop()!.args).toEqual({ id: "claude:s1", messageId: 8, identity });

  main.fillRow(li, { ...base, send_channel: "tmux" }, false);
  expect(li.querySelector(".badge-queue")).toBeNull();
  expect(li.querySelectorAll(".message-queued").length).toBe(0);
});

test("a refused send shows the mapped reason and keeps the text", async () => {
  sendFails = "host_unsupported";
  tauri.state.sessions([{ ...base, send_channel: "kitty" }]);
  const li = document.querySelector<HTMLLIElement>("#session-list > li")!;
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

test("identical queue updates keep the editor, caret and queue nodes", () => {
  if (!main.expanded) tauri.emit("island-toggle", {});
  const session = { ...base, send_channel: "tmux", queued_messages: [{ id: 70, text: "preservar", queued_at_ms: 1 }] };
  const li = main.createRow(session, false);
  document.getElementById("session-list")!.append(li);
  const input = li.querySelector<HTMLTextAreaElement>(".message-input")!;
  const queued = li.querySelector(".message-queued")!;
  input.value = "draft";
  input.focus();
  input.setSelectionRange(1, 3);
  for (let index = 0; index < 100; index += 1) main.fillRow(li, session, false);
  expect(li.querySelector(".message-input")).toBe(input);
  expect(li.querySelector(".message-queued")).toBe(queued);
  expect(input.value).toBe("draft");
  expect(input.selectionStart).toBe(1);
  expect(input.selectionEnd).toBe(3);
  expect(document.activeElement).toBe(input);
  li.remove();
});

function boxOf(li: HTMLLIElement): HTMLElement {
  return li.querySelector<HTMLElement>(".row-message")!;
}
function toggleOf(li: HTMLLIElement): HTMLButtonElement {
  return li.querySelector<HTMLButtonElement>(".row-message-toggle")!;
}
function listRows(): HTMLLIElement[] {
  return [...document.querySelectorAll<HTMLLIElement>("#session-list > li")];
}

test("on demand, the composer stays hidden until its toggle opens it, and only one stays open", () => {
  tauri.state.config({ display: { composer: "on_demand" } });
  if (!main.expanded) tauri.emit("island-toggle", {});
  tauri.state.sessions([
    { ...base, id: "claude:c1", send_channel: "tmux" },
    { ...base, id: "claude:c2", title: "outra", send_channel: "tmux" },
  ]);
  const [first, second] = listRows();
  expect(boxOf(first).hidden).toBe(true);
  expect(toggleOf(first).hidden).toBe(false);
  expect(toggleOf(first).getAttribute("aria-expanded")).toBe("false");
  toggleOf(first).click();
  expect(boxOf(first).hidden).toBe(false);
  expect(toggleOf(first).getAttribute("aria-expanded")).toBe("true");
  expect(document.activeElement).toBe(first.querySelector(".message-input"));
  toggleOf(second).click();
  expect(boxOf(first).hidden).toBe(true);
  expect(boxOf(second).hidden).toBe(false);
  toggleOf(second).click();
  expect(boxOf(second).hidden).toBe(true);
});

function publishDraftScenario(): void {
  tauri.state.sessions([
    { ...base, id: "claude:d1", send_channel: "tmux" },
    { ...base, id: "claude:d2", title: "fila", send_channel: "tmux" },
  ]);
  const cache = tauri.state.read();
  cache.snapshot!.snapshot.message_deliveries = [{
    message_id: 3, session_id: "claude:d2", text: "na fila", queued_at_ms: 1, state: "queued",
    identity: { daemon_epoch: "fixture-epoch", session_instance_id: "session-instance" },
  }];
  tauri.emit("daemon-ui-state", cache);
}

test("a draft or a queued message keeps an on-demand composer visible through re-renders", () => {
  tauri.state.config({ display: { composer: "on_demand" } });
  publishDraftScenario();
  const [drafted, queued] = listRows();
  expect(boxOf(queued).hidden).toBe(false);
  toggleOf(drafted).click();
  const input = drafted.querySelector<HTMLTextAreaElement>(".message-input")!;
  input.value = "rascunho";
  input.dispatchEvent(new window.Event("input"));
  toggleOf(queued).click();
  expect(boxOf(drafted).hidden).toBe(false);
  publishDraftScenario();
  expect(boxOf(listRows()[0]).hidden).toBe(false);
  expect(boxOf(listRows()[1]).hidden).toBe(false);
});

test("Escape closes an empty on-demand composer and keeps one that holds text", () => {
  tauri.state.config({ display: { composer: "on_demand" } });
  tauri.state.sessions([{ ...base, id: "claude:e1", send_channel: "tmux" }]);
  const [row] = listRows();
  toggleOf(row).click();
  const input = row.querySelector<HTMLTextAreaElement>(".message-input")!;
  keydown(input, "Escape");
  expect(boxOf(row).hidden).toBe(true);
  expect(toggleOf(row).getAttribute("aria-expanded")).toBe("false");
  toggleOf(row).click();
  input.value = "ainda escrevendo";
  keydown(input, "Escape");
  expect(boxOf(row).hidden).toBe(false);
});

test("always mode shows every composer and hides the toggles", () => {
  tauri.state.config({ display: { composer: "always" } });
  tauri.state.sessions([
    { ...base, id: "claude:a1", send_channel: "tmux" },
    { ...base, id: "claude:a2", title: "outra", send_channel: "tmux" },
  ]);
  for (const li of listRows()) {
    expect(boxOf(li).hidden).toBe(false);
    expect(toggleOf(li).hidden).toBe(true);
  }
});
