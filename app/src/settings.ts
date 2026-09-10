import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { strings } from "./strings";
import { createSprite } from "./sprites";
import {
  config,
  envLocked,
  isJsonObject,
  loadConfig,
  readPath,
  savePending,
  setValue,
} from "./settings-config";
import {
  ICON_CHECK,
  ICON_CHEVRON_DOWN,
  ICON_CHEVRON_UP,
  ICON_INFO,
  ICON_PLAY,
  ICON_PLUS,
  ICON_TRASH,
  ICON_WARNING,
} from "./settings-icons";
import { PANES } from "./settings-panes";
import { showToast } from "./settings-toast";
import {
  ActionName,
  Control,
  DurationUnit,
  IntegrationName,
  IntegrationState,
  IntegrationStatus,
  IslandMetrics,
  JsonObject,
  JsonValue,
  LauncherRule,
  MatchType,
  Pane,
  PaneId,
  Row,
  RuleField,
  SessionLauncher,
  SilenceRule,
  UpdateAvailable,
} from "./settings-types";

export type { Control } from "./settings-types";
export { flushSave, setValue } from "./settings-config";

const SCALE_STEPS = [1, 1.25, 1.5, 1.75, 2, 2.5, 3];
const CONFIRM_MS = 8000;
const MILLISECONDS_PER_UNIT: Record<DurationUnit, number> = {
  ms: 1,
  s: 1000,
  min: 60_000,
};

const copy = strings.settings;

const sidebarEl = document.getElementById("sidebar") as HTMLElement;
const headerEl = document.getElementById("paneHeader") as HTMLElement;
const contentEl = document.getElementById("content") as HTMLElement;
const markerEl = document.createElement("span");
markerEl.className = "sidebar-marker";

let observedLaunchers: string[] = [];
const UNKNOWN_INTEGRATION: IntegrationState = { detected: false, installed: false };
let integrations: IntegrationStatus = {
  autostart: UNKNOWN_INTEGRATION,
  hyprland: UNKNOWN_INTEGRATION,
  claude: UNKNOWN_INTEGRATION,
  codex: UNKNOWN_INTEGRATION,
  opencode: UNKNOWN_INTEGRATION,
};
let themeSounds: string[] = [];
let autoScale: number | null = null;
export let appVersion = "";
export let availableUpdate: string | null = null;
let activePane: PaneId = "general";
let dependants: { element: HTMLElement; path: string; mode: "dim" | "hide" }[] = [];

function icon(markup: string): SVGElement {
  const holder = document.createElement("span");
  holder.innerHTML = markup;
  return holder.firstElementChild as SVGElement;
}

function toUnit(milliseconds: number, unit: DurationUnit): number {
  const scaled = milliseconds / MILLISECONDS_PER_UNIT[unit];
  return Math.round(scaled * 100) / 100;
}

function fromUnit(value: number, unit: DurationUnit): number {
  return Math.round(value * MILLISECONDS_PER_UNIT[unit]);
}

function soundLabel(path: string): string {
  const file = path.slice(path.lastIndexOf("/") + 1);
  return file.replace(/\.oga$/, "");
}

function lockedVariable(row: Row): string | undefined {
  return envLocked[row.path];
}

function markLocked(element: HTMLInputElement | HTMLSelectElement, row: Row): void {
  if (lockedVariable(row) === undefined) return;
  element.disabled = true;
  element.dataset.locked = "true";
}

function dependencyMet(value: JsonValue | undefined): boolean {
  if (typeof value === "number") return Number.isFinite(value) && value !== 0;
  return value === true;
}

