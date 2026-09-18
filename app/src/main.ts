import "./styles.css";
import { FrameLoop } from "./frame-loop";
import { RenderFrame } from "./render-frame";
import { CompletionEdges, SubagentEdges } from "./activity-edges";
import { UsageLane, type UsagePart } from "./usage-lane";
import { StateStrip } from "./strip";
import { IslandWindow, NativeResize } from "./island-window";
import { snapshotSessions, snapshotApprovals, snapshotQuestions, pendingKey, detachedDeliveries } from "./island-state";
import { DeliveryView } from "./message-deliveries";
import { bindRecovery, RecoveryView, type RecoveryRecord } from "./message-recovery";
import { initializeVoice, voice } from "./voice";
import { bindDaemonState, type ActionIdentity, type Delivery, type UiCache } from "./daemon-state";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { bindIslandKeyboard } from "./island-keyboard";
import { ActivityController } from "./activity-controller";
import { strings } from "./strings";
import {
  ApprovalDecision,
  ApprovalRequest,
  DiffLine,
  FocusState,
  PointerState,
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
  headerNeedEl,
  headerStripEl,
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
import { initializeRows, renderCompact, renderList, tickElapsed, type CompactPending } from "./row";
import {
  initializeQuestions,
  setQuestionConnection,
  replaceQuestions,
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
export const LEAVE_MS = 200;


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
let screenOff = false;
let daemonConnected = false;
let configPaintPending = false;
let resetScalePending = false;
let configRevision = 0;
let metricsRevision = 0;
let activityPending = false;
let focusedPid: number | null = null;
const completionEdges = new CompletionEdges();
const subagentEdges = new SubagentEdges();
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
let forceRowFill = false;
let macosPanel = false;
const islandKeyboard = bindIslandKeyboard(
  islandEl,
  () => expanded,
  (active) => invoke("island_keyboard", { active }),
);
export let sessions: Session[] = [];
let childSessions: Session[] = [];
let pendingApproval: ApprovalRequest | null = null;
let displayedApprovalKey = "";
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

function setHidden(element: HTMLElement, hidden: boolean): void {
  if (element.hidden !== hidden) element.hidden = hidden;
}
initializeRows({
  get compactClean(): boolean { return compactClean; },
  get expanded(): boolean { return expanded; },
  get visible(): boolean { return visualVisible(); },
  get sessions(): Session[] { return sessions; },
  get show(): RowVisibility { return show; },
  get forceFill(): boolean { return forceRowFill; },
  LEAVE_MS, jumpTo, reducedMotion, render, syncExpandedSize, showError,
  canUseTarget,
});
initializeQuestions({ childLabel, jumpToId, render, resetIdle, showCard, showError });

/* ------------------------------------------------------------------ */
/* Window morph                                                        */
/* ------------------------------------------------------------------ */

const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");

export function reducedMotion(): boolean {
  return motionQuery.matches;
}

const CARD_LEAVE_MS = 110;

export function showCard(card: HTMLElement, visible: boolean, onHidden?: () => void): void {
  if (visible) {
    if (card.classList.contains("is-leaving")) card.classList.remove("is-leaving");
    if (card.hidden) {
      card.hidden = false;
      card.classList.remove("is-entering");
      void card.offsetWidth;
      card.classList.add("is-entering");
    }
    return;
  }
  const focused = document.activeElement;
  if (focused instanceof HTMLElement && card.contains(focused)) focused.blur();
  if (card.hidden || card.classList.contains("is-leaving")) return;
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
    ? Math.max(MIN_COMPACT_H, (fixed ? islandHeight : physicalNotchHeight) + notchHeight)
    : base + Math.max(floor, notchHeight);
  const width = hasPhysicalNotch
    ? Math.ceil(physicalNotchWidth + Math.max(100 * next, (compactClean ? 112 : 208) * next + notchWidth))
    : Math.max(MIN_COMPACT_W, Math.round(BASE_COMPACT.w * next) + notchWidth);
  document.documentElement.style.setProperty("--camera-top", `${physicalNotchHeight / next}px`);
  document.documentElement.style.setProperty("--camera-width", `${physicalNotchWidth / next}px`);
  document.documentElement.style.setProperty("--compact-height", `${height / next}px`);
  if (next === uiScale && height === COMPACT.h && width === COMPACT.w) return;
  uiScale = next;
  COMPACT = { w: width, h: height };
  EXPANDED = { w: Math.max(Math.round(BASE_EXPANDED.w * next), hasPhysicalNotch ? Math.ceil(physicalNotchWidth + 320 * next) : 0), h: Math.round(BASE_EXPANDED.h * next) };
  islandEl.style.zoom = String(next);
  islandEl.style.setProperty("--badge-icon", `${Math.round(BADGE_ICON * next)}px`);
  if (expanded) {
    islandEl.style.backgroundImage = expandedShape(EXPANDED.w, windowMorph.current.h);
    syncExpandedSize();
  } else {
    islandEl.style.backgroundImage = compactShape(COMPACT.w, COMPACT.h);
    windowMorph.snap(COMPACT);
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

function setMorph(h: number, morphFrom: Size, morphGoal: Size): void {
  if (Math.min(morphFrom.h, morphGoal.h) > COMPACT.h) {
    islandEl.style.setProperty("--morph", "1");
    return;
  }
  const span = Math.max(1, Math.max(morphFrom.h, morphGoal.h) - COMPACT.h);
  const open = Math.min(1, Math.max(0, (h - COMPACT.h) / span));
  islandEl.style.setProperty("--morph", open.toFixed(3));
}

const windowMorph = new IslandWindow(COMPACT,
  new NativeResize((size) => invoke<void>("set_island_size", { width: size.w, height: size.h })),
  reducedMotion, setMorph);

function setIslandSize(w: number, h: number): Promise<void> {
  return windowMorph.resize.request({ w, h });
}

export function morphTo(target: Size, onStart: () => void, onEnd: () => void): void {
  windowMorph.morph(target, onStart, onEnd);
}

function noop(): void {}

function setView(which: "compact" | "expanded"): void {
  const compactActive = which === "compact";
  if (compactActive) islandEl.style.backgroundImage = compactShape(COMPACT.w, COMPACT.h);
  compactViewEl.classList.toggle("visible", compactActive);
  expandedViewEl.classList.toggle("visible", !compactActive);
  compactViewEl.setAttribute("aria-hidden", String(!compactActive));
  compactViewEl.inert = !compactActive;
  expandedViewEl.inert = compactActive;
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
  const supportHeight = Math.min(supportEl.scrollHeight, maxExpandedHeight() * 55 / (100 * uiScale));
  const needed = Math.ceil(Math.max(listBottom + supportHeight, cardBottom) * uiScale);
  const staying = sessionListEl.querySelectorAll("li:not(.is-leaving)").length;
  const floor = staying === 0 ? EXPANDED.h : COMPACT.h;
  return {
    w: EXPANDED.w,
    h: Math.min(maxExpandedHeight(), Math.max(floor, needed)),
  };
}

function expand(interactive = false): void {
  forceRowFill = true;
  renderFrame.flush();
  renderList();
  forceRowFill = false;
  expanded = true;
  if (interactive && !macosPanel) islandKeyboard.activate();
  // Swap SVG + content at tween START (expanding).
  islandEl.classList.add("expanded");
  setView("expanded");
  paintHidden();
  const target = expandedSize();
  islandEl.style.backgroundImage = expandedShape(target.w, target.h);
  morphTo(target, noop, noop);
}

function collapse(): void {
  if (voice.active()) return;
  expanded = false;
  paintHidden();
  activity.collapsed();
  islandKeyboard.release();
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
  if (target.w === windowMorph.goal.w && target.h === windowMorph.goal.h) {
    if (windowMorph.resize.needsRetry()) void windowMorph.resize.request(target);
    return;
  }
  islandEl.style.backgroundImage = expandedShape(target.w, target.h);
  morphTo(target, noop, noop);
}

/* ------------------------------------------------------------------ */
/* Hover / automatic activity                                          */
/* ------------------------------------------------------------------ */

function islandHasEditor(): boolean {
  const active = document.activeElement;
  return active instanceof HTMLElement && islandEl.contains(active) &&
    active.matches('textarea:not(:disabled), input:not(:disabled):not([type="hidden"]), select:not(:disabled), [contenteditable="true"]');
}

function islandHasInteractiveFocus(): boolean {
  const active = document.activeElement;
  return active instanceof HTMLElement && islandEl.contains(active) && active.matches(
    'button:not(:disabled), a[href], [tabindex]:not([tabindex="-1"]), textarea:not(:disabled), input:not(:disabled):not([type="hidden"]), select:not(:disabled), [contenteditable="true"]',
  );
}

const activity = new ActivityController({
  clock: window,
  open: () => expand(false),
  collapse,
  isExpanded: () => expanded,
  isEditing: () => launchOpen || voice.active() || islandHasEditor(),
  expandOnHover: () => expandOnHover,
  collapseOnLeave: () => collapseOnLeave,
  dwellMs: () => dwellMs,
  autoCollapseMs: () => autoCollapseMs,
});

islandEl.addEventListener("mouseenter", () => activity.domEntered());
islandEl.addEventListener("mouseleave", () => activity.domLeft());
islandEl.addEventListener("pointerdown", () => activity.interaction());
islandEl.addEventListener("pointerup", () => activity.interactionEnded());
islandEl.addEventListener("pointercancel", () => activity.interactionEnded());
window.addEventListener("blur", () => activity.interactionEnded());
islandEl.addEventListener("pointermove", (event) => activity.domMoved(event.screenX, event.screenY));
islandEl.addEventListener("focusin", () => { if (islandHasInteractiveFocus()) activity.editorFocused(); });
islandEl.addEventListener("focusout", () => queueMicrotask(() => {
  if (!islandHasInteractiveFocus()) activity.editorRemoved();
}));
new window.MutationObserver(() => {
  if (!islandHasInteractiveFocus()) activity.editorRemoved();
}).observe(islandEl, { childList: true, subtree: true });

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
  return subagentEdges.observe(list, subagentTiming);
}

function onSessions(list: Session[], hydrate = false): void {
  sessions = list;
  trackAgentIdle(list);
  const subagentDone = subagentEdges.observe(list, subagentTiming, hydrate);
  const completion = completionEdges.observe(list, hydrate);
  render();
  const suppressed = smartSuppression && terminalIsFocused(list);
  if ((completion || subagentDone) && admitsExpansion("completion") && !suppressed) {
    activityPending = true;
  }
  resetIdle();
}

const renderFrame = new RenderFrame(paint);
export function render(): void { renderFrame.invalidate(); }
function paint(): void {
  if (configPaintPending) { configPaintPending = false; paintConfig(); }
  const n = sessions.length;
  paintMute();
  paintUpdate();
  detachedView.update(detachedRecords, daemonConnected);
  if (connectionStatusEl.hidden !== connectionStatusHidden) connectionStatusEl.hidden = connectionStatusHidden;
  if (connectionStatusEl.textContent !== connectionLabel) connectionStatusEl.textContent = connectionLabel;
  const pending: CompactPending | null = pendingApproval !== null
    ? { kind: "approval", sessionId: pendingApproval.session_id }
    : pendingQuestion !== null ? { kind: "question", sessionId: pendingQuestion.session_id } : null;
  renderCompact(n, pending, !daemonConnected);
  headerStrip.update(sessions, !daemonConnected);
  const headerLabel = daemonConnected ? strings.island.sessions(n) : strings.island.noConnection;
  if (headerLabelEl.textContent !== headerLabel) headerLabelEl.textContent = headerLabel;
  paintHeaderNeed();
  renderUsage();
  renderList();
  renderApproval();
  renderQuestion();
  syncExpandedSize();
  if (activityPending) { activityPending = false; activity.relevantEvent(); }
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

function usageWindow(key: string, name: string, percent: number, resetsAt: number | undefined, now: number): UsagePart {
  const remaining = resetsAt === undefined ? 0 : resetsAt - now;
  return { key, label: name, value: strings.usage.percent(usageValue(percent)), percent: usageValue(percent), severity: usageSeverity(percent),
    ...(remaining > 0 ? { reset: strings.usage.resetIn(remaining) } : {}) };
}

function creditText(credits: { balance: number; unlimited: boolean }): string | null {
  if (credits.unlimited) return strings.usage.creditsUnlimited;
  if (credits.balance <= 0) return null;
  return usageOptions.creditDisplay === "dollars"
    ? strings.usage.dollars(credits.balance)
    : strings.usage.credits(credits.balance);
}

const usageLane = new UsageLane(headerUsageEl);
const modelLane = new UsageLane(headerUsageModelEl);
const headerStrip = new StateStrip(headerStripEl);
function paintHeaderNeed(): void {
  const waiting = sessions.filter((session): boolean => session.attention === "waiting_for_input").length;
  const finished = sessions.filter((session): boolean => session.attention === "needs_attention").length;
  const kind = !daemonConnected ? "" : waiting > 0 ? "waiting_for_input" : finished > 0 ? "needs_attention" : "";
  const label = kind === "waiting_for_input" ? strings.island.waiting(waiting) : kind === "needs_attention" ? strings.island.finished(finished) : "";
  const className = `header-need pixel ${kind}`.trim();
  if (headerNeedEl.className !== className) headerNeedEl.className = className;
  if (headerNeedEl.textContent !== label) headerNeedEl.textContent = label;
  setHidden(headerNeedEl, label === "");
}
function renderUsage(): void {
  const snapshot = usageOptions.showLimits ? usageProvider() : null;
  if (snapshot === null) { usageLane.update([], false); modelLane.update([], false); return; }
  const now = Date.now();
  const stale = now - snapshot.fetched_at_ms > USAGE_STALE_AFTER_MS;
  const windows = snapshot.windows.map((window): UsagePart => usageWindow(JSON.stringify([snapshot.provider, window.key]), window.label, window.percent, window.resets_at_ms, now));
  if (stale) windows.push({ key: "stale", label: strings.usage.stale, marker: true });
  usageLane.update(windows, stale);
  const parts: UsagePart[] = [];
  const codex = usage.providers.find((provider) => provider.provider === "codex")?.snapshot;
  const cards = usageOptions.showResetCards ? (codex?.reset_cards ?? []) : [];
  if (cards.length > 0) parts.push({ key: "cards", label: strings.usage.resetCards(cards.length) });
  const credits = codex?.credits === undefined ? null : creditText(codex.credits);
  if (credits !== null) parts.push({ key: "credits", label: credits });
  const model = snapshot.models?.[0];
  if (model !== undefined) parts.push(usageWindow(JSON.stringify([snapshot.provider, model.model]), model.model, model.percent, model.resets_at_ms, now));
  modelLane.update(parts, stale);
}


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
  if (!clickToJump || !session.action_identity || !canUseTarget(session.id, session.action_identity)) return;
  jumpErrorEl.hidden = true;
  clearTimeout(errTimer);

  row.classList.remove("flash");
  void row.offsetWidth; // restart animation on repeated clicks
  row.classList.add("flash");

  invoke("jump_v2", { id: session.id, identity: session.action_identity }).catch((err: unknown) => {
    showError(strings.jump.failed(session.title, String(err)));
  });
}

function canUseTarget(id: string, identity: ActionIdentity, writing = false): boolean {
  const current = [...sessions, ...childSessions].find((session): boolean => session.id === id);
  return daemonConnected && (!writing || current?.send_blocked === undefined) && current?.action_identity?.daemon_epoch === identity.daemon_epoch &&
    current.action_identity.session_instance_id === identity.session_instance_id;
}

export function familySession(id: string): Session | undefined {
  return sessions.find((entry) => entry.id === id || (entry.agent === "opencode" &&
    entry.subagents?.some((child) => `opencode:${child.id}` === id)));
}

export function childLabel(id: string): string {
  const parent = familySession(id);
  if (!parent || parent.id === id) return "";
  const child = parent.subagents?.find((child) => `opencode:${child.id}` === id);
  return child?.description ?? id;
}

export function jumpToId(id: string, expected?: ActionIdentity): void {
  if (!id) return;
  const session = [...sessions, ...childSessions].find((entry) => entry.id === id);
  if (!daemonConnected || !session?.action_identity) return;
  if (expected && (expected.daemon_epoch !== session.action_identity.daemon_epoch || expected.session_instance_id !== session.action_identity.session_instance_id)) return;
  invoke("jump_v2", { id: session.id, identity: expected ?? session.action_identity }).catch((err: unknown) => {
    showError(strings.jump.failed(session?.title ?? id, String(err)));
  });
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
  const key = JSON.stringify(lines);
  if (approvalDiffEl.dataset.renderKey === key) return;
  approvalDiffEl.dataset.renderKey = key;
  approvalDiffEl.replaceChildren();
  setHidden(approvalDiffEl, lines.length === 0);
  if (lines.length === 0) return;
  if (approvalDiffEl.getAttribute("aria-label") !== strings.approval.diffLabel) {
    approvalDiffEl.setAttribute("aria-label", strings.approval.diffLabel);
  }
  for (const line of lines) {
    const element = document.createElement("span");
    element.className = line.sign === "-" ? "diff-line diff-removed" : "diff-line diff-added";
    element.textContent = `${line.sign} ${line.text}`;
    approvalDiffEl.append(element);
  }
}

const approvalQueue: ApprovalRequest[] = [];
function closeApproval(id: string): void {
  const index = approvalQueue.findIndex((entry) => entry.approval_id === id);
  if (index >= 0) approvalQueue.splice(index, 1);
  if (pendingApproval?.approval_id === id) {
    pendingApproval = approvalQueue.shift() ?? null;
    resolvingApproval = false;
  }
}

function renderApproval(): void {
  displayedApprovalKey = pendingKey(pendingApproval);
  if (!pendingApproval) {
    showCard(approvalCardEl, false);
    setHidden(approvalDiffEl, true);
    if (approvalAllowEl.disabled) approvalAllowEl.disabled = false;
    if (approvalAlwaysEl.disabled) approvalAlwaysEl.disabled = false;
    if (approvalDenyEl.disabled) approvalDenyEl.disabled = false;
    return;
  }
  showCard(approvalCardEl, true);
  approvalToolEl.textContent =
    pendingApproval.tool_name === PLAN_TOOL
      ? strings.approval.planTool
      : (pendingApproval.tool_name ?? strings.approval.missingTool);
  const owner = childLabel(pendingApproval.session_id);
  approvalSummaryEl.textContent = (owner ? `${owner} · ` : "") + approvalDescription(pendingApproval);
  renderApprovalDiff(pendingApproval);
  setHidden(approvalAlwaysEl, !supportsAlways(pendingApproval));
  const disabled = resolvingApproval || !daemonConnected || !pendingApproval.action_identity || pendingApproval.pending_generation === undefined;
  if (approvalAllowEl.disabled !== disabled) approvalAllowEl.disabled = disabled;
  if (approvalAlwaysEl.disabled !== disabled) approvalAlwaysEl.disabled = disabled;
  if (approvalDenyEl.disabled !== disabled) approvalDenyEl.disabled = disabled;
}

function resolveApproval(decision: ApprovalDecision): void {
  if (!pendingApproval || resolvingApproval) return;
  if (displayedApprovalKey !== pendingKey(pendingApproval)) return;
  const approval = pendingApproval;
  if (!daemonConnected || !approval.action_identity || approval.pending_generation === undefined) return;
  resolvingApproval = true;
  renderApproval();
  void invoke("resolve_approval_v2", {
    approvalId: approval.approval_id,
    decision,
    identity: approval.action_identity, pendingGeneration: approval.pending_generation,
  })
    .then(() => {
      if (pendingKey(pendingApproval) === pendingKey(approval)) closeApproval(approval.approval_id);
      render();
      resetIdle();
    })
    .catch((error: unknown) => {
      if (pendingKey(pendingApproval) !== pendingKey(approval)) return;
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

function visualVisible(): boolean {
  return !document.hidden && !(daemonConnected && screenOff) && !idle && !(hideInFullscreen && fullscreenActive) && !(hideWhenIdle && agentsIdle);
}
const DRIFT_TICK_MS = 250;
const drift = new FrameLoop((now): void => {
  const ex = 0.5 - 0.5 * Math.cos((2 * Math.PI * (now % PIXEL_PERIOD_X)) / PIXEL_PERIOD_X);
  const ey = 0.5 - 0.5 * Math.cos((2 * Math.PI * (now % PIXEL_PERIOD_Y)) / PIXEL_PERIOD_Y);
  const transform = `translate(${(-2 + 4 * ex).toFixed(2)}px, ${(-2 * ey).toFixed(2)}px)`;
  if (islandEl.style.transform !== transform) islandEl.style.transform = transform;
}, {
  request: (callback): number => window.setTimeout((): void => callback(performance.now()), DRIFT_TICK_MS),
  cancel: (id): void => window.clearTimeout(id),
});
export function driftActive(): boolean {
  return drift.isEnabled();
}
const elapsedWork = new FrameLoop(tickElapsed, {
  request: (callback): number => window.setTimeout((): void => callback(performance.now()), 1000),
  cancel: (id): void => window.clearTimeout(id),
});
motionQuery.addEventListener("change", (): void => {
  if (reducedMotion()) islandEl.style.removeProperty("transform");
  paintHidden();
});
document.addEventListener("visibilitychange", paintHidden);
let elapsedVisible = false;
paintHidden();

function paintHidden(): void {
  drift.setEnabled(visualVisible() && !reducedMotion());
  const nextElapsedVisible = visualVisible() && expanded;
  if (nextElapsedVisible && !elapsedVisible) tickElapsed();
  elapsedVisible = nextElapsedVisible;
  elapsedWork.setEnabled(nextElapsedVisible);
  const hidden = (hideInFullscreen && fullscreenActive) || (hideWhenIdle && agentsIdle);
  const opacity = hidden || idle ? "0" : "1";
  const pointerEvents = hidden ? "none" : "";
  if (islandEl.style.opacity !== opacity) islandEl.style.opacity = opacity;
  if (islandEl.style.pointerEvents !== pointerEvents) islandEl.style.pointerEvents = pointerEvents;
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
      paintHidden();
    }, idleMs);
  }
  paintHidden();
}

window.addEventListener("pointermove", resetIdle, { passive: true });
window.addEventListener("pointerdown", resetIdle);

/* ------------------------------------------------------------------ */
/* Boot                                                                */
/* ------------------------------------------------------------------ */

let authoritativeApprovalKeys = new Set<string>();
let authoritativeQuestionKeys = new Set<string>();
const recoveryEl = document.createElement("section");
const voiceEl = document.createElement("section");
const supportEl = document.createElement("div");
supportEl.className = "island-support";
expandedViewEl.append(supportEl);
supportEl.append(voiceEl);
initializeVoice(voiceEl, { resize: (): void => { if (voice.active() && !expanded) expand(false); else syncExpandedSize(); }, error: showError, isMac: () => macosPanel });
supportEl.append(recoveryEl);
const detachedEl = document.createElement("section");
const detachedTitle = document.createElement("h3"); detachedTitle.textContent = strings.session.deliveryDetached;
const detachedList = document.createElement("ul"); detachedEl.append(detachedTitle, detachedList); detachedEl.hidden = true;
supportEl.append(detachedEl);
let detachedRecords: Delivery[] = [];
const detachedView = new DeliveryView(detachedList, {
  showOrigin: true,
  cancel: (record): Promise<unknown> => {
    if (!daemonConnected || !record.identity || !detachedRecords.some((entry): boolean => entry.session_id === record.session_id && entry.message_id === record.message_id && entry.identity?.daemon_epoch === record.identity?.daemon_epoch && entry.identity?.session_instance_id === record.identity?.session_instance_id)) return Promise.reject(new Error("stale_session"));
    return invoke("cancel_message_v2", { id: record.session_id, messageId: record.message_id, identity: record.identity });
  },
  copy: (text): Promise<void> => navigator.clipboard.writeText(text), error: showError,
  changed: (): void => { detachedEl.hidden = detachedList.hidden; syncExpandedSize(); },
});
const recoveryView = new RecoveryView(recoveryEl, {
  copy: (text): Promise<void> => navigator.clipboard.writeText(text),
  discard: (id): Promise<boolean> => invoke<boolean>("discard_message_recovery", { id }),
  error: showError,
  changed: syncExpandedSize,
});
void bindRecovery({
  listen: (callback): Promise<() => void> => listen<RecoveryRecord[]>("message-recovery", (event): void => callback(event.payload)),
  read: (): Promise<RecoveryRecord[]> => invoke<RecoveryRecord[]>("get_message_recovery"),
}, recoveryView, (): void => { showError(strings.recovery.loadFailed); }).catch((): void => { showError(strings.recovery.loadFailed); });

const connectionStatusEl = document.createElement("div");
let connectionLabel: string = strings.connection.connecting;
let connectionStatusHidden = false;
connectionStatusEl.className = "daemon-connection-status";
connectionStatusEl.setAttribute("role", "status");
connectionStatusEl.setAttribute("aria-live", "polite");
connectionStatusEl.textContent = strings.connection.connecting;
supportEl.append(connectionStatusEl);
function connectionFailed(): void {
  daemonConnected = false;
  activityPending = false;
  paintHidden();
  setQuestionConnection(false);
  sessions = sessions.map((session): Session => ({ ...session, send_blocked: "daemon_unavailable" }));
  render();
  connectionStatusHidden = false;
  connectionLabel = strings.connection.reconnecting;
}
void bindDaemonState({
  listen: (callback): Promise<() => void> => listen<UiCache>("daemon-ui-state", (event): void => callback(event.payload)),
  read: (): Promise<UiCache> => invoke<UiCache>("get_daemon_ui_state"),
}, (update): void => {
  daemonConnected = update.phase === "connected";
  if (!daemonConnected || update.hydrate) activityPending = false;
  if (daemonConnected && update.snapshot) screenOff = update.snapshot.quiet_scenes.screen_off;
  paintHidden();
  if (update.snapshot) detachedRecords = detachedDeliveries(update.snapshot);
  setQuestionConnection(daemonConnected);
  const discovering = update.phase === "connected" && update.snapshot?.discovering === true;
  connectionStatusHidden = update.phase === "connected" && !discovering;
  connectionLabel = discovering ? strings.connection.discovering : strings.connection[update.phase];
  if (update.phase !== "connected") {
    sessions = sessions.map((session): Session => ({ ...session, send_blocked: "daemon_unavailable" }));
    render();
  }
  if (update.phase === "connected" && update.snapshot) {
    const snapshot = update.snapshot;
    const approvals = snapshotApprovals(snapshot);
    const questions = snapshotQuestions(snapshot);
    const newApproval = approvals.some((approval): boolean => !authoritativeApprovalKeys.has(pendingKey(approval)));
    const newQuestion = questions.some((question): boolean => !authoritativeQuestionKeys.has(pendingKey(question)));
    authoritativeApprovalKeys = new Set(approvals.map(pendingKey));
    authoritativeQuestionKeys = new Set(questions.map(pendingKey));
    const next = approvals.find((approval): boolean => pendingKey(approval) === pendingKey(pendingApproval)) ?? approvals[0] ?? null;
    if (pendingKey(next) !== pendingKey(pendingApproval)) resolvingApproval = false;
    pendingApproval = next;
    approvalQueue.splice(0, approvalQueue.length, ...approvals.filter((approval): boolean => approval !== next));
    replaceQuestions(questions);
    const configKey = JSON.stringify(snapshot.config);
    if (update.hydrate || configKey !== JSON.stringify(config)) {
      applyConfig(snapshot.config);
    }
    usage = snapshot.usage;
    availableUpdate = snapshot.update?.version ?? null;
    quietScene = snapshot.quiet_scenes.active;
    childSessions = snapshotSessions(snapshot, true);
    onSessions(snapshotSessions(snapshot), update.hydrate);
    if (!update.hydrate && ((newApproval && admitsExpansion("approval")) || (newQuestion && admitsExpansion("question")))) activityPending = true;
  }

}, connectionFailed).catch(connectionFailed);

void listen<unknown>("question-focus", (event) => {
  const questionId = stringField(event.payload, "question_id");
  if (!questionId || pendingQuestion?.question_id !== questionId) return;
  questionCardEl.classList.remove("focused");
  void questionCardEl.offsetWidth;
  questionCardEl.classList.add("focused");
  resetIdle();
});

void listen<PointerState>("island-pointer", (event) => {
  activity.nativePointer(event.payload);
});

void listen<FocusState>("island-focus", (event) => {
  focusedPid = event.payload.pid;
});

void listen<{ fullscreen: boolean }>("island-fullscreen", (event) => {
  fullscreenActive = event.payload.fullscreen;
  paintHidden();
});

void listen<unknown>("island-toggle", () => {
  const wasIdle = idle;
  resetIdle();
  if (wasIdle || !expanded) {
    activity.manualOpened();
    expand(true);
    return;
  }
  if (launchOpen) return;
  collapse();
});

function paintUpdate(): void {
  setHidden(headerUpdateEl, availableUpdate === null);
  if (availableUpdate === null) return;
  const label = strings.header.update(availableUpdate);
  if (headerUpdateEl.title !== label) headerUpdateEl.title = label;
  if (headerUpdateEl.getAttribute("aria-label") !== label) headerUpdateEl.setAttribute("aria-label", label);
}

function paintMute(): void {
  const disabled = !daemonConnected;
  if (headerMuteEl.disabled !== disabled) headerMuteEl.disabled = disabled;
  muteWavesEl.toggleAttribute("hidden", quiet);
  muteCrossEl.toggleAttribute("hidden", !quiet);
  const label = quiet ? strings.header.unmute : strings.header.mute;
  if (headerMuteEl.getAttribute("aria-label") !== label) headerMuteEl.setAttribute("aria-label", label);
  if (headerMuteEl.title !== label) headerMuteEl.title = label;
}

export function applyConfig(next: Record<string, unknown>): void {
  config = next;
  configRevision += 1;
  configPaintPending = true;
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
    resetScalePending = true;
  }
  expandedMaxH = milliseconds(display, "panel_max_height", expandedMaxH);
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
  render();
  resetIdle();
}

function paintConfig(): void {
  const display = section(config, "display");
  const revision = configRevision;
  if (resetScalePending) {
    resetScalePending = false;
    const scale = uiScale; uiScale = 0; applyUiScale(scale);
  }
  const style = document.documentElement.style;
  for (const [key, value] of [
    ["--content-font", `${milliseconds(display, "content_font", 11)}px`],
    ["--transcript-max", `${milliseconds(display, "completion_card_height", 90)}px`],
  ]) if (style.getPropertyValue(key) !== value) style.setProperty(key, value);
  const monitor = typeof display.monitor === "string" ? display.monitor : "";
  void invoke("set_island_monitor", { name: monitor })
    .catch(() => {})
    .finally(() => {
      if (revision === configRevision) void resolveUiScale(display.ui_scale);
    });
}

async function resolveUiScale(override: unknown): Promise<void> {
  const revision = ++metricsRevision;
  const configuration = configRevision;
  let metrics: { scale: number; compact_height: number | null; safe_top?: number; notch_width?: number } = {
    scale: 1,
    compact_height: null,
  };
  try {
    metrics = await invoke<{ scale: number; compact_height: number | null; safe_top?: number; notch_width?: number }>("island_metrics");
  } catch {
    metrics = { scale: 1, compact_height: null };
  }
  if (revision !== metricsRevision || configuration !== configRevision) return;
  const scale = typeof override === "number" && override > 0 ? override : metrics.scale;
  const previousNotch = hasPhysicalNotch;
  physicalNotchHeight = metrics.safe_top ?? 0;
  hasPhysicalNotch = physicalNotchHeight > 0;
  physicalNotchWidth = metrics.notch_width ?? 0;
  document.body.classList.toggle("has-notch", hasPhysicalNotch);
  // A monitor switch can change the shape even when dimensions stay the same.
  if (previousNotch !== hasPhysicalNotch) {
    islandEl.style.backgroundImage = expanded
      ? expandedShape(windowMorph.current.w, windowMorph.current.h)
      : compactShape(windowMorph.current.w, windowMorph.current.h);
  }
  applyUiScale(scale, metrics.compact_height ?? undefined);
}

headerSettingsEl.addEventListener("click", () => {
  void invoke("open_settings").catch(() => {});
});

void listen<string>("update-progress", (event) => {
  if (!headerUpdateEl.disabled) return;
  headerUpdateEl.title = event.payload;
  headerUpdateEl.setAttribute("aria-label", event.payload);
});

headerUpdateEl.addEventListener("click", () => {
  if (headerUpdateEl.disabled) return;
  headerUpdateEl.disabled = true;
  headerUpdateEl.setAttribute("aria-busy", "true");
  void invoke("run_update", { prompt: strings.header.updatePrompt }).catch((error: unknown) => {
    showError(document.body.classList.contains("platform-macos")
      ? `Falha ao atualizar: ${String(error)}`
      : strings.header.updateFailed(String(error)));
  }).finally(() => {
    headerUpdateEl.disabled = false;
    headerUpdateEl.removeAttribute("aria-busy");
    paintUpdate();
  });
});

type LaunchAgent = keyof typeof strings.launch.agents;

const LAUNCH_AGENTS: LaunchAgent[] = ["claude", "codex", "opencode"];

function closeNewSession(): void {
  launchOpen = false;
  showCard(newSessionCardEl, false);
  syncExpandedSize();
  activity.editingEnded();
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
  activity.manualOpened();
  if (!expanded) expand(true);
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
  if (!daemonConnected) return;
  quiet = !quiet;
  section(config, "sound").quiet = quiet;
  paintMute();
  void saveQuiet(quiet).catch(() => {});
});

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
  void invoke<{ os: string }>("platform_capabilities").then((platform) => {
    macosPanel = platform?.os === "macos";
    document.body.classList.toggle("platform-macos", platform?.os === "macos");
  }).catch(() => {});
  await setIslandSize(COMPACT.w, COMPACT.h);
  if (!expanded) setView("compact");
  resetIdle();
  void resolveUiScale(section(config, "display").ui_scale);
}

void boot();

void listen("island-screen-changed", () => { void resolveUiScale(section(config, "display").ui_scale); });
