import { diagnostics } from "../settings-diagnostics";
import { strings } from "../strings";
import { ICON_EXIT, ICON_INFO, ICON_TRASH } from "../settings-icons";
import { Pane } from "../settings-types";
import { appVersion, availableUpdate } from "../settings";

const copy = strings.settings;

export const about: Pane = {
  id: "about",
  label: copy.panes.about,
  icon: ICON_INFO,
  tint: "var(--tint-about)",
  identity: { name: copy.about.name, version: () => `v${appVersion}` },
  sections: [

    {
      rows: [
        {
          path: "about.update",
          label: copy.about.updateAvailable,
          hint: copy.about.updateAvailableHint,
          control: { kind: "value", text: () => availableUpdate ?? "" },
          visible: () => availableUpdate !== null,
        },
        {
          path: "updates.check_enabled",
          label: copy.about.updateCheck,
          hint: copy.about.updateCheckHint,
          control: { kind: "switch" },
        },
        {
          path: "about.check_update",
          label: copy.about.checkUpdate,
          hint: copy.about.checkUpdateHint,
          control: { kind: "action", name: "checkUpdate", text: copy.about.checkUpdateNow },
        },
      ],
    },
    {
      rows: [
        { path: "about.diagnostics", label: copy.about.diagnostics, hint: copy.about.diagnosticHint, control: { kind: "value", text: () => diagnostics.summary() } },
        { path: "about.diagnostic_versions", label: copy.about.name, control: { kind: "value", text: () => diagnostics.versions() }, visible: () => diagnostics.available() },
        { path: "about.diagnostic_capacity", label: copy.about.diagnosticCapacityLabel, control: { kind: "value", text: () => diagnostics.capacity() }, visible: () => diagnostics.countersAvailable() },
        { path: "about.diagnostic_service", label: copy.about.diagnosticServiceLabel, control: { kind: "value", text: () => diagnostics.service() }, visible: () => diagnostics.localAvailable() },
        { path: "about.diagnostic_hooks", label: copy.about.diagnosticHooksLabel, control: { kind: "value", text: () => diagnostics.hooks() }, visible: () => diagnostics.localAvailable() },
        { path: "about.diagnostic_voice", label: strings.voice.title, control: { kind: "value", text: () => diagnostics.model() }, visible: () => diagnostics.available() },
        { path: "about.diagnostic_audio", label: copy.about.diagnosticAudioLabel, control: { kind: "value", text: () => diagnostics.audio() }, visible: () => diagnostics.available() },
        { path: "about.refresh_diagnostics", label: copy.about.diagnosticRefresh, control: { kind: "action", name: "refreshDiagnostics", text: copy.about.diagnosticRefresh } },
        { path: "about.copy_diagnostics", label: copy.about.diagnosticCopy, control: { kind: "action", name: "copyDiagnostics", text: copy.about.diagnosticCopy }, visible: () => diagnostics.available() },
      ],
    },
    {
      rows: [
        {
          path: "about.acknowledgements",
          label: copy.about.acknowledgements,
          control: { kind: "value", text: () => copy.about.credits },
        },
      ],
    },
    {
      rows: [
        {
          path: "about.remove",
          label: copy.about.removeAutoConfig,
          control: {
            kind: "row-action",
            name: "removeAutoConfig",
            icon: ICON_TRASH,
            confirm: copy.about.removeAutoConfigConfirm,
          },
        },
      ],
      footer: copy.about.removeAutoConfigHint,
    },
    {
      rows: [
        {
          path: "about.quit",
          label: copy.about.quit,
          control: {
            kind: "row-action",
            name: "quit",
            icon: ICON_EXIT,
            confirm: copy.about.quitConfirm,
          },
        },
      ],
    },
  ],
};