function refreshDependencies(): void {
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

function buildSwitch(
  row: Row,
  checked: boolean,
  onChange: (next: boolean) => void,
): HTMLInputElement {
  const input = document.createElement("input");
  input.type = "checkbox";
  input.className = "switch";
  input.checked = checked;
  markLocked(input, row);
  input.addEventListener("change", () => onChange(input.checked));
  return input;
}

function buildDuration(row: Row, control: Extract<Control, { kind: "duration" }>): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.className = "row-control";
  const stored = readPath(config, row.path);
  const input = document.createElement("input");
  input.type = "number";
  input.className = "field";
  const value = toUnit(typeof stored === "number" ? stored : 0, control.unit);
  input.min = String(Math.min(control.min, value));
  input.max = String(Math.max(control.max, value));
  input.step = String(control.step);
  input.value = String(value);
  markLocked(input, row);
  input.addEventListener("change", () => {
    const parsed = Number(input.value);
    if (!Number.isFinite(parsed)) return;
    const clamped = Math.min(Number(input.max), Math.max(Number(input.min), parsed));
    input.value = String(clamped);
    setValue(row.path, fromUnit(clamped, control.unit));
  });
  const unit = document.createElement("span");
  unit.className = "field-unit";
  unit.textContent = copy.units[unitKey(control.unit)];
  wrapper.append(
    input,
    unit,
    buildStepper(input, row, control, (value) =>
      setValue(row.path, fromUnit(value, control.unit)),
    ),
  );
  return wrapper;
}

function buildStepper(
  input: HTMLInputElement,
  row: Row,
  control: { min: number; max: number; step: number },
  apply: (value: number) => void,
): HTMLElement {
  const stepper = document.createElement("div");
  stepper.className = "stepper";
  const start = Number(input.value);
  const low = Number.isFinite(start) ? Math.min(control.min, start) : control.min;
  const high = Number.isFinite(start) ? Math.max(control.max, start) : control.max;
  const step = (direction: 1 | -1): void => {
    const current = Number(input.value);
    if (!Number.isFinite(current)) return;
    const next = Math.min(high, Math.max(low, current + direction * control.step));
    const rounded = Math.round(next * 100) / 100;
    if (rounded === current) return;
    input.value = String(rounded);
    apply(rounded);
  };
  for (const [markup, direction, label] of [
    [ICON_CHEVRON_UP, 1, copy.stepper.increase],
    [ICON_CHEVRON_DOWN, -1, copy.stepper.decrease],
  ] as [string, 1 | -1, string][]) {
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("aria-label", `${label}: ${row.label}`);
    button.append(icon(markup));
    button.disabled = input.disabled;
    if (input.dataset.locked === "true") button.dataset.locked = "true";
    button.addEventListener("click", () => step(direction));
    stepper.append(button);
  }
  return stepper;
}

function unitKey(unit: DurationUnit): "milliseconds" | "seconds" | "minutes" {
  if (unit === "ms") return "milliseconds";
  if (unit === "s") return "seconds";
  return "minutes";
}

function buildSelect(row: Row): HTMLSelectElement {
  const stored = readPath(config, row.path);
  const current = typeof stored === "string" ? stored : null;
  const select = document.createElement("select");
  select.className = "select";
  markLocked(select, row);

  const off = document.createElement("option");
  off.value = "";
  off.textContent = copy.sound.off;
  select.append(off);

  const paths = [...themeSounds];
  if (current !== null && !paths.includes(current)) paths.unshift(current);
  for (const path of paths) {
    const option = document.createElement("option");
    option.value = path;
    option.textContent = soundLabel(path);
    select.append(option);
  }
  select.value = current ?? "";
  select.addEventListener("change", () => {
    setValue(row.path, select.value === "" ? null : select.value);
  });
  return select;
}

function buildScaleSelect(row: Row): HTMLElement {
  const stored = readPath(config, row.path);
  const current = typeof stored === "number" && stored > 0 ? stored : 0;
  const select = document.createElement("select");
  select.className = "select";
  markLocked(select, row);

  const automatic = document.createElement("option");
  automatic.value = "0";
  automatic.textContent = copy.display.uiScaleAuto(
    autoScale === null ? null : copy.display.uiScaleValue(autoScale),
  );
  select.append(automatic);

  const steps =
    current === 0 || SCALE_STEPS.includes(current)
      ? SCALE_STEPS
      : [...SCALE_STEPS, current].sort((left, right) => left - right);
  for (const step of steps) {
    const option = document.createElement("option");
    option.value = String(step);
    option.textContent = copy.display.uiScaleValue(step);
    select.append(option);
  }
  select.value = String(current);
  select.addEventListener("change", () => {
    setValue(row.path, Number(select.value));
  });
  return select;
}

