import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import { config, envLocked, readPath, setValue } from "./settings-config";
import { buildLaunchersRow, buildRulesRow } from "./settings-filters";
import {
  ICON_CHECK,
  ICON_CHEVRON_DOWN,
  ICON_CHEVRON_UP,
  ICON_INFO,
  ICON_PLAY,
  ICON_WARNING,
} from "./settings-icons";
import { showToast } from "./settings-toast";
import { Control, DurationUnit, IntegrationState, Row } from "./settings-types";
import {
  ACTIONS,
  UNKNOWN_INTEGRATION,
  autoScale,
  integrations,
  refreshDependencies,
  setIntegration,
  themeSounds,
} from "./settings";

const copy = strings.settings;

const SCALE_STEPS = [1, 1.25, 1.5, 1.75, 2, 2.5, 3];
const CONFIRM_MS = 8000;
const MILLISECONDS_PER_UNIT: Record<DurationUnit, number> = {
  ms: 1,
  s: 1000,
  min: 60_000,
};

export function icon(markup: string): SVGElement {
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

export function buildNote(text: string, tone: "warning" | "info"): HTMLElement {
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
