import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import { reasonOf } from "./json";
import type { ActionIdentity } from "./daemon-state";
import { MessageComposer } from "./message-controller";
import { attachVoice, updateVoiceBox } from "./voice";
import { DeliveryView } from "./message-deliveries";

interface MessageActions {
  resize: () => void;
  error: (message: string) => void;
  canUseTarget(id: string, identity: ActionIdentity, writing?: boolean): boolean;
}
const targetFor = new WeakMap<HTMLElement, { identity?: ActionIdentity; blocked: boolean }>();
const actionsFor = new WeakMap<HTMLElement, MessageActions>();
const deliveriesFor = new WeakMap<HTMLElement, DeliveryView>();
import { Session } from "./types";

function setHidden(element: HTMLElement, hidden: boolean): void {
  if (element.hidden !== hidden) element.hidden = hidden;
}

export function createMessageBox(sessionId: string, actions: MessageActions): HTMLElement {
  const box = document.createElement("div");
  box.className = "row-message";
  actionsFor.set(box, actions);
  const field = document.createElement("div");
  field.className = "message-field";
  const input = document.createElement("textarea");
  input.className = "message-input";
  input.rows = 1;
  input.placeholder = strings.session.messagePlaceholder;
  input.setAttribute("aria-label", strings.session.messageOpen);
  field.append(input);
  const hint = document.createElement("span");
  hint.className = "message-hint";
  hint.hidden = true;
  const queue = document.createElement("ul");
  queue.className = "message-queue";
  const recovery = document.createElement("ul");
  recovery.className = "message-queue message-recovery";
  recovery.hidden = true;
  const deliveries = document.createElement("ul");
  deliveriesFor.set(box, new DeliveryView(deliveries, {
    cancel: (record) => {
      const current = targetFor.get(box);
      if (!current?.identity || !actions.canUseTarget(sessionId, current.identity) || current.identity.daemon_epoch !== record.identity?.daemon_epoch || current.identity.session_instance_id !== record.identity?.session_instance_id || box.closest(".is-leaving")) return Promise.reject(new Error("stale_session"));
      return invoke("cancel_message_v2", { id: record.session_id, messageId: record.message_id, identity: record.identity });
    },
    copy: (text) => navigator.clipboard.writeText(text), error: actions.error, changed: actions.resize,
  }));
  box.append(field, hint, queue, deliveries, recovery);
  const composer = new MessageComposer(
    (text: string): Promise<unknown> => {
      const target = targetFor.get(box);
      if (!target?.identity || target.blocked || box.closest(".is-leaving") || !box.isConnected || !actions.canUseTarget(sessionId, target.identity, true)) return Promise.reject(new Error("daemon_unavailable"));
      return invoke("send_message_v2", { id: sessionId, text, identity: { ...target.identity } });
    },
    (state): void => {
      if (input.value !== state.text) input.value = state.text;
      input.setAttribute("aria-busy", String(state.pending));
      recovery.replaceChildren();
      for (const draft of state.recovered) {
        const item = document.createElement("li");
        item.className = "message-queued";
        const text = document.createElement("span");
        text.className = "message-queued-text";
        text.textContent = draft.text;
        const copy = document.createElement("button");
        copy.type = "button";
        copy.textContent = strings.session.messageCopy;
        copy.addEventListener("click", () => {
          void navigator.clipboard.writeText(draft.text).catch(() => actions.error(strings.session.messageCopyFailed));
        });
        const discard = document.createElement("button");
        discard.type = "button";
        discard.textContent = strings.session.messageDiscard;
        discard.addEventListener("click", () => composer.discard(draft.id));
        item.append(text, copy, discard);
        recovery.append(item);
      }
      recovery.hidden = state.recovered.length === 0;
      if (!recovery.hidden) box.hidden = false;
      if (state.text === "") input.style.removeProperty("height");
      else { input.style.height = "auto"; input.style.height = `${input.scrollHeight}px`; }
      actions.resize();
    },
    (error: unknown): void => {
      const reason = reasonOf(error);
      actions.error(reason.includes("daemon_response_timeout") || reason.includes("daemon_unavailable")
        ? strings.session.messageUnconfirmed
        : strings.session.messageFailed(strings.session.messageReason(reason)));
    },
  );
  input.addEventListener("input", () => {
    composer.edit(input.value);
    input.style.height = "auto";
    input.style.height = `${input.scrollHeight}px`;
    actions.resize();
  });
  attachVoice(box, composer, {
    target: () => {
      const target = targetFor.get(box);
      return target?.identity && !target.blocked && !box.closest(".is-leaving") && actions.canUseTarget(sessionId, target.identity, true)
        ? { session_id: sessionId, ...target.identity } : null;
    },
    label: () => box.dataset.voiceLabel ?? sessionId,
  }, actions);
  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      input.blur();
      return;
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    composer.edit(input.value);
    void composer.submit();
  });
  return box;
}

