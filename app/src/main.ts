import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { strings } from "./strings";
import { SESSION_ICONS } from "./session-icons";
import { createSprite, spriteAgent } from "./sprites";

type TerminalKind = "kitty" | "alacritty" | "unknown" | "wezterm" | "ghostty" | "zed" | "code" | "cursor" | "windsurf" | "codium";

interface QueuedMessage {
  id: number;
  text: string;
  queued_at_ms: number;
}

interface Task {
  content: string;
  status: "pending" | "in_progress" | "completed" | "cancelled";
}

interface Session {
  id: string;
  agent: string;
  cwd: string;
  title: string;
  pid: number;
  terminal: TerminalKind;
  hook_id?: string;
  status?: string;
  current_tool?: string;
  summary?: string;
  last_message?: string;
  last_message_body?: string;
  tasks?: Task[];
  mode?: string;
  subagents?: Subagent[];
  permission_state?: "unknown" | "pending" | "allowed" | "denied";
  question_state?: "pending" | "answered" | "expired";
  attention?: Attention;
  queued_messages?: QueuedMessage[];
  send_channel?: string;
  send_blocked?: string;
  name?: string;
  branch?: string;
  model?: string;
  effort?: string;
  since_ms?: number;
  raise_pid?: number;
  launcher?: string;
}

type Attention = "waiting_for_input" | "needs_attention" | "working" | "idle";
type SubagentTiming = "root_responses" | "all_finished" | "every_completion";

interface PointerState {
  inside: boolean;
}

interface FocusState {
  pid: number | null;
}

interface QuietScenes {
  active: boolean;
}

type ApprovalDecision = "allow" | "deny" | "allow_always";

interface Subagent {
  id: string;
  kind: string;
  description?: string;
  tool?: string;
  summary?: string;
  since_ms?: number;
  done?: boolean;
}

interface RowVisibility {
  tasks: boolean;
  project: boolean;
  worktree: boolean;
  agentIcons: boolean;
  terminalIcons: boolean;
  model: boolean;
  effort: boolean;
  activity: boolean;
  subagents: boolean;
}

