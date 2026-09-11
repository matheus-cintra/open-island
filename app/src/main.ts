import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { strings } from "./strings";
import {
  ApprovalDecision,
  ApprovalRequest,
  ApprovalResolved,
  DiffLine,
  FocusState,
  PointerState,
  QuietScenes,
  RowVisibility,
  Session,
  Size,
  SubagentTiming,
  UsageReport,
  UsageSnapshot,
} from "./types";
import { isRecord, milliseconds, planSummary, reasonOf, section, stringField } from "./json";
import {
  islandEl,
  compactViewEl,
  expandedViewEl,
  headerLabelEl,
  headerUsageEl,
  headerUsageModelEl,
  sessionListEl,
  jumpErrorEl,
  approvalCardEl,
  approvalToolEl,
  approvalSummaryEl,
  approvalAllowEl,
  approvalAlwaysEl,
  approvalDenyEl,
  approvalDiffEl,
  approvalKickerEl,
  approvalActionsEl,
  questionCardEl,
  headerNewSessionEl,
  newSessionCardEl,
  newSessionKickerEl,
  newSessionAgentsEl,
  newSessionCloseEl,
  headerMuteEl,
  headerSettingsEl,
  headerUpdateEl,
  muteWavesEl,
  muteCrossEl,
} from "./elements";
import { listKey, renderCompact, renderList, tickElapsed } from "./row";
import {
  closeQuestion,
  openQuestion,
  parseQuestion,
  parseQuestionResolution,
  pendingQuestion,
  renderQuestion,
} from "./question";

export { badgeSpec, createRow, fillBadges, fillRow } from "./row";

export let compactClean = false;
let notchWidth = 0;
let physicalNotchWidth = 0;
let hasPhysicalNotch = false;
let physicalNotchHeight = 0;
let notchHeight = 0;
let islandHeight = 0;
export let show: RowVisibility = {
  tasks: true,
  project: true,
  worktree: true,
  agentIcons: true,
  terminalIcons: true,
  model: true,
  effort: false,
  activity: true,
  subagents: true,
};

const ALWAYS_CAPABLE = new Set(["claude", "opencode"]);
const PLAN_TOOL = "ExitPlanMode";

function supportsAlways(approval: ApprovalRequest): boolean {
  if (approval.tool_name === PLAN_TOOL) return false;
  return ALWAYS_CAPABLE.has(approval.session_id.split(":")[0] ?? "");
}

/* ------------------------------------------------------------------ */
/* Sizes / tuning                                                      */
/* ------------------------------------------------------------------ */

const BASE_COMPACT: Size = { w: 232, h: 46 };
const BASE_SCOOP = 8;
const BASE_EXPANDED: Size = { w: 664, h: 158 };
let uiScale = 1;
let COMPACT: Size = { ...BASE_COMPACT };
let EXPANDED: Size = { ...BASE_EXPANDED };
let expandedMaxH = 720;
const EXPANDED_MAX_FLOOR = 316;
const EXPANDED_PAD_BOTTOM = 18;
const BADGE_ICON = 16;
const COMPACT_OVERHANG = 4;
const MIN_COMPACT_W = 120;
const MIN_COMPACT_H = 16;
const CARD_SLACK = 24;
const TWEEN_MS = 320;
export const LEAVE_MS = 200;

const SPRING_DAMPING = 0.7;
const SPRING_OMEGA = 10;

let dwellMs = 250;
let autoCollapseMs = 2500;
let idleMs = 120_000;
const AGENT_IDLE_HIDE_MS = 30_000;
let expandOnHover = true;
let collapseOnLeave = true;
let hideInFullscreen = true;
let hideWhenIdle = false;
let idleFadeEnabled = false;
let clickToJump = true;
let smartSuppression = true;
let expandOnCompletion = true;
let expandOnQuestion = true;
let subagentTiming: SubagentTiming = "root_responses";
let quietScene = false;
let focusedPid: number | null = null;
let doneSubagents = new Map<string, Set<string>>();
let fullscreenActive = false;
let agentsIdle = false;
let agentIdleTimer = 0;

/* QA: multiply the pixel-shift period by 0.02 so the drift is visible  */
/* within seconds instead of minutes.                                   */
const DEBUG_SPEED = false;
const PIXEL_PERIOD_X = 180_000 * (DEBUG_SPEED ? 0.02 : 1);
const PIXEL_PERIOD_Y = 291_240 * (DEBUG_SPEED ? 0.02 : 1);

/* ------------------------------------------------------------------ */
/* State                                                               */
/* ------------------------------------------------------------------ */

export let expanded = false;
let hovered = false;
export let sessions: Session[] = [];
let lastListKey = "";
let pendingApproval: ApprovalRequest | null = null;
let resolvingApproval = false;
let launchOpen = false;
let config: Record<string, unknown> = {};

const USAGE_STALE_AFTER_MS = 900_000;
let usage: UsageReport = { providers: [] };
let availableUpdate: string | null = null;
let usageOptions = {
  showLimits: true,
  remaining: false,
  preferred: "auto",
  showResetCards: true,
  creditDisplay: "credits",
  warnThreshold: 90,
};
let quiet = false;

/* ------------------------------------------------------------------ */
/* Window morph                                                        */
/* ------------------------------------------------------------------ */

let morphRaf = 0;
let morphStart = 0;
let morphFrom: Size = { ...COMPACT };
let morphGoal: Size = { ...COMPACT };
let curSize: Size = { ...COMPACT };

function spring(t: number): number {
  if (t >= 1) return 1;
  const damped = SPRING_OMEGA * Math.sqrt(1 - SPRING_DAMPING * SPRING_DAMPING);
  const decay = Math.exp(-SPRING_DAMPING * SPRING_OMEGA * t);
  const phase =
    Math.cos(damped * t) +
    (SPRING_DAMPING / Math.sqrt(1 - SPRING_DAMPING * SPRING_DAMPING)) * Math.sin(damped * t);
  return 1 - decay * phase;
}