export function fillMessageBox(li: HTMLLIElement, session: Session): void {
  const box = li.querySelector<HTMLElement>(".row-message")!;
  const input = box.querySelector<HTMLTextAreaElement>(".message-input")!;
  const hint = box.querySelector<HTMLElement>(".message-hint")!;
  const blocked = session.send_blocked ?? (session.action_identity ? undefined : "daemon_unavailable");
  targetFor.set(box, { identity: session.action_identity, blocked: blocked !== undefined });
  const voiceLabel = session.name ?? session.title;
  if (box.dataset.voiceLabel !== voiceLabel) box.dataset.voiceLabel = voiceLabel;
  updateVoiceBox(box);
  const disabled = blocked !== undefined;
  if (input.disabled !== disabled) input.disabled = disabled;
  const reason = blocked === undefined ? "" : strings.session.messageBlocked(blocked);
  if (hint.textContent !== reason) hint.textContent = reason;
  const unsupported = blocked === "host_unsupported";
  setHidden(input, unsupported);
  setHidden(hint, blocked === undefined || unsupported);
  if (input.title !== reason) input.title = reason;

  const queued = session.queued_messages ?? [];
  // Keep queued messages cancellable if a session loses its supported host.
  const boxHidden = unsupported && queued.length === 0 && !session.message_deliveries?.length && (box.querySelector<HTMLElement>(".message-recovery")?.hidden ?? true);
  setHidden(box, boxHidden);
  const tail = li.querySelector<HTMLElement>(".row-tail")!;
  let badge = tail.querySelector<HTMLElement>(".badge-queue");
  if (queued.length === 0) {
    badge?.remove();
  } else {
    if (badge === null) {
      badge = document.createElement("span");
      badge.className = "row-queue pixel badge-queue";
      tail.insertBefore(badge, tail.querySelector(".row-elapsed"));
    }
    if (badge.textContent !== String(queued.length)) badge.textContent = String(queued.length);
    const title = strings.session.messageQueued(queued.length);
    if (badge.title !== title) badge.title = title;
  }

  const list = box.querySelector<HTMLElement>(".message-queue")!;
  if (session.message_deliveries !== undefined) {
    if (list.children.length) list.replaceChildren();
    setHidden(list, true);
    deliveriesFor.get(box)?.update(session.message_deliveries, !!session.action_identity && blocked !== "daemon_unavailable" && blocked !== "daemon_incompatible");
    return;
  }
  deliveriesFor.get(box)?.update([], false);
  const identityKey = JSON.stringify(session.action_identity) ?? "";
  const keep = new Set(queued.map((message) => String(message.id)));
  for (const child of Array.from(list.children)) {
    if (!(child instanceof HTMLElement) || (!keep.has(child.dataset.messageId ?? "") || child.dataset.identity !== identityKey)) child.remove();
  }
  for (const [index, message] of queued.entries()) {
    const existing = Array.from(list.children).find((child) => child instanceof HTMLElement && child.dataset.messageId === String(message.id));
    if (existing instanceof HTMLElement) {
      const label = existing.querySelector(".message-queued-text");
      if (label !== null && label.textContent !== message.text) label.textContent = message.text;
      if (list.children[index] !== existing) list.insertBefore(existing, list.children[index] ?? null);
      continue;
    }
    const item = document.createElement("li");
    item.className = "message-queued";
    item.dataset.messageId = String(message.id);
    item.dataset.identity = identityKey;
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
      void (session.action_identity ? invoke("cancel_message_v2", { id: session.id, messageId: message.id, identity: { ...session.action_identity } }) : Promise.reject(new Error("daemon_unavailable"))).catch(
        (error: unknown) => {
          actionsFor.get(box)?.error(strings.session.messageFailed(reasonOf(error)));
        },
      );
    });
    item.append(text, cancel);
    list.insertBefore(item, list.children[index] ?? null);
  }
  setHidden(list, queued.length === 0);
}

export function disposeMessageBox(li: HTMLElement): void {
  const box = li.querySelector<HTMLElement>(".row-message");
  if (box) deliveriesFor.get(box)?.dispose();
}
