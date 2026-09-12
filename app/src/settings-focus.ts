import { invoke } from "@tauri-apps/api/core";

export interface FocusStatus {
  authorization: number;
  silenced: boolean | null;
}
export function focusPermissionSection(status: FocusStatus): HTMLElement {
  const section = document.createElement("section");
  section.className = "section";
  const heading = document.createElement("h2");
  heading.className = "section-title";
  heading.textContent = "Permissão de Foco";
  const card = document.createElement("div");
  card.className = "card";
  const row = document.createElement("div");
  row.className = "row";
  const copy = document.createElement("div");
  copy.className = "row-copy";
  const label = document.createElement("span");
  label.className = "row-label";
  label.textContent = "Respeitar o estado de Foco do Mac";
  const hint = document.createElement("span");
  hint.className = "row-hint";
  hint.setAttribute("role", "status");
  hint.textContent = status.authorization === 3
    ? status.silenced === null
      ? "Autorizado, mas o estado não foi compartilhado pelo sistema. Confira Compartilhar Estado de Foco nos Ajustes do Sistema."
      : "Autorizado. Os controles de Não Perturbe abaixo usam o estado compartilhado pelo Mac."
    : status.authorization === 2
      ? "Acesso negado. Autorize Open Island nos ajustes de privacidade de Foco do macOS e reabra esta tela."
      : status.authorization === 1
        ? "O acesso ao Foco está restrito neste Mac. O modo silencioso e os horários continuam disponíveis."
        : status.authorization !== 0
          ? "Não foi possível consultar a permissão de Foco. Reabra os ajustes para tentar novamente."
          : "Autorize o compartilhamento do estado de Foco para usar os controles de Não Perturbe.";
  copy.append(label, hint);
  row.append(copy);
  if (status.authorization === 0) {
    const button = document.createElement("button");
    button.className = "row-button";
    button.textContent = "Autorizar…";
    button.addEventListener("click", () => {
      button.disabled = true;
      hint.textContent = "Aguardando a resposta ao pedido de permissão do macOS…";
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