const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");

export function reducedMotion(): boolean {
  return motionQuery.matches;
}

const CARD_LEAVE_MS = 110;

export function showCard(card: HTMLElement, visible: boolean, onHidden?: () => void): void {
  if (visible) {
    card.classList.remove("is-leaving");
    if (card.hidden) {
      card.hidden = false;
      card.classList.remove("is-entering");
      void card.offsetWidth;
      card.classList.add("is-entering");
    }
    return;
  }
  if (card.hidden || card.classList.contains("is-leaving")) {
    if (card.hidden) onHidden?.();
    card.hidden = true;
    return;
  }
  if (reducedMotion()) {
    card.hidden = true;
    onHidden?.();
    return;
  }
  card.classList.remove("is-entering");
  card.classList.add("is-leaving");
  window.setTimeout(() => {
    card.classList.remove("is-leaving");
    card.hidden = true;
    onHidden?.();
    syncExpandedSize();
  }, CARD_LEAVE_MS);
}

function applyUiScale(scale: number, compactHeight?: number): void {
  const next = Number.isFinite(scale) && scale > 0 ? scale : 1;
  const fixed = islandHeight > 0;
  // A notch has its own content height. A compositor bar measurement (including
  // older macOS backends reporting 46) must not override that safe-area layout.
  const fromCompositor = !hasPhysicalNotch && !fixed && compactHeight !== undefined && compactHeight > 0;
  const base = fixed
    ? islandHeight
    : fromCompositor
      ? compactHeight
      : Math.round(BASE_COMPACT.h * next);
  const floor = fromCompositor ? -(COMPACT_OVERHANG - 1) : -(base - MIN_COMPACT_H);
  // Dimensions now describe the whole panel, including the camera strip.
  // Compact content lives in two wings beside the camera, never beneath it.
  const height = hasPhysicalNotch
    ? Math.max(physicalNotchHeight, fixed ? islandHeight : 0)
    : base + Math.max(floor, notchHeight);
  const width = hasPhysicalNotch
    ? Math.ceil(physicalNotchWidth + (compactClean ? 112 : 208) * next)
    : Math.max(MIN_COMPACT_W, Math.round(BASE_COMPACT.w * next) + notchWidth);
  document.documentElement.style.setProperty("--camera-top", `${physicalNotchHeight / next}px`);
  document.documentElement.style.setProperty("--camera-width", `${physicalNotchWidth / next}px`);
  if (next === uiScale && height === COMPACT.h && width === COMPACT.w) return;
  uiScale = next;
  COMPACT = { w: width, h: height };
  EXPANDED = { w: Math.max(Math.round(BASE_EXPANDED.w * next), hasPhysicalNotch ? Math.ceil(physicalNotchWidth + 320 * next) : 0), h: Math.round(BASE_EXPANDED.h * next) };
  islandEl.style.zoom = String(next);
  islandEl.style.setProperty("--badge-icon", `${Math.round(BADGE_ICON * next)}px`);
  if (expanded) {
    islandEl.style.backgroundImage = expandedShape(EXPANDED.w, curSize.h);
    syncExpandedSize();
  } else {
    islandEl.style.backgroundImage = compactShape(COMPACT.w, COMPACT.h);
    curSize = { ...COMPACT };
    void setIslandSize(COMPACT.w, COMPACT.h);
  }
}