let compactClean = false;
let notchWidth = 0;
let notchHeight = 0;
let islandHeight = 0;
let show: RowVisibility = {
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

interface ApprovalRequest {
  approval_id: string;
  session_id: string;
  tool_name?: string;
  tool_input?: unknown;
  reason?: string;
}

interface ApprovalResolved {
  approval_id: string;
  session_id: string;
  decision: ApprovalDecision;
}

interface QuestionOption {
  label: string;
  description?: string;
}

interface Question {
  question: string;
  header?: string;
  options: QuestionOption[];
  multi_select: boolean;
  custom: boolean;
  id?: string;
}

interface QuestionRequest {
  question_id: string;
  session_id: string;
  agent: string;
  questions: Question[];
  answerable: boolean;
  expires_in_ms?: number;
}

interface QuestionResolved {
  question_id: string;
  session_id: string;
  outcome: "answered" | "cancelled" | "expired";
}

/* ------------------------------------------------------------------ */
/* Sizes / tuning                                                      */
/* ------------------------------------------------------------------ */

interface Size {
  w: number;
  h: number;
}

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
const LEAVE_MS = 200;

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
/* Element refs                                                        */
/* ------------------------------------------------------------------ */

const islandEl = document.getElementById("island")!;
const compactViewEl = document.getElementById("compact-view")!;
const expandedViewEl = document.getElementById("expanded-view")!;
const compactRowEl = document.getElementById("compact-row")!;
const headerLabelEl = document.getElementById("header-label")!;
const headerUsageEl = document.getElementById("header-usage")!;
const headerUsageModelEl = document.getElementById("header-usage-model")!;
const sessionListEl = document.getElementById("session-list")!;
const jumpErrorEl = document.getElementById("jump-error")!;
const approvalCardEl = document.getElementById("approval-card")!;
const approvalToolEl = document.getElementById("approval-tool")!;
const approvalSummaryEl = document.getElementById("approval-summary")!;
const approvalAllowEl = document.getElementById("approval-allow") as HTMLButtonElement;
const approvalAlwaysEl = document.getElementById("approval-always") as HTMLButtonElement;
const approvalDenyEl = document.getElementById("approval-deny") as HTMLButtonElement;
const approvalDiffEl = document.getElementById("approval-diff")!;
const approvalKickerEl = document.getElementById("approval-kicker")!;
const approvalActionsEl = document.getElementById("approval-actions")!;
const questionCardEl = document.getElementById("question-card")!;
const questionKickerEl = document.getElementById("question-kicker")!;
const questionCountEl = document.getElementById("question-count")!;
const questionBodyEl = document.getElementById("question-body")!;
const questionActionsEl = document.getElementById("question-actions")!;

const headerMuteEl = document.getElementById("header-mute") as HTMLButtonElement;
const headerSettingsEl = document.getElementById("header-settings") as HTMLButtonElement;
const headerUpdateEl = document.getElementById("header-update") as HTMLButtonElement;
const muteWavesEl = document.getElementById("header-mute-waves") as unknown as SVGGElement;
const muteCrossEl = document.getElementById("header-mute-cross") as unknown as SVGGElement;

const compactSpriteEl = document.createElement("span");
compactSpriteEl.className = "compact-sprite";
const compactProject = document.createElement("span");
compactProject.className = "compact-project";
const compactTail = document.createElement("span");
compactTail.className = "compact-tail";
const compactCount = document.createElement("span");
compactCount.className = "compact-count";
const compactLabel = document.createElement("span");
compactLabel.className = "compact-label";
compactTail.append(compactCount, compactLabel);

/* ------------------------------------------------------------------ */
/* State                                                               */
/* ------------------------------------------------------------------ */

let expanded = false;
let hovered = false;
let sessions: Session[] = [];
let lastListKey = "";
let pendingApproval: ApprovalRequest | null = null;
let resolvingApproval = false;
let pendingQuestion: QuestionRequest | null = null;
let questionAnswers: string[][] = [];
let resolvingQuestion = false;
let questionDeadline = 0;
let countdownTimer = 0;
let config: Record<string, unknown> = {};

interface UsageWindow {
  key: string;
  label: string;
  percent: number;
  resets_at_ms?: number;
}

interface UsageModelWindow {
  model: string;
  percent: number;
  resets_at_ms?: number;
}

interface UsageSnapshot {
  provider: string;
  windows: UsageWindow[];
  models?: UsageModelWindow[];
  reset_cards?: { id: string; title?: string; expires_at_ms?: number }[];
  credits?: { balance: number; unlimited: boolean };
  fetched_at_ms: number;
}

interface ProviderUsage {
  provider: string;
  detected: boolean;
  snapshot?: UsageSnapshot;
  error?: string;
  checked_at_ms: number;
}

interface UsageReport {
  providers: ProviderUsage[];
}

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

function reducedMotion(): boolean {
  return motionQuery.matches;
}

const CARD_LEAVE_MS = 110;

function showCard(card: HTMLElement, visible: boolean, onHidden?: () => void): void {
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
  const fromCompositor = !fixed && compactHeight !== undefined && compactHeight > 0;
  const base = fixed
    ? islandHeight
    : fromCompositor
      ? compactHeight
      : Math.round(BASE_COMPACT.h * next);
  const floor = fromCompositor ? -(COMPACT_OVERHANG - 1) : -(base - MIN_COMPACT_H);
  const height = base + Math.max(floor, notchHeight);
  const width = Math.max(MIN_COMPACT_W, Math.round(BASE_COMPACT.w * next) + notchWidth);
  if (next === uiScale && height === COMPACT.h && width === COMPACT.w) return;
  uiScale = next;
  COMPACT = { w: width, h: height };
  EXPANDED = { w: Math.round(BASE_EXPANDED.w * next), h: Math.round(BASE_EXPANDED.h * next) };
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
function syncExpandedSize(): void {
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
  if (collapseOnLeave && expanded) collapse();
}

islandEl.addEventListener("mouseenter", pointerEntered);
islandEl.addEventListener("mouseleave", pointerLeft);

/* ------------------------------------------------------------------ */
/* Rendering                                                           */
/* ------------------------------------------------------------------ */

function listKey(list: Session[]): string {
  return list
    .map((s) =>
      [
        s.id,
        s.agent,
        s.cwd,
        s.title,
        s.terminal,
        s.hook_id ?? "",
        s.status ?? "",
        s.current_tool ?? "",
        s.summary ?? "",
        s.mode ?? "",
        s.permission_state ?? "",
        s.question_state ?? "",
        s.attention ?? "",
        s.name ?? "",
        s.branch ?? "",
      ].join("|"),
    )
    .join("\n");
}

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
    if (!hovered && pendingApproval === null && pendingQuestion === null) collapse();
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

function renderCompact(n: number): void {
  if (n === 0) {
    compactRowEl.replaceChildren();
    return;
  }
  const lead = sessions[0];
  const wanted = spriteAgent(lead.agent);
  let sprite = compactSpriteEl.firstElementChild as SVGSVGElement | null;
  if (sprite === null || sprite.dataset.agent !== wanted) {
    sprite = createSprite(lead.agent);
    compactSpriteEl.replaceChildren(sprite);
  }
  sprite.classList.toggle("is-walking", strongestAttention(sessions) !== "idle");
  if (compactSpriteEl.parentElement === null) {
    compactRowEl.append(compactSpriteEl, compactProject, compactTail);
  }
  compactProject.hidden = compactClean || !show.project;
  if (compactProject.textContent !== lead.title) compactProject.textContent = lead.title;
  compactLabel.hidden = compactClean;
  const label = strings.island.compactSessions(n);
  if (compactLabel.textContent !== label) compactLabel.textContent = label;
  const next = String(n);
  if (compactCount.textContent !== next) {
    compactCount.textContent = next;
    compactCount.classList.remove("is-rolling");
    void compactCount.offsetWidth;
    compactCount.classList.add("is-rolling");
  }
}

function renderList(): void {
  const owner = transcriptOwner(sessions);
  const wanted = new Map(sessions.map((session) => [session.id, session]));
  const alive = new Map<string, HTMLLIElement>();

  for (const li of [...sessionListEl.children] as HTMLLIElement[]) {
    const id = li.dataset.sessionId;
    if (id === undefined) continue;
    if (li.classList.contains("is-leaving")) continue;
    const session = wanted.get(id);
    if (session === undefined) {
      leaveRow(li);
      continue;
    }
    fillRow(li, session, session.id === owner);
    alive.set(id, li);
  }

  let index = 0;
  let previous: HTMLLIElement | null = null;
  for (const session of sessions) {
    let li = alive.get(session.id);
    if (li === undefined) {
      li = createRow(session, session.id === owner);
      li.classList.add("is-entering");
      li.style.setProperty("--enter-index", String(index));
      clearOnAnimationEnd(li, "is-entering");
      index += 1;
    }
    const anchor: ChildNode | null =
      previous === null ? sessionListEl.firstChild : previous.nextSibling;
    if (li !== anchor) sessionListEl.insertBefore(li, anchor);
    previous = li;
  }
}

function leaveRow(li: HTMLLIElement): void {
  li.classList.add("is-leaving");
  li.style.height = `${li.offsetHeight}px`;
  void li.offsetWidth;
  li.style.height = "0px";
  const done = (): void => {
    li.remove();
    syncExpandedSize();
  };
  if (reducedMotion()) {
    done();
    return;
  }
  const timer = window.setTimeout(done, LEAVE_MS + 120);
  li.addEventListener(
    "transitionend",
    (event) => {
      if (event.propertyName !== "height") return;
      clearTimeout(timer);
      done();
    },
    { once: true },
  );
}

function render(): void {
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

function strongestAttention(list: Session[]): Attention {
  const order: Attention[] = ["waiting_for_input", "needs_attention", "working", "idle"];
  const states = list.map((session) => session.attention ?? "working");
  return order.find((state) => states.includes(state)) ?? "working";
}

const BRANCH_GLYPH =
  `<svg viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.2" ` +
  `stroke-linecap="round" aria-hidden="true">` +
  `<circle cx="3.2" cy="2.6" r="1.3"/><circle cx="3.2" cy="9.4" r="1.3"/>` +
  `<circle cx="8.8" cy="4.4" r="1.3"/><path d="M3.2 3.9v4.2M4.5 4.4h2.2a1.4 1.4 0 0 1 1.4 1.4v.5"/>` +
  `</svg>`;

type BadgeSpec = [string, string, string | undefined, string?];

const terminalIconCache = new Map<string, string | null>();
const terminalIconPending = new Set<string>();

function ensureTerminalIcon(session: Session): void {
  const kind = session.terminal;
  if (!kind || kind === "unknown") return;
  if (terminalIconCache.has(kind) || terminalIconPending.has(kind)) return;
  terminalIconPending.add(kind);
  void invoke<string | null>("terminal_icon", { pid: session.pid })
    .then((url) => {
      terminalIconCache.set(kind, url ?? null);
      terminalIconPending.delete(kind);
      if (url) render();
    })
    .catch(() => {
      terminalIconCache.set(kind, null);
      terminalIconPending.delete(kind);
    });
}

function badge(text: string, kind?: string, iconUrl?: string): HTMLSpanElement {
  const element = document.createElement("span");
  element.className = kind ? `row-badge row-badge-${kind}` : "row-badge";
  if (iconUrl === undefined) {
    element.textContent = text;
    return element;
  }
  const image = document.createElement("img");
  image.src = iconUrl;
  image.alt = text;
  element.append(image);
  return element;
}

function blockedOnUser(session: Session): boolean {
  return session.question_state === "pending" || session.permission_state === "pending";
}

/// The third line, and the one the elapsed badge counts for: what the session is waiting on,
/// then the running tool, then its last answer. Never empty, so the row keeps its height.
function activityText(session: Session): string {
  if (session.question_state === "pending") return strings.session.waitingAnswer;
  if (session.permission_state === "pending") return strings.session.waitingApproval;
  if (session.current_tool) {
    return session.summary
      ? `${session.current_tool} ${session.summary}`
      : session.current_tool;
  }
  if (session.last_message) return session.last_message;
  const attention = strings.attention[session.attention ?? "working"];
  return session.status ?? (attention === "" ? strings.session.working : attention);
}

function clearOnAnimationEnd(element: HTMLElement, className: string): void {
  element.addEventListener(
    "animationend",
    () => {
      element.classList.remove(className);
      element.style.removeProperty("--enter-index");
    },
    { once: true },
  );
}

function setAttention(element: HTMLElement, attention: Attention): void {
  const next = `attention-${attention}`;
  if (element.classList.contains(next)) return;
  for (const name of [...element.classList]) {
    if (name.startsWith("attention-")) element.classList.remove(name);
  }
  element.classList.add(next);
}

function separatorSpan(): HTMLSpanElement {
  const separator = document.createElement("span");
  separator.className = "row-separator";
  separator.setAttribute("aria-hidden", "true");
  separator.textContent = strings.session.separator;
  return separator;
}

export function badgeSpec(session: Session): BadgeSpec[] {
  const specs: BadgeSpec[] = [];
  if (session.mode === "bypassPermissions") specs.push(["mode", strings.session.bypass, "bypass"]);
  const iconUrl = SESSION_ICONS[session.agent];
  if (show.agentIcons && iconUrl !== undefined) {
    specs.push(["session-icon", strings.session.agent(session.agent), "icon", iconUrl]);
  } else {
    specs.push(["agent", strings.session.agent(session.agent), session.agent]);
  }
  if (session.model && show.model) specs.push(["model", strings.session.model(session.model), undefined]);
  if (session.effort && show.effort) specs.push(["effort", session.effort, undefined]);
  if (session.terminal && session.terminal !== "unknown") {
    const terminalUrl = terminalIconCache.get(session.terminal);
    if (show.terminalIcons && terminalUrl) {
      specs.push(["terminal-icon", session.terminal, "icon", terminalUrl]);
    } else {
      specs.push(["terminal", session.terminal, undefined]);
    }
  }
  if (session.since_ms !== undefined) {
    specs.push(["elapsed", strings.session.elapsed(Date.now() - session.since_ms), "elapsed"]);
  }
  return specs;
}

export function fillBadges(container: HTMLElement, session: Session): void {
  const specs = badgeSpec(session);
  const wanted = new Set(specs.map(([key]) => key));
  for (const existing of [...container.children]) {
    const key = (existing as HTMLElement).dataset.badge;
    if (key === undefined || !wanted.has(key)) existing.remove();
  }
  let previous: Element | null = null;
  for (const [key, text, kind, iconUrl] of specs) {
    let element = container.querySelector<HTMLSpanElement>(`[data-badge="${key}"]`);
    if (element === null) {
      element = badge(text, kind, iconUrl);
      element.dataset.badge = key;
      element.classList.add("is-entering");
      clearOnAnimationEnd(element, "is-entering");
      container.insertBefore(element, previous === null ? container.firstChild : previous.nextSibling);
    } else if (iconUrl !== undefined) {
      const image = element.firstElementChild as HTMLImageElement | null;
      if (image !== null && image.src !== iconUrl) image.src = iconUrl;
    } else if (element.textContent !== text) {
      element.textContent = text;
    }
    previous = element;
  }
}

function fillActivity(label: HTMLElement, session: Session): void {
  const text = activityText(session);
  if (label.textContent === text) return;
  if (label.textContent !== "") {
    label.classList.remove("is-swapping");
    void label.offsetWidth;
    label.classList.add("is-swapping");
  }
  label.textContent = text;
}

function transcriptOwner(list: Session[]): string | null {
  const owner = list.find(
    (session) =>
      session.attention === "needs_attention" && (session.last_message_body ?? "") !== "",
  );
  return owner === undefined ? null : owner.id;
}

function agentSignature(list: Subagent[]): string {
  return list
    .map((agent) =>
      [
        agent.id,
        agent.kind,
        agent.description ?? "",
        agent.tool ?? "",
        agent.summary ?? "",
        agent.done ? "1" : "",
        agent.since_ms ?? "",
      ].join(
        "|",
      ),
    )
    .join("\n");
}

function agentItem(agent: Subagent): HTMLElement {
  const item = document.createElement("span");
  item.className = "agent-item";

  const line = document.createElement("span");
  line.className = "agent-line";
  const dot = document.createElement("span");
  dot.className = agent.done ? "agent-dot is-done" : "agent-dot";
  dot.setAttribute("aria-hidden", "true");
  const name = document.createElement("span");
  name.className = "agent-name";
  name.textContent = agent.description ? `${agent.kind} (${agent.description})` : agent.kind;
  const state = document.createElement("span");
  state.className = "agent-state";
  if (agent.done || agent.since_ms === undefined) {
    state.textContent = agent.done ? strings.session.done : "";
  } else {
    state.classList.add("agent-elapsed");
    state.dataset.since = String(agent.since_ms);
    state.textContent = strings.session.elapsed(Date.now() - agent.since_ms);
  }
  line.append(dot, name, state);
  item.append(line);

  if (agent.tool) {
    const tool = document.createElement("span");
    tool.className = "agent-tool";
    tool.textContent = agent.summary
      ? `${strings.session.subagentTool} ${agent.tool} ${agent.summary}`
      : `${strings.session.subagentTool} ${agent.tool}`;
    item.append(tool);
  }
  return item;
}

function taskSignature(list: Task[]): string {
  return list.map((task) => `${task.status}|${task.content}`).join("\n");
}

function taskItem(task: Task): HTMLElement {
  const item = document.createElement("span");
  item.className = "task-item";
  const mark = document.createElement("span");
  mark.className = `task-mark is-${task.status.replace("_", "-")}`;
  mark.setAttribute("aria-hidden", "true");
  const text = document.createElement("span");
  text.className = "task-text";
  text.textContent = task.content;
  item.append(mark, text);
  return item;
}

function fillTasks(row: HTMLElement, session: Session): void {
  const block = row.querySelector<HTMLElement>(".row-tasks")!;
  const list = session.tasks ?? [];
  block.hidden = list.length === 0 || !show.tasks;
  if (block.hidden) return;

  const done = list.filter(
    (task) => task.status === "completed" || task.status === "cancelled",
  ).length;
  const progress = list.filter((task) => task.status === "in_progress").length;
  const count = block.querySelector<HTMLElement>(".tasks-count")!;
  const label = strings.session.tasks(done, progress, list.length - done - progress);
  if (count.textContent !== label) count.textContent = label;

  const items = block.querySelector<HTMLElement>(".tasks-list")!;
  const signature = taskSignature(list);
  if (items.dataset.signature === signature) return;
  items.dataset.signature = signature;
  items.replaceChildren(...list.map(taskItem));
}

function fillAgents(row: HTMLElement, session: Session): void {
  const block = row.querySelector<HTMLElement>(".row-agents")!;
  const list = session.subagents ?? [];
  block.hidden = list.length === 0;
  if (list.length === 0) return;

  const count = block.querySelector<HTMLElement>(".agents-count")!;
  const label = strings.session.subagents(list.length);
  if (count.textContent !== label) count.textContent = label;

  const items = block.querySelector<HTMLElement>(".agents-list")!;
  items.hidden = !show.subagents;
  if (!show.subagents) return;
  const signature = agentSignature(list);
  if (items.dataset.signature === signature) return;
  items.dataset.signature = signature;
  items.replaceChildren(...list.map(agentItem));
}

function fillTranscript(row: HTMLElement, session: Session, wanted: boolean): void {
  const card = row.querySelector<HTMLElement>(".row-transcript")!;
  if (!wanted) {
    card.hidden = true;
    return;
  }
  if (card.hidden) {
    card.hidden = false;
    card.classList.add("is-entering");
    clearOnAnimationEnd(card, "is-entering");
  }
  const prompt = card.querySelector<HTMLElement>(".transcript-prompt")!;
  const body = card.querySelector<HTMLElement>(".transcript-body")!;
  const promptText = session.summary
    ? `${strings.session.promptPrefix} ${session.summary}`
    : strings.session.promptPrefix;
  if (prompt.textContent !== promptText) prompt.textContent = promptText;
  const bodyText = session.last_message_body ?? "";
  if (body.textContent !== bodyText) body.textContent = bodyText;
}

export function fillRow(li: HTMLLIElement, session: Session, transcript: boolean): void {
  const attention = session.attention ?? "working";
  const row = li.firstElementChild as HTMLButtonElement;
  setAttention(row, attention);
  row.classList.toggle("collapsed", attention === "idle");
  row.title = session.summary
    ? `${session.agent} — ${session.cwd}\n${session.summary}`
    : `${session.agent} — ${session.cwd}`;

  const head = row.querySelector<HTMLElement>(".row-head")!;
  const badges = row.querySelector<HTMLElement>(".row-badges")!;
  const dot = row.querySelector<HTMLElement>(".row-dot")!;
  const sprite = row.querySelector<SVGSVGElement>(".row-sprite")!;
  setAttention(dot, attention);
  sprite.classList.toggle("is-walking", attention !== "idle");

  for (const stale of [...head.children]) {
    if (stale !== dot && stale !== badges && stale !== sprite) stale.remove();
  }

  if (show.project) {
    const project = document.createElement("span");
    project.className = "row-project";
    project.textContent = session.title;
    head.insertBefore(project, badges);
  }

  if (session.branch && show.worktree) {
    const glyph = document.createElement("span");
    glyph.className = "row-branch-glyph";
    glyph.innerHTML = BRANCH_GLYPH;
    glyph.setAttribute("aria-label", strings.session.branchLabel(session.branch));
    const branch = document.createElement("span");
    branch.className = "row-branch";
    branch.textContent = session.branch;
    head.insertBefore(glyph, badges);
    head.insertBefore(branch, badges);
  }

  if (session.name) {
    const name = document.createElement("span");
    name.className = "row-name";
    name.textContent = session.name;
    head.insertBefore(separatorSpan(), badges);
    head.insertBefore(name, badges);
  }

  ensureTerminalIcon(session);
  fillBadges(badges, session);
  const age = badges.querySelector<HTMLElement>(".row-badge-elapsed");
  if (age !== null) age.dataset.since = String(session.since_ms);

  const prompt = row.querySelector<HTMLElement>(".row-prompt")!;
  const promptText = session.summary
    ? `${strings.session.promptPrefix} ${session.summary}`
    : session.cwd;
  if (prompt.textContent !== promptText) prompt.textContent = promptText;
  prompt.hidden = transcript;

  const activity = row.querySelector<HTMLElement>(".row-activity")!;
  activity.hidden = (!show.activity && !blockedOnUser(session)) || transcript;
  fillActivity(row.querySelector<HTMLElement>(".row-activity-label")!, session);

  fillTasks(row, session);
  fillAgents(row, session);
  fillTranscript(row, session, transcript);
  fillMessageBox(li, session);
}

function reasonOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function createMessageBox(sessionId: string): HTMLElement {
  const box = document.createElement("div");
  box.className = "row-message";
  const input = document.createElement("textarea");
  input.className = "message-input";
  input.rows = 1;
  input.placeholder = strings.session.messagePlaceholder;
  input.setAttribute("aria-label", strings.session.messageOpen);
  const hint = document.createElement("span");
  hint.className = "message-hint";
  hint.hidden = true;
  const queue = document.createElement("ul");
  queue.className = "message-queue";
  box.append(input, hint, queue);
  input.addEventListener("input", () => {
    input.style.height = "auto";
    input.style.height = `${input.scrollHeight}px`;
    syncExpandedSize();
  });
  input.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      input.blur();
      return;
    }
    if (event.key !== "Enter" || event.shiftKey) return;
    event.preventDefault();
    const text = input.value;
    if (text.trim() === "") return;
    input.value = "";
    input.style.removeProperty("height");
    void invoke<{ delivered: boolean }>("send_message", { id: sessionId, text }).catch(
      (error: unknown) => {
        input.value = text;
        showError(strings.session.messageFailed(strings.session.messageBlocked(reasonOf(error))));
      },
    );
  });
  return box;
}

