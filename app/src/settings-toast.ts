const TOAST_MS = 6000;

const TOAST_EXIT_MS = 220;

const toastEl = document.getElementById("toast") as HTMLElement;

let toastTimer = 0;
let toastExitTimer = 0;

export function showToast(message: string): void {
  clearTimeout(toastExitTimer);
  toastEl.textContent = message;
  toastEl.hidden = false;
  requestAnimationFrame(() => toastEl.classList.add("is-open"));
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => {
    toastEl.classList.remove("is-open");
    toastExitTimer = window.setTimeout(() => {
      toastEl.hidden = true;
    }, TOAST_EXIT_MS);
  }, TOAST_MS);
}
