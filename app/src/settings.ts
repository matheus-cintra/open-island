import { linuxCapabilities, supportsRow, type PlatformCapabilities } from "./platform-capabilities";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { strings } from "./strings";
import { createSprite } from "./sprites";
import {
  config,
  loadConfig,
  readPath,
  savePending,
} from "./settings-config";
import { buildNote, buildRow, icon } from "./settings-controls";
import { PANES } from "./settings-panes";
import { showToast } from "./settings-toast";
import {
  ActionName,
  IntegrationName,
  IntegrationState,
  IntegrationStatus,
  IslandMetrics,
  JsonValue,
  Pane,
  PaneId,
  SessionLauncher,
  UpdateAvailable,
} from "./settings-types";

export type { Control } from "./settings-types";
export { flushSave, setValue } from "./settings-config";
export { asJson, asRule, saveLaunchers, saveRules, storedLaunchers, storedRules } from "./settings-filters";
export { buildRow } from "./settings-controls";

const copy = strings.settings;

const sidebarEl = document.getElementById("sidebar") as HTMLElement;
const headerEl = document.getElementById("paneHeader") as HTMLElement;
const contentEl = document.getElementById("content") as HTMLElement;
const markerEl = document.createElement("span");
markerEl.className = "sidebar-marker";

export let observedLaunchers: string[] = [];
export const UNKNOWN_INTEGRATION: IntegrationState = { detected: false, installed: false };
export let integrations: IntegrationStatus = {
  autostart: UNKNOWN_INTEGRATION,
  hyprland: UNKNOWN_INTEGRATION,
  claude: UNKNOWN_INTEGRATION,
  codex: UNKNOWN_INTEGRATION,
  opencode: UNKNOWN_INTEGRATION,
};
export let themeSounds: string[] = [];
export let autoScale: number | null = null;
export let appVersion = "";
export let availableUpdate: string | null = null;
let capabilities = linuxCapabilities;
let shortcutStatus: { shortcut: string; error: string | null } = { shortcut: "", error: null };
let activePane: PaneId = "general";
let dependants: { element: HTMLElement; path: string; mode: "dim" | "hide" }[] = [];

function dependencyMet(value: JsonValue | undefined): boolean {
  if (typeof value === "number") return Number.isFinite(value) && value !== 0;
  return value === true;
}

export function refreshDependencies(): void {
  for (const entry of dependants) {
    const enabled = dependencyMet(readPath(config, entry.path));
    if (entry.mode === "hide") {
      entry.element.hidden = !enabled;
      continue;
    }
    entry.element.classList.toggle("is-muted", !enabled);
    const controls = entry.element.querySelectorAll<
      HTMLInputElement | HTMLSelectElement | HTMLButtonElement
    >("input, select, .stepper button, .preview");
    for (const control of controls) {
      if (control.dataset.locked === "true") continue;
      control.disabled = !enabled;
    }
  }
}

export const ACTIONS: Record<ActionName, () => Promise<void>> = {
  removeAutoConfig: async () => {
    const removed = await invoke<string[]>("remove_auto_configuration");
    showToast(copy.about.removeDone(removed.length));
  },
  quit: async () => {
    await invoke("quit_app");
  },
  checkUpdate: async () => {
    const result = await invoke<{ version: string | null }>("check_update");
    availableUpdate = result.version;
    if (activePane === "about") renderPane();
    showToast(
      result.version === null ? copy.about.updateNone : copy.about.updateFound(result.version),
    );
  },
};

function renderHeader(pane: Pane): void {
  headerEl.replaceChildren();
  const tile = document.createElement("span");
  tile.className = "tile";
  tile.style.setProperty("--tint", pane.tint);
  tile.append(icon(pane.icon));
  const title = document.createElement("h1");
  title.className = "pane-title";
  title.textContent = pane.label;
  headerEl.append(tile, title);
}