function fillMessageBox(li: HTMLLIElement, session: Session): void {
  const box = li.querySelector<HTMLElement>(".row-message")!;
  const input = box.querySelector<HTMLTextAreaElement>(".message-input")!;
  const hint = box.querySelector<HTMLElement>(".message-hint")!;
  const blocked = session.send_blocked;
  input.disabled = blocked !== undefined;
  const reason = blocked === undefined ? "" : strings.session.messageBlocked(blocked);
  hint.textContent = reason;
  hint.hidden = blocked === undefined;
  input.title = reason;

  const queued = session.queued_messages ?? [];
  const badges = li.querySelector<HTMLElement>(".row-badges")!;
  let badge = badges.querySelector<HTMLElement>(".badge-queue");
  if (queued.length === 0) {
    badge?.remove();
  } else {
    if (badge === null) {
      badge = document.createElement("span");
      badge.className = "row-badge badge-queue";
      badges.append(badge);
    }
    badge.textContent = String(queued.length);
    badge.title = strings.session.messageQueued(queued.length);
  }

  const list = box.querySelector<HTMLElement>(".message-queue")!;
  list.replaceChildren();
  for (const message of queued) {
    const item = document.createElement("li");
    item.className = "message-queued";
    item.dataset.messageId = String(message.id);
    const text = document.createElement("span");
    text.className = "message-queued-text";
    text.textContent = message.text;
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "message-cancel";
    cancel.textContent = "✕";
    cancel.title = strings.session.messageCancel;
    cancel.setAttribute("aria-label", strings.session.messageCancel);
    cancel.addEventListener("click", () => {
      void invoke("cancel_message", { id: session.id, messageId: message.id }).catch(
        (error: unknown) => {
          showError(strings.session.messageFailed(reasonOf(error)));
        },
      );
    });
    item.append(text, cancel);
    list.append(item);
  }
  list.hidden = queued.length === 0;
}

