import { strings } from "../strings";
import { ICON_GENERAL } from "../settings-icons";
import { Pane } from "../settings-types";

const copy = strings.settings;

export const general: Pane = {
  id: "general",
  label: copy.panes.general,
  icon: ICON_GENERAL,
  tint: "var(--tint-general)",
  sections: [
    {
      title: copy.general.system,
      rows: [
        {
          path: "integration.autostart",
          label: copy.general.autostart,
          hint: copy.general.autostartHint,
          control: { kind: "integration", name: "autostart" },
        },
        {
          path: "integration.hyprland",
          label: copy.general.hyprland,
          hint: copy.general.hyprlandHint,
          control: { kind: "integration", name: "hyprland" },
        },
      ],
    },
    {
      title: "Novas sessões",
      rows: [{
        path: "integrations.macos_terminal",
        label: "Abrir no terminal",
        hint: "Escolha um aplicativo instalado. A sessão abre na pasta selecionada em uma nova janela.",
        control: { kind: "options", choices: [["terminal", "Terminal.app"], ["iterm2", "iTerm2"], ["warp", "Warp"], ["wezterm", "WezTerm"], ["kitty", "Kitty"]] },
      }],
    },
    {
      title: copy.general.island,
      rows: [
        {
          path: "island.hover_dwell_ms",
          label: copy.general.hoverDwell,
          control: { kind: "size", min: 0, max: 800, step: 50, unit: "s", scale: 1000 },
        },
        {
          path: "island.auto_collapse_ms",
          label: copy.general.autoCollapse,
          control: { kind: "duration", unit: "s", min: 0.5, max: 30, step: 0.5 },
        },
        {
          path: "island.idle_fade",
          label: copy.general.idleFade,
          hint: copy.general.idleFadeHint,
          control: { kind: "switch" },
        },
        {
          path: "island.idle_fade_ms",
          label: copy.general.idleFadeAfter,
          visibleWhen: "island.idle_fade",
          control: { kind: "duration", unit: "s", min: 5, max: 600, step: 5 },
        },
        {
          path: "island.expand_on_hover",
          label: copy.general.expandOnHover,
          control: { kind: "switch" },
        },
        {
          path: "island.smart_suppression",
          label: copy.general.smartSuppression,
          hint: copy.general.smartSuppressionHint,
          control: { kind: "switch" },
        },
        {
          path: "island.collapse_on_leave",
          label: copy.general.collapseOnLeave,
          control: { kind: "switch" },
        },
        {
          path: "island.hide_in_fullscreen",
          label: copy.general.hideInFullscreen,
          hint: copy.general.hideInFullscreenHint,
          control: { kind: "switch" },
        },
        {
          path: "island.hide_when_idle",
          label: copy.general.hideWhenIdle,
          hint: copy.general.hideWhenIdleHint,
          control: { kind: "switch" },
        },
        {
          path: "island.click_to_jump",
          label: copy.general.clickToJump,
          hint: copy.general.clickToJumpHint,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.general.sessions,
      rows: [
        {
          path: "sessions.idle_after_ms",
          label: copy.general.idleAfter,
          hint: copy.general.idleAfterHint,
          control: { kind: "duration", unit: "min", min: 1, max: 120, step: 1 },
        },
        {
          path: "notifications.idle_reminder_after_ms",
          label: copy.general.idleReminderAfter,
          hint: copy.general.idleReminderAfterHint,
          control: { kind: "duration", unit: "min", min: 1, max: 120, step: 1 },
        },
        {
          path: "sessions.cleanup_after_ms",
          label: copy.general.cleanupAfter,
          hint: copy.general.cleanupAfterHint,
          control: { kind: "duration", unit: "min", min: 0, max: 1440, step: 15 },
        },
      ],
    },
  ],
};
