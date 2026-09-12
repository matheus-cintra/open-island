import { strings } from "../strings";
import { ICON_INTEGRATIONS } from "../settings-icons";
import { Pane } from "../settings-types";

const copy = strings.settings;

export const integrations: Pane = {
  id: "integrations",
  label: copy.panes.integrations,
  icon: ICON_INTEGRATIONS,
  tint: "var(--tint-integrations)",
  sections: [
    {
      title: "Mensagens pela ilha",
      footer: "Vale para novas sessões em Bash, Zsh e Fish, em qualquer terminal. Após ativar ou desativar, abra uma nova aba. Aliases e funções personalizados são preservados.",
      rows: [{
        path: "integration.input",
        label: "Enviar mensagens aos agentes pelo terminal",
        hint: "Sessões abertas pelo botão + já incluem esse recurso. Ative também para os comandos digitados no terminal.",
        control: { kind: "integration", name: "input" },
      }],
    },
    {
      title: copy.integrations.agents,
      footer: copy.integrations.agentsFooter,
      rows: [
        {
          path: "integration.claude",
          label: copy.integrations.claude,
          control: { kind: "integration", name: "claude" },
        },
        {
          path: "integration.codex",
          label: copy.integrations.codex,
          control: { kind: "integration", name: "codex" },
        },
        {
          path: "integration.opencode",
          label: copy.integrations.opencode,
          control: { kind: "integration", name: "opencode" },
        },
        {
          path: "integrations.auto_configure",
          label: copy.integrations.autoConfigure,
          hint: copy.integrations.autoConfigureHint,
          control: { kind: "switch" },
        },
      ],
    },
    {
      title: copy.integrations.codexSteps,
      requiresIntegration: "codex",
      rows: [],
      notes: [copy.integrations.codexTrust, copy.integrations.codexUserInput],
    },
  ],
};
