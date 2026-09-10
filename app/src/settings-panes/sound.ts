import { strings } from "../strings";
import { ICON_SOUND } from "../settings-icons";
import { Pane } from "../settings-types";

const copy = strings.settings;

export const sound: Pane = {
  id: "sound",
  label: copy.panes.sound,
  icon: ICON_SOUND,
  tint: "var(--tint-sound)",
  sections: [
    {
      title: copy.sound.output,
      rows: [
        {
          path: "sound.enabled",
          label: copy.sound.enabled,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.sound.behaviour,
      dependsOn: "sound.enabled",
      rows: [
        {
          path: "sound.volume",
          label: copy.sound.volume,
          control: { kind: "volume" },
        },
        {
          path: "sound.quiet",
          label: copy.sound.quiet,
          hint: copy.sound.quietHint,
          control: { kind: "switch" },
        },
        {
          path: "sound.follow_dnd",
          label: copy.sound.followDnd,
          hint: copy.sound.followDndHint,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.sound.events,
      dependsOn: "sound.enabled",
      rows: [
        {
          path: "sound.events.session_start",
          label: copy.sound.sessionStart,
          control: { kind: "sound" },
        },
        {
          path: "sound.events.task_complete",
          label: copy.sound.taskComplete,
          control: { kind: "sound" },
        },
        {
          path: "sound.events.approval_needed",
          label: copy.sound.approvalNeeded,
          control: { kind: "sound" },
        },
        {
          path: "sound.events.task_acknowledge",
          label: copy.sound.taskAcknowledge,
          control: { kind: "sound" },
        },
        {
          path: "sound.events.idle_reminder",
          label: copy.sound.idleReminder,
          control: { kind: "sound" },
        },
        {
          path: "sound.events.context_limit",
          label: copy.sound.contextLimit,
          control: { kind: "sound" },
        },
        {
          path: "sound.events.user_spam",
          label: copy.sound.userSpam,
          hint: copy.sound.userSpamHint,
          control: { kind: "sound" },
        },
      ],
    },
    {
      title: copy.sound.spam,
      dependsOn: "sound.enabled",
      rows: [
        {
          path: "sound.spam_threshold",
          label: copy.sound.spamThreshold,
          control: { kind: "size", min: 2, max: 10, step: 1, unit: "" },
        },
        {
          path: "sound.spam_window_ms",
          label: copy.sound.spamWindow,
          control: { kind: "duration", unit: "s", min: 2, max: 60, step: 1 },
        },
      ],
    },
    {
      title: copy.sound.quietHours,
      description: copy.sound.quietHoursHint,
      rows: [
        {
          path: "sound.quiet_hours",
          label: copy.sound.quietHours,
          control: { kind: "switch" },
        },
        {
          path: "sound.quiet_hours_start",
          label: copy.sound.quietHoursStart,
          control: { kind: "time" },
        },
        {
          path: "sound.quiet_hours_end",
          label: copy.sound.quietHoursEnd,
          control: { kind: "time" },
        },
      ],
    },
    {
      title: copy.sound.mySounds,
      rows: [],
      notesTone: "info",
      notes: [],
    },
  ],
};