function buildOptions(row: Row, control: Extract<Control, { kind: "options" }>): HTMLElement {
  const stored = readPath(config, row.path);
  const select = document.createElement("select");
  select.className = "select";
  markLocked(select, row);
  for (const [value, label] of control.choices) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = label;
    select.append(option);
  }
  const current = typeof stored === "string" ? stored : control.choices[0][0];
  select.value = control.choices.some(([value]) => value === current)
    ? current
    : control.choices[0][0];
  select.addEventListener("change", () => {
    setValue(row.path, select.value);
  });
  return select;
}

function buildPicker(row: Row, control: Extract<Control, { kind: "picker" }>): HTMLElement {
  const stored = readPath(config, row.path);
  const current = typeof stored === "number" ? stored : control.defaultValue;
  const values = control.values.includes(current)
    ? [...control.values]
    : [...control.values, current].sort((left, right) => left - right);
  const select = document.createElement("select");
  select.className = "select";
  markLocked(select, row);
  for (const value of values) {
    const option = document.createElement("option");
    option.value = String(value);
    const label = control.label ? control.label(value) : `${value}${control.unit}`;
    option.textContent = value === control.defaultValue ? copy.pickerDefault(label) : label;
    select.append(option);
  }
  select.value = String(current);
  select.addEventListener("change", () => {
    setValue(row.path, Number(select.value));
    refreshDependencies();
  });
  return select;
}

function buildPercent(row: Row, control: Extract<Control, { kind: "percent" }>): HTMLElement {
  const wrapper = document.createElement("div");
  wrapper.className = "row-control";
  const stored = readPath(config, row.path);
  const input = document.createElement("input");
  input.type = "number";
  input.className = "field";
  const value = typeof stored === "number" ? stored : control.max;
  input.min = String(Math.min(control.min, value));
  input.max = String(Math.max(control.max, value));
  input.step = String(control.step);
  input.value = String(value);
  markLocked(input, row);
  input.addEventListener("change", () => {
    const parsed = Number(input.value);
    if (!Number.isFinite(parsed)) return;
    const clamped = Math.min(Number(input.max), Math.max(Number(input.min), parsed));
    input.value = String(clamped);
    setValue(row.path, clamped);
  });
  const unit = document.createElement("span");
  unit.className = "field-unit";
  unit.textContent = copy.units.percent;
  wrapper.append(
    input,
    unit,
    buildStepper(input, row, control, (value) => setValue(row.path, value)),
  );
  return wrapper;
}

function buildValue(text: string): HTMLElement {
  const value = document.createElement("span");
  value.className = "row-value";
  value.textContent = text;
  return value;
}

const ACTIONS: Record<ActionName, () => Promise<void>> = {
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

function buildAction(control: Extract<Control, { kind: "action" }>): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = control.tone === "danger" ? "row-button is-danger" : "row-button";
  button.textContent = control.text;
  let armed = false;
  let disarm = 0;
  const reset = (): void => {
    armed = false;
    button.textContent = control.text;
    button.classList.remove("is-armed");
  };
  button.addEventListener("click", () => {
    if (control.confirm !== undefined && !armed) {
      armed = true;
      button.textContent = control.confirm;
      button.classList.add("is-armed");
      clearTimeout(disarm);
      disarm = window.setTimeout(reset, CONFIRM_MS);
      return;
    }
    clearTimeout(disarm);
    reset();
    button.disabled = true;
    void ACTIONS[control.name]()
      .catch((error: unknown) => {
        button.disabled = false;
        showToast(copy.about.actionFailed(String(error)));
      });
  });
  return button;
}

