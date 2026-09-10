import { strings } from "../strings";
import { ICON_FILTERS } from "../settings-icons";
import { Pane } from "../settings-types";

const copy = strings.settings;

const REMINDER_DELAYS = [0, 600_000, 1_800_000, 3_600_000] as const;

function reminderDelayLabel(value: number): string {
  if (value === 0) return copy.filters.reminderOff;
  const minutes = Math.round(value / 60_000);
  if (minutes % 60 === 0) {
    const hours = minutes / 60;
    return copy.filters.reminderAfter(hours === 1 ? "1 hora" : `${hours} horas`);
  }
  return copy.filters.reminderAfter(`${minutes} minutos`);
}

export const filters: Pane = {
  id: "filters",
  label: copy.panes.filters,
  icon: ICON_FILTERS,
  tint: "var(--tint-filters)",
  sections: [
    {
      title: copy.filters.panel,
      footer: copy.filters.panelFooter,
      rows: [
        {
          path: "notifications.expand_on_completion",
          label: copy.filters.expandOnCompletion,
          hint: copy.filters.expandOnCompletionHint,
          control: { kind: "switch" },
        },
        {
          path: "notifications.expand_on_question",
          label: copy.filters.expandOnQuestion,
          hint: copy.filters.expandOnQuestionHint,
          control: { kind: "switch" },
        },
        {
          path: "notifications.subagent_timing",
          label: copy.filters.subagentTiming,
          control: {
            kind: "options",
            choices: [
              ["root_responses", copy.filters.subagentRoot],
              ["all_finished", copy.filters.subagentAllFinished],
              ["every_completion", copy.filters.subagentEvery],
            ],
          },
        },
      ],
    },
    {
      title: copy.filters.reminders,
      rows: [
        {
          path: "notifications.idle_reminder_after_ms",
          label: copy.filters.reminderDelay,
          hint: copy.filters.reminderDelayHint,
          control: {
            kind: "picker",
            values: REMINDER_DELAYS,
            unit: "",
            defaultValue: 300_000,
            label: reminderDelayLabel,
          },
        },
      ],
    },
    {
      title: copy.filters.reminderInclude,
      dependsOn: "notifications.idle_reminder_after_ms",
      rows: [
        {
          path: "notifications.reminder_needs_response",
          label: copy.filters.reminderNeedsResponse,
          hint: copy.filters.reminderNeedsResponseHint,
          control: { kind: "switch" },
        },
        {
          path: "notifications.reminder_completed_tasks",
          label: copy.filters.reminderCompletedTasks,
          hint: copy.filters.reminderCompletedTasksHint,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.filters.quietScenes,
      description: copy.filters.quietScenesHint,
      rows: [
        {
          path: "filters.quiet.focus_mode",
          label: copy.filters.quietFocusMode,
          hint: copy.filters.quietFocusModeHint,
          control: { kind: "switch" },
        },
        {
          path: "filters.quiet.screen_off",
          label: copy.filters.quietScreenOff,
          hint: copy.filters.quietScreenOffHint,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.filters.launchers,
      description: copy.filters.launchersHint,
      footer: copy.filters.launchersFooter,
      rows: [
        {
          path: "filters.launchers",
          label: copy.filters.launchers,
          control: { kind: "launchers" },
        },
      ],
    },
    {
      title: copy.filters.directory,
      description: copy.filters.directoryHint,
      rows: [
        {
          path: "filters.rules",
          label: copy.filters.directory,
          control: { kind: "rules", field: "cwd" },
        },
      ],
    },
    {
      title: copy.filters.prompt,
      description: copy.filters.promptHint,
      footer: copy.filters.footer,
      rows: [
        {
          path: "filters.rules",
          label: copy.filters.prompt,
          control: { kind: "rules", field: "prompt" },
        },
      ],
    },
  ],
};
