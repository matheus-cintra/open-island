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
