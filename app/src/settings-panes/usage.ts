import { strings } from "../strings";
import { ICON_USAGE } from "../settings-icons";
import { Pane } from "../settings-types";

const copy = strings.settings;

export const usage: Pane = {
  id: "usage",
  label: copy.panes.usage,
  icon: ICON_USAGE,
  tint: "var(--tint-usage)",
  sections: [
    {
      title: copy.usage.limits,
      rows: [
        {
          path: "usage.show_limits",
          label: copy.usage.showLimits,
          hint: copy.usage.showLimitsHint,
          control: { kind: "switch" },
        },
        {
          path: "usage.use_claude_login",
          label: copy.usage.useClaudeLogin,
          hint: copy.usage.useClaudeLoginHint,
          control: { kind: "switch" },
        },
        {
          path: "usage.value_mode",
          label: copy.usage.valueMode,
          control: {
            kind: "options",
            choices: [
              ["used", copy.usage.valueUsed],
              ["remaining", copy.usage.valueRemaining],
            ],
          },
        },
        {
          path: "usage.preferred_provider",
          label: copy.usage.preferredProvider,
          control: {
            kind: "options",
            choices: [
              ["auto", copy.usage.providerAuto],
              ["anthropic", copy.usage.providerAnthropic],
              ["codex", copy.usage.providerCodex],
            ],
          },
        },
        {
          path: "usage.show_reset_cards",
          label: copy.usage.showResetCards,
          hint: copy.usage.showResetCardsHint,
          control: { kind: "switch" },
        },
        {
          path: "usage.codex_credit_display",
          label: copy.usage.codexCredits,
          hint: copy.usage.codexCreditsHint,
          control: {
            kind: "options",
            choices: [
              ["credits", copy.usage.codexCreditsCredits],
              ["dollars", copy.usage.codexCreditsDollars],
            ],
          },
        },
      ],
    },
    {
      title: copy.usage.alert,
      dependsOn: "usage.show_limits",
      rows: [
        {
          path: "usage.warn_threshold",
          label: copy.usage.warnThreshold,
          hint: copy.usage.warnThresholdHint,
          control: { kind: "percent", min: 50, max: 100, step: 5 },
        },
        {
          path: "usage.refresh_interval_ms",
          label: copy.usage.refreshInterval,
          control: { kind: "duration", unit: "min", min: 1, max: 60, step: 1 },
        },
      ],
    },
    {
      title: copy.usage.bridge,
      rows: [],
      notesTone: "info",
      notes: [copy.usage.bridgeAnthropic, copy.usage.bridgeCodex],
    },
  ],
};