export function createRow(session: Session, transcript: boolean): HTMLLIElement {
  const li = document.createElement("li");
  li.dataset.sessionId = session.id;

  const row = document.createElement("button");
  row.type = "button";
  row.className = "session-row";

  const head = document.createElement("span");
  head.className = "row-head";
  const sprite = createSprite(session.agent);
  sprite.classList.add("row-sprite");
  const dot = document.createElement("span");
  dot.className = "row-dot";
  dot.setAttribute("aria-hidden", "true");
  const badges = document.createElement("span");
  badges.className = "row-badges";
  head.append(sprite, badges, dot);

  const prompt = document.createElement("span");
  prompt.className = "row-prompt";

  const activity = document.createElement("span");
  activity.className = "row-activity";
  const label = document.createElement("span");
  label.className = "row-activity-label";
  activity.append(label);

  const tasks = document.createElement("span");
  tasks.className = "row-tasks";
  tasks.hidden = true;
  const tasksHead = document.createElement("span");
  tasksHead.className = "tasks-head";
  const tasksLabel = document.createElement("span");
  tasksLabel.className = "tasks-label";
  tasksLabel.textContent = strings.session.tasksLabel;
  const tasksCount = document.createElement("span");
  tasksCount.className = "tasks-count";
  tasksHead.append(tasksLabel, tasksCount);
  const tasksList = document.createElement("span");
  tasksList.className = "tasks-list";
  tasks.append(tasksHead, tasksList);

  const agents = document.createElement("span");
  agents.className = "row-agents";
  agents.hidden = true;
  const agentsHead = document.createElement("span");
  agentsHead.className = "agents-head";
  const agentsGlyph = document.createElement("span");
  agentsGlyph.className = "agents-glyph";
  agentsGlyph.innerHTML = BRANCH_GLYPH;
  agentsGlyph.setAttribute("aria-hidden", "true");
  const agentsCount = document.createElement("span");
  agentsCount.className = "agents-count";
  agentsHead.append(agentsGlyph, agentsCount);
  const agentsList = document.createElement("span");
  agentsList.className = "agents-list";
  agents.append(agentsHead, agentsList);

  const card = document.createElement("span");
  card.className = "row-transcript";
  card.hidden = true;
  const cardHead = document.createElement("span");
  cardHead.className = "transcript-head";
  const cardPrompt = document.createElement("span");
  cardPrompt.className = "transcript-prompt";
  const cardStatus = document.createElement("span");
  cardStatus.className = "transcript-status";
  cardStatus.textContent = strings.session.done;
  cardHead.append(cardPrompt, cardStatus);
  const cardBody = document.createElement("span");
  cardBody.className = "transcript-body";
  card.append(cardHead, cardBody);

  row.append(head, prompt, activity, tasks, agents, card);
  li.append(row, createMessageBox(session.id));
  fillRow(li, session, transcript);
  row.addEventListener("click", () => {
    const current = sessions.find((entry) => entry.id === session.id);
    if (current !== undefined) jumpTo(current, row);
  });
  return li;
}

