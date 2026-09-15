import { Pane } from "../settings-types";
import { general } from "./general";
import { integrations } from "./integrations";
import { filters } from "./filters";
import { display } from "./display";
import { sound } from "./sound";
import { usage } from "./usage";
import { about } from "./about";
import { voice } from "./voice";

export const PANES: Pane[] = [general, integrations, filters, display, sound, voice, usage, about];