function buildRowAction(
  row: Row,
  control: Extract<Control, { kind: "row-action" }>,
): HTMLElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "row row-danger";
  const glyph = icon(control.icon);
  glyph.classList.add("row-glyph");
  const label = document.createElement("span");
  label.className = "row-label";
  label.textContent = row.label;
  button.append(glyph, label);
  let armed = false;
  let disarm = 0;
  const reset = (): void => {
    armed = false;
    label.textContent = row.label;
    button.classList.remove("is-armed");
  };
  button.addEventListener("click", () => {
    if (!armed) {
      armed = true;
      label.textContent = control.confirm;
      button.classList.add("is-armed");
      clearTimeout(disarm);
      disarm = window.setTimeout(reset, CONFIRM_MS);
      return;
    }
    clearTimeout(disarm);
    reset();
    button.disabled = true;
    void ACTIONS[control.name]().catch((error: unknown) => {
      button.disabled = false;
      showToast(copy.about.actionFailed(String(error)));
    });
  });
  return button;
}

function buildStatusPip(state: IntegrationState): HTMLElement {
  const pip = document.createElement("span");
  pip.className = state.installed ? "pip" : "pip is-off";
  if (state.installed) pip.append(icon(ICON_CHECK));
  const label = document.createElement("span");
  label.textContent = state.installed ? copy.status.active : copy.status.notDetected;
  pip.append(label);
  return pip;
}

function buildNote(text: string, tone: "warning" | "info"): HTMLElement {
  const element = document.createElement("div");
  element.className = "row is-note";
  const hint = document.createElement("span");
  hint.className = tone === "info" ? "row-hint is-info" : "row-hint";
  hint.append(icon(tone === "info" ? ICON_INFO : ICON_WARNING));
  const body = document.createElement("span");
  body.textContent = text;
  hint.append(body);
  element.append(hint);
  return element;
}

const MATCH_LABELS: Record<MatchType, string> = {
  contains: copy.filters.matchContains,
  prefix: copy.filters.matchPrefix,
  equals: copy.filters.matchEquals,
};

function asRule(value: JsonValue): SilenceRule | null {
  if (!isJsonObject(value)) return null;
  const { field, match_type, pattern, name, built_in, enabled } = value;
  if (field !== "cwd" && field !== "prompt") return null;
  if (match_type !== "contains" && match_type !== "prefix" && match_type !== "equals") {
    return null;
  }
  if (typeof pattern !== "string" || pattern === "") return null;
  return {
    field,
    match_type,
    pattern,
    name: typeof name === "string" && name !== "" ? name : pattern,
    built_in: built_in === true,
    enabled: enabled !== false,
  };
}

function asJson(rule: SilenceRule): JsonObject {
  return {
    field: rule.field,
    match_type: rule.match_type,
    pattern: rule.pattern,
    name: rule.name,
    built_in: rule.built_in,
    enabled: rule.enabled,
  };
}

function storedRules(): SilenceRule[] {
  const stored = readPath(config, "filters.rules");
  if (!Array.isArray(stored)) return [];
  const rules: SilenceRule[] = [];
  for (const entry of stored) {
    const rule = asRule(entry);
    if (rule !== null) rules.push(rule);
  }
  return rules;
}

function saveRules(rules: SilenceRule[]): void {
  setValue("filters.rules", rules.map(asJson));
  renderPane();
}

function buildRulesRow(row: Row, field: RuleField): HTMLElement {
  const element = document.createElement("div");
  element.className = "row-rules";

  const rules = storedRules();
  const mine = rules
    .map((rule, index) => ({ rule, index }))
    .filter((entry) => entry.rule.field === field);

  if (mine.length === 0) {
    const empty = document.createElement("p");
    empty.className = "rules-empty";
    empty.textContent = copy.filters.empty;
    element.append(empty);
  }

  for (const { rule, index } of mine) {
    element.append(buildRuleEntry(rule, index));
  }

  element.append(buildRuleEditor(row, field));
  return element;
}