export function renderPane(): void {
  const pane = PANES.find((entry) => entry.id === activePane);
  if (pane === undefined) return;
  renderHeader(pane);
  contentEl.replaceChildren();
  dependants = [];

  const body = document.createElement("div");
  body.className = "pane";
  body.id = `pane-${pane.id}`;
  body.setAttribute("role", "tabpanel");
  body.setAttribute("aria-labelledby", `tab-${pane.id}`);

  if (pane.identity !== undefined) {
    const identity = document.createElement("div");
    identity.className = "identity";
    const mark = document.createElement("span");
    mark.className = "identity-mark tile";
    mark.style.setProperty("--tint", pane.tint);
    mark.append(createSprite("open-island"));
    identity.append(mark);
    const name = document.createElement("p");
    name.className = "identity-name";
    name.textContent = pane.identity.name;
    const version = document.createElement("p");
    version.className = "identity-version";
    version.textContent = pane.identity.version();
    identity.append(name, version);
    identity.style.setProperty("--index", "0");
    body.append(identity);
  }

  for (const section of pane.sections) {
    if (
      section.requiresIntegration !== undefined &&
      integrations[section.requiresIntegration]?.installed !== true
    ) {
      continue;
    }
    const wrapper = document.createElement("section");
    wrapper.className = section.title === undefined ? "section is-bare" : "section";
    const card = document.createElement("div");
    card.className = "card";
    for (const row of section.rows) {
      if (row.visible?.() === false || !supportsRow(capabilities, row.path)) continue;
      const effectiveRow = capabilities.os === "macos" && row.path === "integration.autostart"
        ? { ...row, hint: "Inicia a ilha e o daemon ao entrar na sua conta do Mac." }
        : row;
      const built = buildRow(effectiveRow);
      if (row.visibleWhen !== undefined)
        dependants.push({ element: built, path: row.visibleWhen, mode: "hide" });
      card.append(built);
    }
    if (card.childElementCount === 0 && !(section.notes?.length)) continue;
    for (const note of section.notes ?? [])
      card.append(buildNote(note, section.notesTone ?? "warning"));
    if (section.title !== undefined) {
      const heading = document.createElement("h2");
      heading.className = "section-title";
      heading.textContent = section.title;
      wrapper.append(heading);
    }
    if (section.description !== undefined) {
      const description = document.createElement("p");
      description.className = "section-desc";
      description.textContent = section.description;
      wrapper.append(description);
    }
    wrapper.append(card);
    if (section.footer !== undefined) {
      const footer = document.createElement("p");
      footer.className = "section-footer";
      footer.textContent = section.footer;
      wrapper.append(footer);
    }
    wrapper.style.setProperty("--index", String(body.childElementCount));
    if (section.dependsOn !== undefined) {
      dependants.push({ element: wrapper, path: section.dependsOn, mode: "dim" });
    }
    body.append(wrapper);
  }

  if (capabilities.experimental && pane.id === "about") {
    body.prepend(buildNote("macOS experimental — validação em um Mac real pendente. O foco ativa o aplicativo; a janela ou aba exata depende da integração do terminal.", "warning"));
  }
  if (!capabilities.automatic_dnd && (pane.id === "sound" || pane.id === "filters")) {
    body.prepend(buildNote("A leitura automática de Não Perturbe e da tela desligada não está disponível no macOS. Use o modo silencioso ou o horário silencioso.", "warning"));
  }
  if (capabilities.global_shortcut && pane.id === "general") {
    const section = document.createElement("section");
    section.className = "section";
    const heading = document.createElement("h2");
    heading.className = "section-title";
    heading.textContent = "Atalho global";
    const card = document.createElement("div");
    card.className = "card";
    const row = document.createElement("form");
    row.className = "row shortcut-row";
    const copy = document.createElement("div");
    copy.className = "row-copy";
    const label = document.createElement("label");
    label.className = "row-label";
    label.htmlFor = "global-shortcut";
    label.textContent = "Mostrar ou ocultar a ilha";
    const hint = document.createElement("span");
    hint.className = "row-hint";
    hint.textContent = "Deixe vazio para desativar.";
    copy.append(label, hint);
    const controls = document.createElement("div");
    controls.className = "row-control shortcut-controls";
    const input = document.createElement("input");
    input.id = "global-shortcut";
    input.type = "text";
    input.className = "field shortcut-input";
    input.value = shortcutStatus.shortcut;
    input.placeholder = "Command+Shift+I";
    input.setAttribute("aria-label", "Atalho global");
    const button = document.createElement("button");
    button.type = "submit";
    button.className = "row-button";
    button.textContent = "Salvar";
    const status = document.createElement("p");
    status.className = "section-footer shortcut-status";
    status.setAttribute("role", "status");
    status.textContent = shortcutStatus.error ?? "";
    status.hidden = !shortcutStatus.error;
    row.addEventListener("submit", (event) => {
      event.preventDefault();
      button.disabled = true;
      void invoke<typeof shortcutStatus>("set_shortcut", { shortcut: input.value }).then((reply) => {
        shortcutStatus = reply;
        status.textContent = reply.error ?? "Atalho salvo.";
        status.hidden = false;
      }).catch((error: unknown) => { status.textContent = String(error); status.hidden = false; })
        .finally(() => { button.disabled = false; });
    });
    controls.append(input, button);
    row.append(copy, controls);
    card.append(row);
    section.append(heading, card, status);
    body.insertBefore(section, body.children[1] ?? null);
  }
  if (capabilities.manual_update && pane.id === "about") {
    body.append(buildNote("A atualização abre o DMG da sua arquitetura. Substitua o aplicativo manualmente em Aplicativos.", "warning"));
  }
  contentEl.append(body);
  refreshDependencies();
}

