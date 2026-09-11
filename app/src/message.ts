import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import { reasonOf } from "./json";
import { showError, syncExpandedSize } from "./main";
import { Session } from "./types";

export function createMessageBox(sessionId: string): HTMLElement {
  const box = document.createElement("div");
  box.className = "row-message";
  const input = document.createElement("textarea");
  input.className = "message-input";
  input.rows = 1;
  input.placeholder = strings.session.messagePlaceholder;
  input.setAttribute("aria-label", strings.session.messageOpen);
  const hint = document.createElement("span");
  hint.className = "message-hint";
  hint.hidden = true;
  const queue = document.createElement("ul");
  queue.className = "message-queue";
  box.append(input, hint, queue);
  input.addEventListener("input", () => {
    input.style.height = "auto";
    input.style.height = `${input.scrollHeight}px`;
    syncExpandedSize();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      input.blur();
      return;
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    const text = input.value;
    if (text.trim() === "") return;
    input.value = "";
    input.style.removeProperty("height");
    void invoke<{ delivered: boolean }>("send_message", { id: sessionId, text }).catch(
      (error: unknown) => {
        input.value = text;
        showError(strings.session.messageFailed(strings.session.messageBlocked(reasonOf(error))));
      },
    );
  });
  return box;
}

export function fillMessageBox(li: HTMLLIElement, session: Session): void {
  const box = li.querySelector<HTMLElement>(".row-message")!;
  const input = box.querySelector<HTMLTextAreaElement>(".message-input")!;
  const hint = box.querySelector<HTMLElement>(".message-hint")!;
  const blocked = session.send_blocked;
  input.disabled = blocked !== undefined;
  const reason = blocked === undefined ? "" : strings.session.messageBlocked(blocked);
  hint.textContent = reason;
  const unsupported = blocked === "host_unsupported";
  input.hidden = unsupported;
  hint.hidden = blocked === undefined || unsupported;
  input.title = reason;

  const queued = session.queued_messages ?? [];
  // Keep queued messages cancellable if a session loses its supported host.
  box.hidden = unsupported && queued.length === 0;
  const badges = li.querySelector<HTMLElement>(".row-badges")!;
  let badge = badges.querySelector<HTMLElement>(".badge-queue");
  if (queued.length === 0) {
    badge?.remove();
  } else {
    if (badge === null) {
      badge = document.createElement("span");
      badge.className = "row-badge badge-queue";
      badges.append(badge);
    }
    badge.textContent = String(queued.length);
    badge.title = strings.session.messageQueued(queued.length);
  }

  const list = box.querySelector<HTMLElement>(".message-queue")!;
  list.replaceChildren();
  for (const message of queued) {
    const item = document.createElement("li");
    item.className = "message-queued";
    item.dataset.messageId = String(message.id);
    const text = document.createElement("span");
    text.className = "message-queued-text";
    text.textContent = message.text;
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "message-cancel";
    cancel.textContent = "✕";
    cancel.title = strings.session.messageCancel;
    cancel.setAttribute("aria-label", strings.session.messageCancel);
    cancel.addEventListener("click", () => {
      void invoke("cancel_message", { id: session.id, messageId: message.id }).catch(
        (error: unknown) => {
          showError(strings.session.messageFailed(reasonOf(error)));
        },
      );
    });
    item.append(text, cancel);
    list.append(item);
  }
  list.hidden = queued.length === 0;
}