function buildRuleEntry(rule: SilenceRule, index: number): HTMLElement {
  const entry = document.createElement("div");
  entry.className = "rule";

  const copyBlock = document.createElement("label");
  copyBlock.className = "row-copy";
  const title = document.createElement("span");
  title.className = "row-label";
  title.textContent = rule.name;
  copyBlock.append(title);
  if (rule.built_in) {
    const badge = document.createElement("span");
    badge.className = "rule-preset";
    badge.textContent = copy.filters.preset;
    title.append(" ", badge);
  }
  const hint = document.createElement("span");
  hint.className = "row-hint rule-pattern";
  hint.textContent = `${MATCH_LABELS[rule.match_type]} · ${rule.pattern}`;
  copyBlock.append(hint);
  entry.append(copyBlock);

  if (!rule.built_in) {
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "rule-remove";
    remove.title = copy.filters.remove;
    remove.setAttribute("aria-label", `${copy.filters.remove}: ${rule.name}`);
    remove.innerHTML = ICON_TRASH;
    remove.addEventListener("click", () => {
      const next = storedRules();
      next.splice(index, 1);
      saveRules(next);
    });
    entry.append(remove);
  }

  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.className = "switch";
  toggle.checked = rule.enabled;
  toggle.id = `rule-${index}`;
  copyBlock.htmlFor = toggle.id;
  toggle.addEventListener("change", () => {
    const next = storedRules();
    const target = next[index];
    if (target === undefined) return;
    target.enabled = toggle.checked;
    saveRules(next);
  });
  entry.append(toggle);
  return entry;
}

function storedLaunchers(): LauncherRule[] {
  const stored = readPath(config, "filters.launchers");
  if (!Array.isArray(stored)) return [];
  const launchers: LauncherRule[] = [];
  for (const entry of stored) {
    if (entry === null || typeof entry !== "object" || Array.isArray(entry)) continue;
    const appId = entry.app_id;
    if (typeof appId !== "string" || appId === "") continue;
    const name = entry.name;
    launchers.push({
      app_id: appId,
      name: typeof name === "string" && name !== "" ? name : appId,
      enabled: entry.enabled !== false,
    });
  }
  return launchers;
}

function saveLaunchers(launchers: LauncherRule[]): void {
  setValue(
    "filters.launchers",
    launchers.map((launcher) => ({
      app_id: launcher.app_id,
      name: launcher.name,
      enabled: launcher.enabled,
    })),
  );
  renderPane();
}

function buildLaunchersRow(row: Row): HTMLElement {
  const element = document.createElement("div");
  element.className = "row-rules";
  const launchers = storedLaunchers();

  if (launchers.length === 0) {
    const empty = document.createElement("p");
    empty.className = "rules-empty";
    empty.textContent = copy.filters.launchersEmpty;
    element.append(empty);
  }

  launchers.forEach((launcher, index) => {
    element.append(buildLauncherEntry(launcher, index));
  });

  element.append(buildLauncherEditor(row));
  return element;
}

function buildLauncherEntry(launcher: LauncherRule, index: number): HTMLElement {
  const entry = document.createElement("div");
  entry.className = "rule";

  const copyBlock = document.createElement("label");
  copyBlock.className = "row-copy";
  const title = document.createElement("span");
  title.className = "row-label";
  title.textContent = launcher.name;
  copyBlock.append(title);
  const hint = document.createElement("span");
  hint.className = "row-hint rule-pattern";
  hint.textContent = launcher.app_id;
  copyBlock.append(hint);
  entry.append(copyBlock);

  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "rule-remove";
  remove.title = copy.filters.remove;
  remove.setAttribute("aria-label", `${copy.filters.remove}: ${launcher.name}`);
  remove.innerHTML = ICON_TRASH;
  remove.addEventListener("click", () => {
    const next = storedLaunchers();
    next.splice(index, 1);
    saveLaunchers(next);
  });
  entry.append(remove);

  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.className = "switch";
  toggle.checked = launcher.enabled;
  toggle.id = `launcher-${index}`;
  copyBlock.htmlFor = toggle.id;
  toggle.addEventListener("change", () => {
    const next = storedLaunchers();
    const target = next[index];
    if (target === undefined) return;
    target.enabled = toggle.checked;
    saveLaunchers(next);
  });
  entry.append(toggle);
  return entry;
}

