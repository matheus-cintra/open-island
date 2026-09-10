import { strings } from "../strings";
import { ICON_DISPLAY } from "../settings-icons";
import { Pane } from "../settings-types";

const copy = strings.settings;

export const display: Pane = {
  id: "display",
  label: copy.panes.display,
  icon: ICON_DISPLAY,
  tint: "var(--tint-display)",
  sections: [
    {
      title: copy.display.notch,
      rows: [
        {
          path: "display.compact_layout",
          label: copy.display.notch,
          control: {
            kind: "tiles",
            choices: [
              ["clean", copy.display.compactClean, copy.display.compactCleanHint],
              ["detailed", copy.display.compactDetailed, copy.display.compactDetailedHint],
            ],
          },
        },
        {
          path: "display.monitor",
          label: copy.display.monitor,
          hint: copy.display.monitorHint,
          control: { kind: "options", choices: [["", copy.display.monitorAuto]] },
        },
      ],
    },
    {
      title: copy.display.sessionCard,
      rows: [
        { path: "display.project", label: copy.display.project, control: { kind: "switch" } },
        { path: "display.worktree", label: copy.display.worktree, control: { kind: "switch" } },
        {
          path: "display.agent_icons",
          label: copy.display.agentIcons,
          hint: copy.display.agentIconsHint,
          control: { kind: "switch" },
        },
        {
          path: "display.terminal_icons",
          label: copy.display.terminalIcons,
          hint: copy.display.terminalIconsHint,
          control: { kind: "switch" },
        },
        { path: "display.model", label: copy.display.model, control: { kind: "switch" } },
        {
          path: "display.effort",
          label: copy.display.effort,
          hint: copy.display.effortHint,
          control: { kind: "switch" },
        },
        {
          path: "display.tasks",
          label: copy.display.tasks,
          hint: copy.display.tasksHint,
          control: { kind: "switch" },
        },
        {
          path: "display.activity",
          label: copy.display.activity,
          hint: copy.display.activityHint,
          control: { kind: "switch" },
        },
        {
          path: "display.subagents",
          label: copy.display.subagents,
          hint: copy.display.subagentsHint,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.display.panelSize,
      rows: [
        {
          path: "display.content_font",
          label: copy.display.contentFont,
          hint: copy.display.contentFontHint,
          control: {
            kind: "picker",
            values: [9, 10, 11, 12, 13, 14, 15, 16],
            unit: "px",
            defaultValue: 11,
          },
        },
        {
          path: "display.completion_card_height",
          label: copy.display.completionCardHeight,
          control: { kind: "size", min: 60, max: 240, step: 10, unit: "px" },
        },
        {
          path: "display.panel_max_height",
          label: copy.display.panelMaxHeight,
          hint: copy.display.panelMaxHeightHint,
          control: { kind: "size", min: 320, max: 1200, step: 20, unit: "px" },
        },
        {
          path: "display.panel_max_width",
          label: copy.display.panelMaxWidth,
          control: { kind: "size", min: 480, max: 1000, step: 8, unit: "px" },
        },
      ],
    },
    {
      title: copy.display.notchTuning,
      footer: copy.display.notchTuningHint,
      rows: [
        {
          path: "display.notch_width_offset",
          label: copy.display.notchWidth,
          control: { kind: "size", min: -12, max: 12, step: 1, unit: "px" },
        },
        {
          path: "display.notch_height_offset",
          label: copy.display.notchHeight,
          control: { kind: "size", min: -12, max: 12, step: 1, unit: "px" },
        },
        {
          path: "display.island_height",
          label: copy.display.islandHeight,
          hint: copy.display.islandHeightHint,
          control: { kind: "size", min: 0, max: 200, step: 2, unit: "px" },
        },
      ],
    },
    {
      title: copy.display.size,
      rows: [
        {
          path: "display.ui_scale",
          label: copy.display.uiScale,
          hint: copy.display.uiScaleHint,
          control: { kind: "scale" },
        },
      ],
    },
  ],
};