function pillShape(w: number, h: number, scoop: number, bottom: number): string {
  const cut = Math.min(scoop, Math.floor(h / 2));
  const round = Math.min(bottom, Math.floor(h / 2), Math.floor(w / 2) - cut);
  const path =
    `M0 0 A${cut} ${cut} 0 0 1 ${cut} ${cut} L${cut} ${h - round} ` +
    `A${round} ${round} 0 0 0 ${cut + round} ${h} ` +
    `L${w - cut - round} ${h} A${round} ${round} 0 0 0 ${w - cut} ${h - round} ` +
    `L${w - cut} ${cut} A${cut} ${cut} 0 0 1 ${w} 0 Z`;
  const svg =
    `<svg xmlns='http://www.w3.org/2000/svg' width='${w}' height='${h}'>` +
    `<path d='${path}' fill='#000000'/></svg>`;
  return `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
}

function compactShape(w: number, h: number): string {
  return pillShape(w, h, Math.round(BASE_SCOOP * uiScale), Math.round(8 * uiScale));
}

function setMorph(h: number): void {
  if (Math.min(morphFrom.h, morphGoal.h) > COMPACT.h) {
    islandEl.style.setProperty("--morph", "1");
    return;
  }
  const span = Math.max(1, Math.max(morphFrom.h, morphGoal.h) - COMPACT.h);
  const open = Math.min(1, Math.max(0, (h - COMPACT.h) / span));
  islandEl.style.setProperty("--morph", open.toFixed(3));
}

async function setIslandSize(w: number, h: number): Promise<void> {
  try {
    await invoke("set_island_size", { width: w, height: h });
  } catch {
    // Positioning is best-effort; the window still renders.
  }
}

export function morphTo(target: Size, onStart: () => void, onEnd: () => void): void {
  cancelAnimationFrame(morphRaf);
  if (curSize.w === target.w && curSize.h === target.h) {
    morphGoal = target;
    // curSize is only what we last *asked* for: set_island_size is async and
    // best-effort, so the window can already disagree with it. Re-assert the
    // size instead of assuming the previous request stuck, otherwise a dropped
    // resize leaves the island permanently compact while we believe otherwise.
    setMorph(target.h);
    void setIslandSize(target.w, target.h);
    onEnd();
    return;
  }
  morphFrom = { ...curSize };
  morphGoal = target;
  morphStart = performance.now();
  onStart();

  if (reducedMotion()) {
    curSize = { ...morphGoal };
    setMorph(morphGoal.h);
    void setIslandSize(morphGoal.w, morphGoal.h);
    onEnd();
    return;
  }

  const step = (now: number): void => {
    const t = Math.min(1, (now - morphStart) / TWEEN_MS);
    const e = spring(t);
    const w = Math.round(morphFrom.w + (morphGoal.w - morphFrom.w) * e);
    const h = Math.round(morphFrom.h + (morphGoal.h - morphFrom.h) * e);
    curSize = { w, h };
    setMorph(h);
    void setIslandSize(w, h);
    if (t < 1) {
      morphRaf = requestAnimationFrame(step);
    } else {
      curSize = { ...morphGoal };
      setMorph(morphGoal.h);
      void setIslandSize(morphGoal.w, morphGoal.h);
      onEnd();
    }
  };
  morphRaf = requestAnimationFrame(step);
}

function noop(): void {}

function setView(which: "compact" | "expanded"): void {
  const compactActive = which === "compact";
  if (compactActive) islandEl.style.backgroundImage = compactShape(COMPACT.w, COMPACT.h);
  compactViewEl.classList.toggle("visible", compactActive);
  expandedViewEl.classList.toggle("visible", !compactActive);
  compactViewEl.setAttribute("aria-hidden", String(compactActive));
  expandedViewEl.setAttribute("aria-hidden", String(!compactActive));
}

// Deliberately not guarded by `if (expanded) return`: an enter/leave landing
// mid-tween could leave the flag true while the window was still compact, and
// every later hover then became a no-op. Re-applying a state we are already in
// is harmless because morphTo cancels any in-flight tween.
// The expanded pill is a single background SVG stretched to the window, so a taller
// window would stretch its corners too. The path is parametric in width and height, so
// it is regenerated for whatever height a card needs.
function expandedShape(w: number, h: number): string {
  return pillShape(w, h, Math.round(BASE_SCOOP * uiScale), Math.round(16.5 * uiScale));
}

export function maxExpandedHeight(): number {
  const third = Math.round(window.screen.height / 3);
  if (!Number.isFinite(third) || third <= 0) return expandedMaxH;
  return Math.min(expandedMaxH, Math.max(EXPANDED_MAX_FLOOR, third));
}

function leavingHeight(): number {
  const leaving = sessionListEl.querySelectorAll<HTMLElement>("li.is-leaving");
  if (leaving.length === 0) return 0;
  const gap = Number.parseFloat(getComputedStyle(sessionListEl).rowGap) || 0;
  let total = 0;
  for (const li of leaving) total += li.offsetHeight + gap;
  return total;
}

export function expandedSize(): Size {
  const listBottom =
    sessionListEl.offsetTop +
    sessionListEl.scrollHeight -
    leavingHeight() +
    EXPANDED_PAD_BOTTOM;
  const cardBottom = pendingQuestion
    ? expandedViewEl.scrollHeight + CARD_SLACK
    : 0;
  const needed = Math.ceil(Math.max(listBottom, cardBottom) * uiScale);
  const staying = sessionListEl.querySelectorAll("li:not(.is-leaving)").length;
  const floor = staying === 0 ? EXPANDED.h : COMPACT.h;
  return {
    w: EXPANDED.w,
    h: Math.min(maxExpandedHeight(), Math.max(floor, needed)),
  };
}

function expand(): void {
  expanded = true;
  void invoke("island_keyboard", { active: true }).catch(() => {});
  // Swap SVG + content at tween START (expanding).
  islandEl.classList.add("expanded");
  setView("expanded");
  const target = expandedSize();
  islandEl.style.backgroundImage = expandedShape(target.w, target.h);
  morphTo(target, noop, noop);
}

function collapse(): void {
  expanded = false;
  void invoke("island_keyboard", { active: false }).catch(() => {});
  // Keep the expanded SVG during the shrink; swap at tween END.
  morphTo(COMPACT, noop, () => {
    islandEl.classList.remove("expanded");
    islandEl.style.removeProperty("background-image");
    setView("compact");
  });
}

/** Re-morph after a card appeared or went away while the island was already expanded. */
export function syncExpandedSize(): void {
  if (!expanded) return;
  const target = expandedSize();
  if (target.w === morphGoal.w && target.h === morphGoal.h) return;
  islandEl.style.backgroundImage = expandedShape(target.w, target.h);
  morphTo(target, noop, noop);
}

/* ------------------------------------------------------------------ */
/* Hover / dwell state machine                                         */
/* ------------------------------------------------------------------ */

let dwellTimer = 0;
let autoCollapseTimer = 0;

function pointerEntered(): void {
  if (hovered) return;
  hovered = true;
  clearTimeout(dwellTimer);
  clearTimeout(autoCollapseTimer);
  if (!expandOnHover) return;
  dwellTimer = window.setTimeout(() => {
    if (!expanded) expand();
  }, dwellMs);
}

function pointerLeft(): void {
  if (!hovered) return;
  hovered = false;
  clearTimeout(dwellTimer);
  clearTimeout(autoCollapseTimer);
  if (collapseOnLeave && expanded && !launchOpen) collapse();
}

islandEl.addEventListener("mouseenter", pointerEntered);
islandEl.addEventListener("mouseleave", pointerLeft);

export function admitsExpansion(reason: "completion" | "question" | "approval"): boolean {
  if (quietScene) return false;
  if (reason === "completion") return expandOnCompletion;
  if (reason === "question") return expandOnQuestion;
  return true;
}

export function terminalIsFocused(list: Session[]): boolean {
  if (focusedPid === null) return false;
  return list.some((session) => session.raise_pid === focusedPid);
}

export function subagentEdge(list: Session[]): boolean {
  const next = new Map<string, Set<string>>();
  let fired = false;
  for (const session of list) {
    const done = new Set(
      (session.subagents ?? []).filter((agent) => agent.done).map((agent) => agent.id),
    );
    next.set(session.id, done);
    if (subagentTiming === "root_responses") continue;
    const before = doneSubagents.get(session.id);
    if (before === undefined) continue;
    const arrived = [...done].filter((id) => !before.has(id));
    if (arrived.length === 0) continue;
    if (subagentTiming === "every_completion") {
      fired = true;
      continue;
    }
    if (done.size === (session.subagents ?? []).length) fired = true;
  }
  doneSubagents = next;
  return fired;
}

function showActivity(forceExpand = false): void {
  if (forceExpand || !hovered) expand();
  clearTimeout(autoCollapseTimer);
  autoCollapseTimer = window.setTimeout(() => {
    if (!hovered && pendingApproval === null && pendingQuestion === null && !launchOpen) collapse();
  }, autoCollapseMs);
}

function onSessions(list: Session[]): void {
  sessions = list;
  trackAgentIdle(list);
  const key = listKey(list);
  const changed = key !== lastListKey;
  lastListKey = key;
  const subagentDone = subagentEdge(list);
  render();
  const suppressed = smartSuppression && terminalIsFocused(list);
  if ((changed || subagentDone) && !hovered && admitsExpansion("completion") && !suppressed) {
    showActivity();
  }
  resetIdle();
}

export function render(): void {
  const n = sessions.length;
  renderCompact(n);
  headerLabelEl.textContent = strings.island.sessions(n);
  renderUsage();
  renderList();
  renderApproval();
  renderQuestion();
  syncExpandedSize();
}


function usageProvider(): UsageSnapshot | null {
  const leading = sessions[0]?.agent;
  const wanted =
    usageOptions.preferred === "auto"
      ? leading === "codex"
        ? "codex"
        : "anthropic"
      : usageOptions.preferred;
  const order = wanted === "codex" ? ["codex", "anthropic"] : ["anthropic", "codex"];
  for (const name of order) {
    const entry = usage.providers.find((provider) => provider.provider === name);
    if (entry?.snapshot !== undefined && entry.snapshot.windows.length > 0) return entry.snapshot;
  }
  return null;
}

function usageValue(percent: number): number {
  return usageOptions.remaining ? Math.max(0, 100 - percent) : percent;
}

function usageSeverity(percent: number): string {
  if (percent >= usageOptions.warnThreshold) return "is-critical";
  if (percent >= 50) return "is-high";
  return "";
}

function usageWindowEl(name: string, percent: number, resetsAt: number | undefined): HTMLElement {
  const wrapper = document.createElement("span");
  wrapper.className = "usage-window";
  const label = document.createElement("span");
  label.className = "usage-name";
  label.textContent = name;
  const value = document.createElement("span");
  value.className = `usage-percent ${usageSeverity(percent)}`.trim();
  value.textContent = strings.usage.percent(usageValue(percent));
  wrapper.append(label, value);
  if (resetsAt !== undefined) {
    const remaining = resetsAt - Date.now();
    if (remaining > 0) {
      const reset = document.createElement("span");
      reset.className = "usage-reset";
      reset.textContent = strings.usage.resetIn(remaining);
      wrapper.append(reset);
    }
  }
  return wrapper;
}

function creditText(credits: { balance: number; unlimited: boolean }): string | null {
  if (credits.unlimited) return strings.usage.creditsUnlimited;
  if (credits.balance <= 0) return null;
  return usageOptions.creditDisplay === "dollars"
    ? strings.usage.dollars(credits.balance)
    : strings.usage.credits(credits.balance);
}

function usageBadgeEl(text: string): HTMLElement {
  const badge = document.createElement("span");
  badge.className = "usage-window";
  const label = document.createElement("span");
  label.className = "usage-name";
  label.textContent = text;
  badge.append(label);
  return badge;
}

function appendModelPiece(piece: HTMLElement): void {
  if (headerUsageModelEl.childElementCount > 0) {
    const separator = document.createElement("span");
    separator.className = "usage-separator";
    separator.textContent = strings.usage.separator;
    headerUsageModelEl.append(separator);
  }
  headerUsageModelEl.append(piece);
}

function renderUsage(): void {
  const snapshot = usageOptions.showLimits ? usageProvider() : null;
  headerUsageEl.replaceChildren();
  headerUsageModelEl.replaceChildren();
  headerUsageEl.hidden = snapshot === null;
  headerUsageModelEl.hidden = snapshot === null;
  headerLabelEl.hidden = snapshot !== null;
  if (snapshot === null) return;

  const stale = Date.now() - snapshot.fetched_at_ms > USAGE_STALE_AFTER_MS;
  headerUsageEl.classList.toggle("is-stale", stale);
  headerUsageModelEl.classList.toggle("is-stale", stale);

  snapshot.windows.forEach((window, index) => {
    if (index > 0) {
      const separator = document.createElement("span");
      separator.className = "usage-separator";
      separator.textContent = strings.usage.separator;
      headerUsageEl.append(separator);
    }
    headerUsageEl.append(usageWindowEl(window.label, window.percent, window.resets_at_ms));
  });
  if (stale) {
    const marker = document.createElement("span");
    marker.className = "usage-stale";
    marker.textContent = strings.usage.stale;
    headerUsageEl.append(marker);
  }

  const codex = usage.providers.find((provider) => provider.provider === "codex")?.snapshot;
  const cards = usageOptions.showResetCards ? (codex?.reset_cards ?? []) : [];
  if (cards.length > 0) appendModelPiece(usageBadgeEl(strings.usage.resetCards(cards.length)));
  const credits = codex?.credits === undefined ? null : creditText(codex.credits);
  if (credits !== null) appendModelPiece(usageBadgeEl(credits));
  const model = snapshot.models?.[0];
  if (model !== undefined) {
    appendModelPiece(usageWindowEl(model.model, model.percent, model.resets_at_ms));
  }
  headerUsageModelEl.hidden = headerUsageModelEl.childElementCount === 0;
}

window.setInterval(tickElapsed, 1000);

/* ------------------------------------------------------------------ */
/* Jump interaction (must NOT collapse the island)                     */
/* ------------------------------------------------------------------ */

let errTimer = 0;

export function showError(message: string): void {
  jumpErrorEl.textContent = message;
  jumpErrorEl.hidden = false;
  clearTimeout(errTimer);
  errTimer = window.setTimeout(() => {
    jumpErrorEl.hidden = true;
  }, 4000);
}

export function jumpTo(session: Session, row: HTMLButtonElement): void {
  if (!clickToJump) return;
  jumpErrorEl.hidden = true;
  clearTimeout(errTimer);

  row.classList.remove("flash");
  void row.offsetWidth; // restart animation on repeated clicks
  row.classList.add("flash");

  invoke("jump", { id: session.id }).catch((err: unknown) => {
    showError(strings.jump.failed(session.title, String(err)));
  });
}

export function jumpToId(id: string): void {
  if (!id) return;
  const session = sessions.find((entry) => entry.id === id);
  invoke("jump", { id }).catch((err: unknown) => {
    showError(strings.jump.failed(session?.title ?? id, String(err)));
  });
}

function parseApproval(value: unknown): ApprovalRequest | null {
  const approvalId = stringField(value, "approval_id");
  const sessionId = stringField(value, "session_id");
  if (!approvalId || !sessionId) return null;
  const toolName = stringField(value, "tool_name");
  const reason = stringField(value, "reason");
  return {
    approval_id: approvalId,
    session_id: sessionId,
    ...(toolName ? { tool_name: toolName } : {}),
    ...(reason ? { reason } : {}),
    ...(isRecord(value) && "tool_input" in value ? { tool_input: value.tool_input } : {}),
  };
}

function parseResolution(value: unknown): ApprovalResolved | null {
  const approvalId = stringField(value, "approval_id");
  const sessionId = stringField(value, "session_id");
  const decision = stringField(value, "decision");
  if (!approvalId || !sessionId || (decision !== "allow" && decision !== "deny")) return null;
  return { approval_id: approvalId, session_id: sessionId, decision };
}

function approvalDescription(approval: ApprovalRequest): string {
  if (approval.reason) return approval.reason;
  if (isRecord(approval.tool_input)) {
    const plan = approval.tool_input.plan;
    if (approval.tool_name === PLAN_TOOL && typeof plan === "string") {
      return planSummary(plan) ?? strings.approval.planFallback;
    }
    const command = approval.tool_input.command;
    if (typeof command === "string" && command.length > 0) return command;
    const path = approval.tool_input.file_path;
    if (typeof path === "string" && path.length > 0) return path;
  }
  return strings.approval.fallback;
}

/// The three shapes the three agents actually send: `old_string`/`new_string` from Edit,
/// `content` from Write, and a unified `patch` from ApplyPatch. Everything else has no
/// diff to show and the block stays hidden.
function approvalDiff(approval: ApprovalRequest): DiffLine[] {
  const input = approval.tool_input;
  if (!isRecord(input)) return [];
  const patch = input.patch;
  if (typeof patch === "string") {
    return patch
      .split("\n")
      .filter((line) => line.startsWith("-") || line.startsWith("+"))
      .filter((line) => !line.startsWith("---") && !line.startsWith("+++"))
      .map((line) => ({ sign: line[0] as "-" | "+", text: line.slice(1) }));
  }
  const removed = typeof input.old_string === "string" ? input.old_string : undefined;
  const added =
    typeof input.new_string === "string"
      ? input.new_string
      : typeof input.content === "string"
        ? input.content
        : undefined;
  const lines: DiffLine[] = [];
  if (removed !== undefined) {
    for (const text of removed.split("\n")) lines.push({ sign: "-", text });
  }
  if (added !== undefined) {
    for (const text of added.split("\n")) lines.push({ sign: "+", text });
  }
  return lines;
}

function renderApprovalDiff(approval: ApprovalRequest): void {
  const lines = approvalDiff(approval);
  approvalDiffEl.replaceChildren();
  approvalDiffEl.hidden = lines.length === 0;
  if (lines.length === 0) return;
  approvalDiffEl.setAttribute("aria-label", strings.approval.diffLabel);
  for (const line of lines) {
    const element = document.createElement("span");
    element.className = line.sign === "-" ? "diff-line diff-removed" : "diff-line diff-added";
    element.textContent = `${line.sign} ${line.text}`;
    approvalDiffEl.append(element);
  }
}

function renderApproval(): void {
  if (!pendingApproval) {
    showCard(approvalCardEl, false);
    approvalDiffEl.hidden = true;
    approvalAllowEl.disabled = false;
    approvalAlwaysEl.disabled = false;
    approvalDenyEl.disabled = false;
    return;
  }
  showCard(approvalCardEl, true);
  approvalToolEl.textContent =
    pendingApproval.tool_name === PLAN_TOOL
      ? strings.approval.planTool
      : (pendingApproval.tool_name ?? strings.approval.missingTool);
  approvalSummaryEl.textContent = approvalDescription(pendingApproval);
  renderApprovalDiff(pendingApproval);
  approvalAlwaysEl.hidden = !supportsAlways(pendingApproval);
  approvalAllowEl.disabled = resolvingApproval;
  approvalAlwaysEl.disabled = resolvingApproval;
  approvalDenyEl.disabled = resolvingApproval;
}

function resolveApproval(decision: ApprovalDecision): void {
  if (!pendingApproval || resolvingApproval) return;
  const approval = pendingApproval;
  resolvingApproval = true;
  renderApproval();
  void invoke("resolve_approval", {
    approvalId: approval.approval_id,
    decision,
  })
    .then(() => {
      if (pendingApproval?.approval_id === approval.approval_id) pendingApproval = null;
      resolvingApproval = false;
      render();
      resetIdle();
    })
    .catch((error: unknown) => {
      resolvingApproval = false;
      renderApproval();
      showError(strings.approval.failed(decision, String(error)));
    });
}

approvalAllowEl.addEventListener("click", () => resolveApproval("allow"));
approvalAlwaysEl.addEventListener("click", () => resolveApproval("allow_always"));
approvalDenyEl.addEventListener("click", () => resolveApproval("deny"));



/* ------------------------------------------------------------------ */
/* OLED pixel-shift + idle fade                                        */
/*                                                                     */
/* X oscillates -2..+2 (180s period), Y 0..+2 (291.24s period, golden  */
/* ratio), both eased in-out. Paused while idle; idle fades the island */
/* to opacity 0 (600ms transition). Any pointer activity or session    */
/* change cancels idle and restores opacity 1.                         */
/* ------------------------------------------------------------------ */

let idle = false;
let idleTimer = 0;

function driftLoop(now: number): void {
  if (reducedMotion()) {
    islandEl.style.removeProperty("transform");
    return;
  }
  if (!idle) {
    const ex =
      0.5 - 0.5 * Math.cos((2 * Math.PI * (now % PIXEL_PERIOD_X)) / PIXEL_PERIOD_X);
    const ey =
      0.5 - 0.5 * Math.cos((2 * Math.PI * (now % PIXEL_PERIOD_Y)) / PIXEL_PERIOD_Y);
    const x = -2 + 4 * ex; // -2 .. +2
    // Drifts UP only: downward opened a visible gap above the notch. Upward is
    // clipped by the screen edge instead, so burn-in mitigation still happens.
    const y = -2 * ey; // 0 .. -2
    islandEl.style.transform = `translate(${x.toFixed(2)}px, ${y.toFixed(2)}px)`;
  }
  requestAnimationFrame(driftLoop);
}
requestAnimationFrame(driftLoop);
motionQuery.addEventListener("change", () => {
  if (reducedMotion()) islandEl.style.removeProperty("transform");
  else requestAnimationFrame(driftLoop);
});

function paintHidden(): void {
  const hidden = (hideInFullscreen && fullscreenActive) || (hideWhenIdle && agentsIdle);
  islandEl.style.opacity = hidden || idle ? "0" : "1";
  islandEl.style.pointerEvents = hidden ? "none" : "";
}

function trackAgentIdle(list: Session[]): void {
  const working = list.some((session) => (session.attention ?? "working") === "working");
  clearTimeout(agentIdleTimer);
  if (working) {
    if (agentsIdle) {
      agentsIdle = false;
      paintHidden();
    }
    return;
  }
  agentIdleTimer = window.setTimeout(() => {
    agentsIdle = true;
    paintHidden();
  }, AGENT_IDLE_HIDE_MS);
}

export function resetIdle(): void {
  if (idle) {
    idle = false;
    islandEl.style.opacity = "1";
  }
  clearTimeout(idleTimer);
  if (idleFadeEnabled) {
    idleTimer = window.setTimeout(() => {
      idle = true;
      islandEl.style.opacity = "0";
    }, idleMs);
  }
  paintHidden();
}

window.addEventListener("pointermove", resetIdle, { passive: true });
window.addEventListener("pointerdown", resetIdle);

/* ------------------------------------------------------------------ */
/* Boot                                                                */
/* ------------------------------------------------------------------ */

async function refresh(): Promise<void> {
  try {
    const list = await invoke<Session[]>("list_sessions");
    onSessions(Array.isArray(list) ? list : []);
  } catch {
    onSessions([]);
  }
}

void listen<Session[]>("sessions-updated", (event) => {
  onSessions(Array.isArray(event.payload) ? event.payload : []);
}).catch(() => {
  resetIdle();
});

void listen<unknown>("approval-requested", (event) => {
  const approval = parseApproval(event.payload);
  if (!approval) {
    showError(strings.approval.invalid);
    if (admitsExpansion("approval")) showActivity(true);
    return;
  }
  pendingApproval = approval;
  if (admitsExpansion("approval")) showActivity(true);
  render();
  resetIdle();
});

void listen<unknown>("approval-resolved", (event) => {
  const resolution = parseResolution(event.payload);
  if (!resolution) return;
  if (pendingApproval?.approval_id === resolution.approval_id) {
    pendingApproval = null;
    resolvingApproval = false;
    render();
  }
  resetIdle();
});

void listen<unknown>("question-asked", (event) => {
  const question = parseQuestion(event.payload);
  if (!question) {
    showError(strings.question.invalid);
    if (admitsExpansion("question")) showActivity(true);
    return;
  }
  openQuestion(question);
  if (admitsExpansion("question")) showActivity(true);
  render();
  resetIdle();
});

void listen<unknown>("question-resolved", (event) => {
  const resolution = parseQuestionResolution(event.payload);
  if (!resolution) return;
  if (pendingQuestion?.question_id === resolution.question_id) {
    closeQuestion();
    render();
  }
  resetIdle();
});

void listen<unknown>("question-focus", (event) => {
  const questionId = stringField(event.payload, "question_id");
  if (!questionId || pendingQuestion?.question_id !== questionId) return;
  if (!admitsExpansion("question")) return;
  showActivity(true);
  questionCardEl.classList.remove("focused");
  void questionCardEl.offsetWidth;
  questionCardEl.classList.add("focused");
  resetIdle();
});

void listen<PointerState>("island-pointer", (event) => {
  if (event.payload.inside) {
    pointerEntered();
    return;
  }
  pointerLeft();
});

void listen<FocusState>("island-focus", (event) => {
  focusedPid = event.payload.pid;
});

void listen<QuietScenes>("quiet-scenes", (event) => {
  quietScene = event.payload.active === true;
});

void listen<{ fullscreen: boolean }>("island-fullscreen", (event) => {
  fullscreenActive = event.payload.fullscreen;
  paintHidden();
});

void listen<unknown>("island-toggle", () => {
  const wasIdle = idle;
  resetIdle();
  clearTimeout(dwellTimer);
  clearTimeout(autoCollapseTimer);
  if (wasIdle || !expanded) {
    expand();
    return;
  }
  if (pendingApproval !== null || pendingQuestion !== null || launchOpen) return;
  collapse();
});

function paintUpdate(): void {
  headerUpdateEl.hidden = availableUpdate === null;
  if (availableUpdate === null) return;
  const label = strings.header.update(availableUpdate);
  headerUpdateEl.title = label;
  headerUpdateEl.setAttribute("aria-label", label);
}

function paintMute(): void {
  muteWavesEl.toggleAttribute("hidden", quiet);
  muteCrossEl.toggleAttribute("hidden", !quiet);
  const label = quiet ? strings.header.unmute : strings.header.mute;
  headerMuteEl.setAttribute("aria-label", label);
  headerMuteEl.title = label;
}

export function applyConfig(next: Record<string, unknown>): void {
  config = next;
  const island = section(next, "island");
  dwellMs = milliseconds(island, "hover_dwell_ms", dwellMs);
  autoCollapseMs = milliseconds(island, "auto_collapse_ms", autoCollapseMs);
  idleMs = milliseconds(island, "idle_fade_ms", idleMs);
  expandOnHover = island.expand_on_hover !== false;
  collapseOnLeave = island.collapse_on_leave !== false;
  hideInFullscreen = island.hide_in_fullscreen !== false;
  hideWhenIdle = island.hide_when_idle === true;
  idleFadeEnabled = island.idle_fade === true;
  clickToJump = island.click_to_jump !== false;
  smartSuppression = island.smart_suppression !== false;
  const notifications = section(next, "notifications");
  expandOnCompletion = notifications.expand_on_completion !== false;
  expandOnQuestion = notifications.expand_on_question !== false;
  subagentTiming =
    notifications.subagent_timing === "all_finished" ||
    notifications.subagent_timing === "every_completion"
      ? notifications.subagent_timing
      : "root_responses";
  paintHidden();
  quiet = section(next, "sound").quiet === true;
  const display = section(next, "display");
  compactClean = display.compact_layout === "clean";
  notchWidth = milliseconds(display, "notch_width_offset", 0);
  notchHeight = milliseconds(display, "notch_height_offset", 0);
  islandHeight = milliseconds(display, "island_height", 0);
  const baseWidth = milliseconds(display, "panel_max_width", BASE_EXPANDED.w);
  if (baseWidth !== BASE_EXPANDED.w) {
    BASE_EXPANDED.w = baseWidth;
    const scale = uiScale;
    uiScale = 0;
    applyUiScale(scale);
  }
  expandedMaxH = milliseconds(display, "panel_max_height", expandedMaxH);
  document.documentElement.style.setProperty(
    "--content-font",
    `${milliseconds(display, "content_font", 11)}px`,
  );
  document.documentElement.style.setProperty(
    "--transcript-max",
    `${milliseconds(display, "completion_card_height", 90)}px`,
  );
  show = {
    tasks: display.tasks !== false,
    project: display.project !== false,
    worktree: display.worktree !== false,
    agentIcons: display.agent_icons !== false,
    terminalIcons: display.terminal_icons !== false,
    model: display.model !== false,
    effort: display.effort === true,
    activity: display.activity !== false,
    subagents: display.subagents !== false,
  };
  const usageSection = section(next, "usage");
  usageOptions = {
    showLimits: usageSection.show_limits !== false,
    remaining: usageSection.value_mode === "remaining",
    preferred:
      typeof usageSection.preferred_provider === "string"
        ? usageSection.preferred_provider
        : "auto",
    showResetCards: usageSection.show_reset_cards !== false,
    creditDisplay:
      usageSection.codex_credit_display === "dollars" ? "dollars" : "credits",
    warnThreshold:
      typeof usageSection.warn_threshold === "number" ? usageSection.warn_threshold : 90,
  };
  const monitor = typeof display.monitor === "string" ? display.monitor : "";
  void invoke("set_island_monitor", { name: monitor })
    .catch(() => {})
    .finally(() => {
      void resolveUiScale(display.ui_scale);
    });
  paintMute();
  render();
  resetIdle();
}

async function resolveUiScale(override: unknown): Promise<void> {
  let metrics: { scale: number; compact_height: number | null; safe_top?: number; notch_width?: number } = {
    scale: 1,
    compact_height: null,
  };
  try {
    metrics = await invoke<{ scale: number; compact_height: number | null; safe_top?: number; notch_width?: number }>("island_metrics");
  } catch {
    metrics = { scale: 1, compact_height: null };
  }
  const scale = typeof override === "number" && override > 0 ? override : metrics.scale;
  const previousNotch = hasPhysicalNotch;
  physicalNotchHeight = metrics.safe_top ?? 0;
  hasPhysicalNotch = physicalNotchHeight > 0;
  physicalNotchWidth = metrics.notch_width ?? 0;
  document.body.classList.toggle("has-notch", hasPhysicalNotch);
  // A monitor switch can change the shape even when dimensions stay the same.
  if (previousNotch !== hasPhysicalNotch) {
    islandEl.style.backgroundImage = expanded
      ? expandedShape(curSize.w, curSize.h)
      : compactShape(curSize.w, curSize.h);
  }
  applyUiScale(scale, metrics.compact_height ?? undefined);
}

async function loadConfig(): Promise<void> {
  try {
    const payload = await invoke<{ config: Record<string, unknown> }>("get_config");
    if (isRecord(payload.config)) applyConfig(payload.config);
  } catch {
    paintMute();
  }
}

headerSettingsEl.addEventListener("click", () => {
  void invoke("open_settings").catch(() => {});
});

headerUpdateEl.addEventListener("click", () => {
  void invoke("run_update", { prompt: strings.header.updatePrompt }).catch((error: unknown) => {
    showError(strings.header.updateFailed(String(error)));
  });
});

type LaunchAgent = keyof typeof strings.launch.agents;

const LAUNCH_AGENTS: LaunchAgent[] = ["claude", "codex", "opencode"];

function closeNewSession(): void {
  launchOpen = false;
  showCard(newSessionCardEl, false);
  syncExpandedSize();
}

function pickSessionFolder(agent: LaunchAgent): void {
  closeNewSession();
  void invoke("pick_session_folder", {
    agent,
    title: strings.launch.pickFolder(strings.launch.agents[agent]),
    accept: strings.launch.accept,
    cancel: strings.launch.cancel,
  }).catch((error: unknown) => {
    showError(strings.launch.failed(reasonOf(error)));
  });
}

function renderAgentTiles(available: string[]): void {
  newSessionAgentsEl.replaceChildren();
  for (const agent of LAUNCH_AGENTS) {
    const tile = document.createElement("button");
    tile.type = "button";
    tile.className = "approval-button new-session-agent";
    tile.dataset.agent = agent;
    tile.textContent = strings.launch.agents[agent];
    tile.style.setProperty("--agent-bg", `var(--agent-${agent}-bg)`);
    tile.style.setProperty("--agent-fg", `var(--agent-${agent}-fg)`);
    if (!available.includes(agent)) {
      tile.disabled = true;
      tile.title = strings.launch.missing;
    }
    tile.addEventListener("click", () => pickSessionFolder(agent));
    newSessionAgentsEl.append(tile);
  }
}

async function openNewSession(): Promise<void> {
  launchOpen = true;
  const available = await invoke<string[]>("agents_available").catch(() => [] as string[]);
  if (!launchOpen) return;
  renderAgentTiles(available);
  showCard(newSessionCardEl, true);
  syncExpandedSize();
}

headerNewSessionEl.addEventListener("click", () => {
  if (!expanded) expand();
  void openNewSession();
});

newSessionCloseEl.addEventListener("click", closeNewSession);

void listen<{ agent: string; path: string | null }>("session-folder", (event) => {
  if (event.payload.path === null) return;
  void invoke("open_session", { agent: event.payload.agent, folder: event.payload.path }).catch(
    (error: unknown) => {
      showError(strings.launch.failed(reasonOf(error)));
    },
  );
});

async function saveQuiet(next: boolean): Promise<void> {
  const payload = await invoke<{ config: Record<string, unknown> }>("get_config");
  if (!isRecord(payload.config)) return;
  section(payload.config, "sound").quiet = next;
  config = payload.config;
  await invoke("save_config", { config: payload.config });
}

headerMuteEl.addEventListener("click", () => {
  quiet = !quiet;
  section(config, "sound").quiet = quiet;
  paintMute();
  void saveQuiet(quiet).catch(() => {});
});

void listen<{ config: Record<string, unknown> }>("config-changed", (event) => {
  if (isRecord(event.payload?.config)) applyConfig(event.payload.config);
});

void listen<UsageReport>("usage-updated", (event) => {
  if (Array.isArray(event.payload?.providers)) {
    usage = event.payload;
    renderUsage();
  }
});

void listen<{ version: string }>("update-available", (event) => {
  if (typeof event.payload?.version === "string") {
    availableUpdate = event.payload.version;
    paintUpdate();
  }
});

async function loadUpdate(): Promise<void> {
  try {
    const update = await invoke<{ version: string } | null>("get_update");
    availableUpdate = typeof update?.version === "string" ? update.version : null;
  } catch {
    availableUpdate = null;
  }
  paintUpdate();
}

async function loadUsage(): Promise<void> {
  try {
    const report = await invoke<UsageReport>("get_usage");
    if (Array.isArray(report?.providers)) {
      usage = report;
      renderUsage();
    }
  } catch {
    renderUsage();
  }
}

function applyStaticStrings(): void {
  islandEl.setAttribute("aria-label", strings.island.label);
  headerLabelEl.textContent = strings.island.sessions(0);
  headerSettingsEl.setAttribute("aria-label", strings.header.settings);
  headerSettingsEl.title = strings.header.settings;
  headerNewSessionEl.setAttribute("aria-label", strings.header.newSession);
  headerNewSessionEl.title = strings.header.newSession;
  newSessionKickerEl.textContent = strings.launch.kicker;
  newSessionCloseEl.textContent = strings.launch.close;
  paintMute();
  paintUpdate();
  approvalCardEl.setAttribute("aria-label", strings.approval.label);
  approvalActionsEl.setAttribute("aria-label", strings.approval.actionsLabel);
  approvalKickerEl.textContent = strings.approval.kicker;
  approvalAllowEl.textContent = strings.approval.allow;
  approvalDenyEl.textContent = strings.approval.deny;
  questionCardEl.setAttribute("aria-label", strings.question.label);
}

async function boot(): Promise<void> {
  applyStaticStrings();
  await setIslandSize(COMPACT.w, COMPACT.h);
  setView("compact");
  resetIdle();
  void loadConfig();
  void loadUsage();
  void loadUpdate();
  void refresh();
}

void boot();

void listen("island-screen-changed", () => { void loadConfig(); });