function buildLauncherEditor(row: Row): HTMLElement {
  const editor = document.createElement("div");
  editor.className = "rule-editor";

  const input = document.createElement("input");
  input.type = "text";
  input.className = "field rule-input";
  input.placeholder = copy.filters.launcherPlaceholder;
  input.setAttribute("aria-label", row.label);

  const blocked = new Set(storedLaunchers().map((launcher) => launcher.app_id));
  const seen = observedLaunchers.filter((app_id) => !blocked.has(app_id));
  if (seen.length > 0) {
    const list = document.createElement("datalist");
    list.id = "launcher-seen";
    for (const app_id of seen) {
      const option = document.createElement("option");
      option.value = app_id;
      option.label = copy.filters.launcherSeen;
      list.append(option);
    }
    editor.append(list);
    input.setAttribute("list", list.id);
  }

  const add = document.createElement("button");
  add.type = "button";
  add.className = "rule-add";
  add.innerHTML = ICON_PLUS;
  add.append(document.createTextNode(copy.filters.add));
  add.disabled = true;

  const commit = (): void => {
    const text = input.value.trim();
    if (text === "") return;
    const next = storedLaunchers();
    if (next.some((launcher) => launcher.app_id === text)) return;
    next.push({ app_id: text, name: text, enabled: true });
    saveLaunchers(next);
  };

  input.addEventListener("input", () => {
    add.disabled = input.value.trim() === "";
  });
  input.addEventListener("keydown", (keyEvent) => {
    if (keyEvent.key === "Enter") {
      keyEvent.preventDefault();
      commit();
    }
  });
  add.addEventListener("click", commit);

  editor.append(input, add);
  return editor;
}

function buildRuleEditor(row: Row, field: RuleField): HTMLElement {
  const editor = document.createElement("div");
  editor.className = "rule-editor";

  const match = document.createElement("select");
  match.className = "select";
  match.setAttribute(
    "aria-label",
    field === "cwd" ? copy.filters.directory : copy.filters.prompt,
  );
  for (const value of ["contains", "prefix", "equals"] as const) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = MATCH_LABELS[value];
    match.append(option);
  }
  match.value = field === "cwd" ? "contains" : "prefix";

  const pattern = document.createElement("input");
  pattern.type = "text";
  pattern.className = "field rule-input";
  pattern.placeholder =
    field === "cwd" ? copy.filters.patternCwd : copy.filters.patternPrompt;
  pattern.setAttribute("aria-label", row.label);

  const add = document.createElement("button");
  add.type = "button";
  add.className = "rule-add";
  add.innerHTML = ICON_PLUS;
  add.append(document.createTextNode(copy.filters.add));
  add.disabled = true;

  const commit = (): void => {
    const text = pattern.value.trim();
    if (text === "") return;
    const next = storedRules();
    next.push({
      field,
      match_type: match.value as MatchType,
      pattern: text,
      name: text,
      built_in: false,
      enabled: true,
    });
    saveRules(next);
  };

  pattern.addEventListener("input", () => {
    add.disabled = pattern.value.trim() === "";
  });
  pattern.addEventListener("keydown", (keyEvent) => {
    if (keyEvent.key === "Enter") {
      keyEvent.preventDefault();
      commit();
    }
  });
  add.addEventListener("click", commit);

  editor.append(match, pattern, add);
  return editor;
}

function buildTilesRow(
  row: Row,
  choices: readonly (readonly [string, string, string])[],
): HTMLElement {
  const element = document.createElement("div");
  element.className = "row row-tiles";
  const group = document.createElement("div");
  group.className = "tiles";
  group.setAttribute("role", "radiogroup");
  group.setAttribute("aria-label", row.label);
  const stored = readPath(config, row.path);
  const buttons: HTMLButtonElement[] = [];
  for (const [value, label, hint] of choices) {
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "tile-card";
    tile.setAttribute("role", "radio");
    tile.setAttribute("aria-checked", String(stored === value));
    const name = document.createElement("span");
    name.className = "tile-card-name";
    name.textContent = label;
    const note = document.createElement("span");
    note.className = "tile-card-hint";
    note.textContent = hint;
    tile.append(name, note);
    tile.addEventListener("click", () => {
      for (const other of buttons) other.setAttribute("aria-checked", String(other === tile));
      setValue(row.path, value);
    });
    buttons.push(tile);
    group.append(tile);
  }
  element.append(group);
  return element;
}

