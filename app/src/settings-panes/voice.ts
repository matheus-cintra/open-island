import type { Pane } from "../settings-types";
import { strings } from "../strings";
import { voiceSettings } from "../settings-voice";
import { MICROPHONE } from "../voice-icons";

const copy = strings.voice;
export const voice: Pane = {
  id: "voice", label: copy.title, icon: MICROPHONE, tint: "var(--tint-sound)",
  sections: [{
    rows: [
      { path: "voice.status", label: copy.modelStatus, control: { kind: "value", text: () => voiceSettings.summary() } },
      { path: "voice.pick", label: copy.modelStatus, hint: copy.explanation, control: { kind: "action", name: "pickVoiceModel", text: copy.model } },
      { path: "voice.refresh", label: copy.refreshModel, control: { kind: "action", name: "refreshVoiceModel", text: copy.refreshModel } },
      { path: "voice.clear", label: copy.removeModel, hint: copy.removeHint, control: { kind: "action", name: "clearVoiceModel", text: copy.removeModel, confirm: copy.removeConfirm } },
      { path: "voice.privacy", label: copy.privacy, control: { kind: "action", name: "voicePrivacy", text: copy.privacy }, visible: () => document.documentElement.dataset.platform === "macos" },
    ],
  }],
};
