/** macOS panels only take keyboard focus for an explicitly selected input. */
export function bindIslandKeyboard(
  root: HTMLElement,
  enabled: () => boolean,
  send: (active: boolean) => Promise<unknown>,
): { activate: () => void; release: () => void } {
  let active = false;
  let pending = Promise.resolve();
  const set = (next: boolean) => {
    if (active === next) return;
    active = next;
    // Preserve the order of quick click/blur/collapse transitions over IPC.
    pending = pending.then(() => send(next)).then(() => {}, () => {});
  };
  const editable = (target: EventTarget | null): target is HTMLElement =>
    target instanceof HTMLElement && root.contains(target) &&
    target.matches('textarea:not(:disabled), input:not(:disabled):not([type="hidden"]), select:not(:disabled), [contenteditable="true"]');
  const release = () => {
    if (editable(document.activeElement)) document.activeElement.blur();
    set(false);
  };
  root.addEventListener("pointerdown", (event) => {
    if (!enabled()) return;
    if (editable(event.target)) set(true);
    else release();
  });
  root.addEventListener("focusin", (event) => {
    if (enabled() && editable(event.target)) set(true);
  });
  root.addEventListener("focusout", () => {
    queueMicrotask(() => {
      if (!editable(document.activeElement)) set(false);
    });
  });
  window.addEventListener("blur", () => { if (active) release(); });
  // Removing a question/card need not emit focusout in WebKit.
  new window.MutationObserver(() => {
    if (active && !editable(document.activeElement)) set(false);
  }).observe(root, { childList: true, subtree: true });
  return { activate: () => set(true), release };
}