export function buildRow(row: Row): HTMLElement {
  const control = row.control;
  if (control.kind === "row-action") return buildRowAction(row, control);
  if (control.kind === "volume") return buildSliderRow(row, VOLUME_SLIDER);
  if (control.kind === "size") return buildSliderRow(row, control);
  if (control.kind === "rules") return buildRulesRow(row, control.field);
  if (control.kind === "launchers") return buildLaunchersRow(row);
  if (control.kind === "tiles") return buildTilesRow(row, control.choices);

  const element = document.createElement("div");
  element.className = "row";
  const variable = lockedVariable(row);
  if (variable !== undefined) element.classList.add("is-locked");

  const label = document.createElement("label");
  label.className = "row-copy";
  const title = document.createElement("span");
  title.className = "row-label";
  title.textContent = row.label;
  label.append(title);

  const hint = variable !== undefined ? copy.lockedByEnv(variable) : row.hint;
  if (hint !== undefined) {
    const description = document.createElement("span");
    description.className = "row-hint";
    if (variable !== undefined) description.append(icon(ICON_WARNING));
    const text = document.createElement("span");
    text.textContent = hint;
    description.append(text);
    label.append(description);
  }
  element.append(label);

  if (control.kind === "switch") {
    const stored = readPath(config, row.path);
    const input = buildSwitch(row, stored === true, (next) => {
      setValue(row.path, next);
      refreshDependencies();
    });
    label.htmlFor = input.id || (input.id = `control-${row.path}`);
    element.append(input);
  } else if (control.kind === "integration") {
    const state = integrations[control.name] ?? UNKNOWN_INTEGRATION;
    const input = buildSwitch(row, state.installed, (next) => {
      void setIntegration(control.name, next);
    });
    label.htmlFor = input.id || (input.id = `control-${row.path}`);
    if (!state.detected) {
      input.disabled = true;
      element.classList.add("is-locked");
    }
    if (state.installed || !state.detected) element.append(buildStatusPip(state));
    element.append(input);
  } else if (control.kind === "duration") {
    element.append(buildDuration(row, control));
  } else if (control.kind === "percent") {
    element.append(buildPercent(row, control));
  } else if (control.kind === "value") {
    element.append(buildValue(control.text()));
  } else if (control.kind === "action") {
    element.append(buildAction(control));
  } else if (control.kind === "options") {
    const select = buildOptions(row, control);
    label.htmlFor = select.id || (select.id = `control-${row.path}`);
    element.append(select);
  } else if (control.kind === "picker") {
    const select = buildPicker(row, control);
    label.htmlFor = select.id || (select.id = `control-${row.path}`);
    element.append(select);
  } else if (control.kind === "time") {
    const input = buildTime(row);
    label.htmlFor = input.id;
    element.append(input);
  } else if (control.kind === "sound") {
    const select = buildSelect(row);
    label.htmlFor = select.id || (select.id = `control-${row.path}`);
    const group = document.createElement("div");
    group.className = "row-control";
    group.append(select, buildPreview(row, select));
    element.append(group);
  } else {
    const select = buildScaleSelect(row);
    label.htmlFor = select.id || (select.id = `control-${row.path}`);
    element.append(select);
  }
  return element;
}

function buildTime(row: Row): HTMLInputElement {
  const stored = readPath(config, row.path);
  const minutes = typeof stored === "number" ? stored : 0;
  const input = document.createElement("input");
  input.type = "time";
  input.className = "field field-time";
  const pad = (value: number): string => String(value).padStart(2, "0");
  input.value = `${pad(Math.floor(minutes / 60))}:${pad(minutes % 60)}`;
  markLocked(input, row);
  input.id = `control-${row.path}`;
  input.addEventListener("change", () => {
    const [hours, mins] = input.value.split(":").map(Number);
    if (!Number.isFinite(hours) || !Number.isFinite(mins)) return;
    setValue(row.path, hours * 60 + mins);
  });
  return input;
}