function renderSidebar(): void {
  sidebarEl.replaceChildren(markerEl);
  for (const pane of PANES) {
    const item = document.createElement("button");
    item.type = "button";
    item.className = "sidebar-item";
    item.dataset.pane = pane.id;
    item.id = `tab-${pane.id}`;
    item.setAttribute("role", "tab");
    item.setAttribute("aria-controls", `pane-${pane.id}`);
    item.setAttribute("aria-selected", String(pane.id === activePane));
    const tile = document.createElement("span");
    tile.className = "tile";
    tile.style.setProperty("--tint", pane.tint);
    tile.append(icon(pane.icon));
    const label = document.createElement("span");
    label.className = "sidebar-label";
    label.textContent = pane.label;
    item.append(tile, label);
    item.addEventListener("click", () => selectPane(pane.id));
    sidebarEl.append(item);
  }
  moveMarker();
}

function selectPane(id: PaneId): void {
  if (id === activePane) return;
  activePane = id;
  moveMarker();
  renderPane();
  contentEl.scrollTop = 0;
  for (const item of sidebarEl.querySelectorAll<HTMLElement>(".sidebar-item")) {
    item.setAttribute("aria-selected", String(item.dataset.pane === id));
  }
}

function moveMarker(): void {
  const selected = sidebarEl.querySelector<HTMLElement>(`[data-pane="${activePane}"]`);
  if (selected === null) return;
  markerEl.style.setProperty("--marker-top", `${selected.offsetTop}px`);
  markerEl.style.setProperty("--marker-height", `${selected.offsetHeight}px`);
}

export async function setIntegration(name: IntegrationName, enabled: boolean): Promise<void> {
  try {
    integrations = await invoke<IntegrationStatus>("set_integration", { name, enabled });
    rememberAgent(name);
  } catch (error: unknown) {
    showToast(copy.saveFailed(String(error)));
  }
  renderPane();
}

function rememberAgent(name: IntegrationName): void {
  const known = readPath(config, "integrations.known_agents");
  if (!Array.isArray(known) || known.includes(name)) return;
  known.push(name);
}

async function load(): Promise<void> {
  try {
    capabilities = (await invoke<PlatformCapabilities>("platform_capabilities").catch(() => linuxCapabilities)) ?? linuxCapabilities;
    document.documentElement.dataset.platform = capabilities.os;
    if (capabilities.global_shortcut) shortcutStatus = await invoke<typeof shortcutStatus>("get_shortcut");
    const [, status, sounds, metrics, soundDir, screens, version, sessions, update] =
      await Promise.all([
        loadConfig(),
        invoke<IntegrationStatus>("integration_status"),
        invoke<string[]>("sound_theme_files"),
        invoke<IslandMetrics>("island_metrics"),
        invoke<string>("user_sound_dir"),
        invoke<string[]>("list_monitors"),
        invoke<string>("app_version"),
        invoke<unknown>("list_sessions").catch(() => []),
        invoke<UpdateAvailable | null>("get_update").catch(() => null),
      ]);
    integrations = status;
    observedLaunchers = [
      ...new Set(
        (Array.isArray(sessions) ? (sessions as SessionLauncher[]) : [])
          .map((session) => session.launcher)
          .filter((launcher): launcher is string => typeof launcher === "string" && launcher !== ""),
      ),
    ].sort();
    themeSounds = sounds;
    appVersion = version;
    availableUpdate = update?.version ?? null;
    const monitorRow = PANES.find((pane) => pane.id === "display")
      ?.sections.flatMap((section) => section.rows)
      .find((entry) => entry.path === "display.monitor");
    if (monitorRow !== undefined && monitorRow.control.kind === "options") {
      monitorRow.control = {
        kind: "options",
        choices: [
          ["", copy.display.monitorAuto],
          ...screens.map((name) => [name, name] as const),
        ],
      };
    }
    autoScale = metrics.scale;
    const mySounds = PANES.find((pane) => pane.id === "sound")?.sections.find(
      (section) => section.title === copy.sound.mySounds,
    );
    if (mySounds !== undefined && soundDir !== "") {
      mySounds.notes = [copy.sound.mySoundsHint(soundDir)];
    }
  } catch (error: unknown) {
    showToast(copy.loadFailed(String(error)));
  }
  renderSidebar();
  renderPane();
}

void listen("config-changed", () => {
  if (savePending) return;
  void loadConfig().then((changed) => {
    if (changed) renderPane();
  });
});

void listen<UpdateAvailable>("update-available", (event) => {
  availableUpdate = event.payload.version;
  if (activePane === "about") renderPane();
});

void listen("settings-revealed", () => {
  if (savePending) return;
  void load();
});

void load();
