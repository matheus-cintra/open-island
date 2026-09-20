import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";

const focusCopy = strings.settings.focus;

export interface FocusStatus {
  authorization: number;
  silenced: boolean | null;
}
export function focusPermissionSection(status: FocusStatus): HTMLElement {
  const section = document.createElement("section");
  section.className = "section";
  const heading = document.createElement("h2");
  heading.className = "section-title";
  heading.textContent = focusCopy.title;
  const card = document.createElement("div");
  card.className = "card";
  const row = document.createElement("div");
  row.className = "row";
  const copy = document.createElement("div");
  copy.className = "row-copy";
  const label = document.createElement("span");
  label.className = "row-label";
  label.textContent = focusCopy.label;
  const hint = document.createElement("span");
  hint.className = "row-hint";
  hint.setAttribute("role", "status");
  hint.textContent = status.authorization === 3
    ? status.silenced === null
      ? focusCopy.grantedWithoutState
      : focusCopy.granted
    : status.authorization === 2
      ? focusCopy.denied
      : status.authorization === 1
        ? focusCopy.restricted
        : status.authorization !== 0
          ? focusCopy.unknown
          : focusCopy.prompt;
  copy.append(label, hint);
  row.append(copy);
  if (status.authorization === 0) {
    const button = document.createElement("button");
    button.className = "row-button";
    button.textContent = focusCopy.authorize;
    button.addEventListener("click", () => {
      button.disabled = true;
      hint.textContent = focusCopy.requesting;
      void invoke("request_focus_permission").catch((error: unknown) => {
        hint.textContent = String(error);
        button.disabled = false;
      });
    });
    row.append(button);
  }
  card.append(row);
  section.append(heading, card);
  return section;
}