/// Only the elapsed text is rewritten, and only while the panel is open, so an idle island
/// never wakes the webview and a tick never re-renders the list.
function tickElapsed(): void {
  if (!expanded) return;
  const now = Date.now();
  for (const element of sessionListEl.querySelectorAll<HTMLElement>(
    ".row-badge-elapsed, .agent-elapsed",
  )) {
    const since = Number(element.dataset.since);
    if (Number.isFinite(since)) element.textContent = strings.session.elapsed(now - since);
  }
}

window.setInterval(tickElapsed, 1000);

/* ------------------------------------------------------------------ */
/* Jump interaction (must NOT collapse the island)                     */
/* ------------------------------------------------------------------ */

let errTimer = 0;

function showError(message: string): void {
  jumpErrorEl.textContent = message;
  jumpErrorEl.hidden = false;
  clearTimeout(errTimer);
  errTimer = window.setTimeout(() => {
    jumpErrorEl.hidden = true;
  }, 4000);
}

function jumpTo(session: Session, row: HTMLButtonElement): void {
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

function jumpToId(id: string): void {
  if (!id) return;
  const session = sessions.find((entry) => entry.id === id);
  invoke("jump", { id }).catch((err: unknown) => {
    showError(strings.jump.failed(session?.title ?? id, String(err)));
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function stringField(value: unknown, field: string): string | undefined {
  if (!isRecord(value)) return undefined;
  const fieldValue = value[field];
  return typeof fieldValue === "string" && fieldValue.length > 0 ? fieldValue : undefined;
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

function planSummary(plan: string): string | undefined {
  return plan
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0 && !line.startsWith("#"));
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

interface DiffLine {
  sign: "-" | "+";
  text: string;
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
/* Agent questions                                                     */
/* ------------------------------------------------------------------ */

function parseQuestion(value: unknown): QuestionRequest | null {
  const questionId = stringField(value, "question_id");
  const sessionId = stringField(value, "session_id");
  const agent = stringField(value, "agent");
  if (!questionId || !sessionId || !agent || !isRecord(value)) return null;
  const raw = value.questions;
  if (!Array.isArray(raw) || raw.length === 0) return null;
  const questions = raw.map((entry): Question => {
    const options = isRecord(entry) && Array.isArray(entry.options) ? entry.options : [];
    return {
      question: stringField(entry, "question") ?? "",
      ...(stringField(entry, "header") ? { header: stringField(entry, "header") } : {}),
      options: options.flatMap((option) => {
        const label = stringField(option, "label");
        if (!label) return [];
        const description = stringField(option, "description");
        return [{ label, ...(description ? { description } : {}) }];
      }),
      multi_select: isRecord(entry) && entry.multi_select === true,
      custom: isRecord(entry) && entry.custom === true,
    };
  });
  return {
    question_id: questionId,
    session_id: sessionId,
    agent,
    questions,
    answerable: value.answerable === true,
    ...(typeof value.expires_in_ms === "number" ? { expires_in_ms: value.expires_in_ms } : {}),
  };
}

function parseQuestionResolution(value: unknown): QuestionResolved | null {
  const questionId = stringField(value, "question_id");
  const sessionId = stringField(value, "session_id");
  const outcome = stringField(value, "outcome");
  if (!questionId || !sessionId) return null;
  if (outcome !== "answered" && outcome !== "cancelled" && outcome !== "expired") return null;
  return { question_id: questionId, session_id: sessionId, outcome };
}

function openQuestion(question: QuestionRequest): void {
  pendingQuestion = question;
  questionAnswers = question.questions.map(() => []);
  resolvingQuestion = false;
  clearInterval(countdownTimer);
  questionDeadline = question.expires_in_ms ? Date.now() + question.expires_in_ms : 0;
  if (questionDeadline > 0) {
    countdownTimer = window.setInterval(tickCountdown, 1000);
  }
}

function closeQuestion(): void {
  pendingQuestion = null;
  questionAnswers = [];
  resolvingQuestion = false;
  questionDeadline = 0;
  clearInterval(countdownTimer);
}

function toggleAnswer(index: number, label: string, multiSelect: boolean): void {
  const current = questionAnswers[index] ?? [];
  if (!multiSelect) {
    questionAnswers[index] = current[0] === label ? [] : [label];
  } else if (current.includes(label)) {
    questionAnswers[index] = current.filter((entry) => entry !== label);
  } else {
    questionAnswers[index] = [...current, label];
  }
  paintAnswers(index);
  renderQuestionActions();
}


function paintAnswers(index: number): void {
  const options = questionBodyEl.querySelector<HTMLElement>(
    `.question-options[data-question="${index}"]`,
  );
  if (options === null) return;
  const selected = questionAnswers[index] ?? [];
  for (const button of options.querySelectorAll<HTMLButtonElement>(".question-option")) {
    const on = selected.includes(button.dataset.option ?? "");
    button.classList.toggle("selected", on);
    button.setAttribute("aria-pressed", String(on));
  }
  const custom = questionBodyEl.querySelector<HTMLInputElement>(
    `.question-custom[data-question="${index}"]`,
  );
  if (custom !== null && selected.length > 0) custom.value = "";
}

function answersComplete(): boolean {
  return questionAnswers.every((labels) => labels.length > 0);
}

function questionItem(question: Question, index: number): HTMLDivElement {
  const item = document.createElement("div");
  item.className = "question-item";

  const text = document.createElement("span");
  text.className = "question-text";
  text.textContent = question.header
    ? `${question.header} — ${question.question}`
    : question.question;
  item.append(text);

  const options = document.createElement("div");
  options.className = "question-options";
  options.dataset.question = String(index);
  options.setAttribute("aria-label", strings.question.optionsLabel);
  for (const option of question.options) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "question-option";
    button.dataset.option = option.label;
    button.textContent = option.label;
    if (option.description) button.title = option.description;
    const selected = (questionAnswers[index] ?? []).includes(option.label);
    button.classList.toggle("selected", selected);
    button.setAttribute("aria-pressed", String(selected));
    button.disabled = resolvingQuestion || !pendingQuestion?.answerable;
    button.addEventListener("click", () =>
      toggleAnswer(index, option.label, question.multi_select),
    );
    options.append(button);
  }
  item.append(options);

  if (question.multi_select) {
    const hint = document.createElement("span");
    hint.className = "question-hint";
    hint.textContent = strings.question.multiSelectHint;
    item.append(hint);
  }

  if (question.custom && pendingQuestion?.answerable) {
    const custom = document.createElement("input");
    custom.type = "text";
    custom.className = "question-custom";
    custom.dataset.question = String(index);
    custom.placeholder = strings.question.customPlaceholder;
    custom.disabled = resolvingQuestion;
    custom.addEventListener("input", () => {
      const value = custom.value.trim();
      questionAnswers[index] = value ? [value] : [];
      renderQuestionActions();
      for (const button of options.querySelectorAll("button")) {
        button.classList.remove("selected");
        button.setAttribute("aria-pressed", "false");
      }
    });
    item.append(custom);
  }

  return item;
}

function countdownText(): string {
  const remaining =
    questionDeadline > 0 ? Math.max(0, Math.ceil((questionDeadline - Date.now()) / 1000)) : 0;
  return remaining > 0
    ? `${strings.question.terminalFallback} · ${strings.question.countdown(remaining)}`
    : strings.question.terminalFallback;
}

function tickCountdown(): void {
  const fallback = questionActionsEl.querySelector<HTMLElement>(".question-fallback");
  if (fallback === null) {
    renderQuestionActions();
    return;
  }
  const text = countdownText();
  if (fallback.textContent !== text) fallback.textContent = text;
}

function renderQuestionActions(): void {
  questionActionsEl.replaceChildren();
  if (!pendingQuestion) return;

  if (pendingQuestion.answerable) {
    const submit = document.createElement("button");
    submit.type = "button";
    submit.className = "question-submit";
    submit.textContent = strings.question.submit;
    submit.disabled = resolvingQuestion || !answersComplete();
    submit.addEventListener("click", submitAnswers);
    questionActionsEl.append(submit);
    return;
  }

  const fallback = document.createElement("span");
  fallback.className = "question-fallback";
  fallback.textContent = countdownText();
  const jump = document.createElement("button");
  jump.type = "button";
  jump.className = "question-jump";
  jump.textContent = strings.question.jump;
  jump.addEventListener("click", () => jumpToId(pendingQuestion?.session_id ?? ""));
  questionActionsEl.append(fallback, jump);
}

function renderQuestion(): void {
  if (!pendingQuestion) {
    showCard(questionCardEl, false, () => {
      questionCardEl.classList.remove("focused");
      questionBodyEl.replaceChildren();
      questionActionsEl.replaceChildren();
    });
    return;
  }
  showCard(questionCardEl, true);
  questionCardEl.setAttribute("aria-label", strings.question.label);
  questionKickerEl.textContent = strings.question.kicker(pendingQuestion.agent);
  questionCountEl.textContent = strings.question.count(pendingQuestion.questions.length);
  questionBodyEl.replaceChildren(...pendingQuestion.questions.map(questionItem));
  renderQuestionActions();
}

function submitAnswers(): void {
  if (!pendingQuestion || resolvingQuestion || !answersComplete()) return;
  const question = pendingQuestion;
  const answers = questionAnswers.map((labels) => [...labels]);
  resolvingQuestion = true;
  renderQuestion();
  void invoke("answer_question", { questionId: question.question_id, answers })
    .then(() => {
      if (pendingQuestion?.question_id === question.question_id) closeQuestion();
      render();
      resetIdle();
    })
    .catch((error: unknown) => {
      resolvingQuestion = false;
      renderQuestion();
      showError(strings.question.failed(String(error)));
    });
}



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

function resetIdle(): void {
  if (idle) {
    idle = false;
    islandEl.style.opacity = "1";
  }
  clearTimeout(idleTimer);
  idleTimer = window.setTimeout(() => {
    idle = true;
    islandEl.style.opacity = "0";
  }, idleMs);
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
  if (pendingApproval !== null || pendingQuestion !== null) return;
  collapse();
});

function section(root: Record<string, unknown>, name: string): Record<string, unknown> {
  const value = root[name];
  return isRecord(value) ? value : {};
}

function milliseconds(root: Record<string, unknown>, name: string, fallback: number): number {
  const value = root[name];
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

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
  let metrics: { scale: number; compact_height: number | null } = {
    scale: 1,
    compact_height: null,
  };
  try {
    metrics = await invoke<{ scale: number; compact_height: number | null }>("island_metrics");
  } catch {
    metrics = { scale: 1, compact_height: null };
  }
  const scale = typeof override === "number" && override > 0 ? override : metrics.scale;
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