function buildPreview(row: Row, select: HTMLSelectElement): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "preview";
  button.innerHTML = ICON_PLAY;
  const label = copy.sound.preview(row.label);
  button.title = label;
  button.setAttribute("aria-label", label);
  const sync = (): void => {
    button.disabled = select.value === "";
  };
  sync();
  select.addEventListener("change", sync);
  button.addEventListener("click", () => {
    if (select.value === "") return;
    void invoke("play_sound", { path: select.value }).catch((error: unknown) => {
      showToast(copy.sound.previewFailed(String(error)));
    });
  });
  return button;
}

type SliderSpec = { min: number; max: number; step: number; unit: string; scale?: number };

const VOLUME_SLIDER: SliderSpec = { min: 0, max: 1, step: 0.05, unit: "%" };

function sliderValue(spec: SliderSpec, raw: number): string {
  if (spec.unit === "%") return `${Math.round(raw * 100)}%`;
  if (spec.scale === undefined) return `${Math.round(raw)}${spec.unit}`;
  const scaled = (raw / spec.scale).toLocaleString("pt-BR", {
    minimumFractionDigits: 0,
    maximumFractionDigits: 2,
  });
  return `${scaled}${spec.unit}`;
}

function buildSliderRow(row: Row, spec: SliderSpec): HTMLElement {
  const element = document.createElement("div");
  element.className = "row-slider";
  const variable = lockedVariable(row);
  if (variable !== undefined) element.classList.add("is-locked");

  const head = document.createElement("div");
  head.className = "row-slider-head";
  const label = document.createElement("label");
  label.className = "row-label";
  label.textContent = row.label;
  const pill = document.createElement("span");
  pill.className = "value-pill";
  head.append(label, pill);

  const hint = variable !== undefined ? copy.lockedByEnv(variable) : row.hint;
  if (hint !== undefined) {
    const description = document.createElement("span");
    description.className = "row-hint";
    if (variable !== undefined) description.append(icon(ICON_WARNING));
    const text = document.createElement("span");
    text.textContent = hint;
    description.append(text);
    element.append(description);
  }

  const stored = readPath(config, row.path);
  const value = typeof stored === "number" ? stored : spec.min;
  const wrapper = document.createElement("div");
  wrapper.className = "slider-wrap";
  const input = document.createElement("input");
  input.type = "range";
  input.className = "slider";
  input.min = String(Math.min(spec.min, value));
  input.max = String(Math.max(spec.max, value));
  input.step = String(spec.step);
  input.value = String(value);
  markLocked(input, row);
  input.id = `control-${row.path}`;
  label.htmlFor = input.id;

  const ticks = document.createElement("div");
  ticks.className = "slider-ticks";
  ticks.setAttribute("aria-hidden", "true");
  wrapper.append(input, ticks);

  const low = Number(input.min);
  const high = Number(input.max);
  ticks.style.setProperty("--ticks", String(Math.round((high - low) / spec.step)));

  const paint = (): void => {
    const current = Number(input.value);
    pill.textContent = sliderValue(spec, current);
    input.style.setProperty("--fill", `${((current - low) / (high - low)) * 100}%`);
  };
  paint();
  input.addEventListener("input", paint);
  input.addEventListener("change", () => {
    setValue(row.path, Number(input.value));
  });

  element.insertBefore(head, element.firstChild);
  element.append(wrapper);
  return element;
}

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

function renderPane(): void {
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
      if (row.visible?.() === false) continue;
      const built = buildRow(row);
      if (row.visibleWhen !== undefined)
        dependants.push({ element: built, path: row.visibleWhen, mode: "hide" });
      card.append(built);
    }
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

async function setIntegration(name: IntegrationName, enabled: boolean): Promise<void> {
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
