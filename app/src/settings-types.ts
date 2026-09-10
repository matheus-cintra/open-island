export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export type PaneId =
  | "general"
  | "integrations"
  | "display"
  | "sound"
  | "usage"
  | "filters"
  | "about";
export type ActionName = "removeAutoConfig" | "quit" | "checkUpdate";
export type DurationUnit = "ms" | "s" | "min";
export type RuleField = "cwd" | "prompt";
export type MatchType = "contains" | "prefix" | "equals";

export interface SessionLauncher {
  launcher?: string | null;
}

export interface LauncherRule {
  app_id: string;
  name: string;
  enabled: boolean;
}

export interface SilenceRule {
  field: RuleField;
  match_type: MatchType;
  pattern: string;
  name: string;
  built_in: boolean;
  enabled: boolean;
}

export type IntegrationName = "autostart" | "hyprland" | "claude" | "codex" | "opencode";

export interface ConfigPayload {
  config: JsonObject;
  env_locked: Record<string, string>;
}

export type IntegrationState = { detected: boolean; installed: boolean };
export type IntegrationStatus = Record<IntegrationName, IntegrationState>;

export interface IslandMetrics {
  scale: number;
  compact_height: number | null;
}

export interface UpdateAvailable {
  version: string;
}

export type Control =
  | { kind: "switch" }
  | { kind: "duration"; unit: DurationUnit; min: number; max: number; step: number }
  | { kind: "volume" }
  | { kind: "size"; min: number; max: number; step: number; unit: string; scale?: number }
  | {
      kind: "picker";
      values: readonly number[];
      unit: string;
      defaultValue: number;
      label?: (value: number) => string;
    }
  | { kind: "time" }
  | { kind: "sound" }
  | { kind: "scale" }
  | { kind: "options"; choices: readonly (readonly [string, string])[] }
  | { kind: "tiles"; choices: readonly (readonly [string, string, string])[] }
  | { kind: "percent"; min: number; max: number; step: number }
  | { kind: "integration"; name: IntegrationName }
  | { kind: "rules"; field: RuleField }
  | { kind: "launchers" }
  | { kind: "value"; text: () => string }
  | { kind: "action"; name: ActionName; text: string; tone?: "danger"; confirm?: string }
  | { kind: "row-action"; name: ActionName; icon: string; confirm: string };

export interface Row {
  path: string;
  label: string;
  hint?: string;
  control: Control;
  visible?: () => boolean;
  visibleWhen?: string;
}

export interface Identity {
  name: string;
  version: () => string;
}

export interface Section {
  title?: string;
  description?: string;
  rows: Row[];
  dependsOn?: string;
  requiresIntegration?: IntegrationName;
  notes?: string[];
  notesTone?: "warning" | "info";
  footer?: string;
}

export interface Pane {
  id: PaneId;
  label: string;
  icon: string;
  tint: string;
  identity?: Identity;
  sections: Section[];
}
