import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import { SESSION_ICONS } from "./session-icons";
import { createSprite, spriteAgent } from "./sprites";
import {
  compactActivity,
  compactCount,
  compactKicker,
  compactLeading,
  compactNeed,
  compactProject,
  compactRowEl,
  compactSpriteEl,
  compactStripEl,
  compactTail,
  compactText,
  compactWordmark,
  sessionEmptyEl,
  sessionListEl,
} from "./elements";
import { StateStrip } from "./strip";
import { createMessageBox, fillMessageBox, disposeMessageBox } from "./message";
import { Attention, Session, Subagent, Task, RowVisibility } from "./types";
import type { ActionIdentity } from "./daemon-state";
interface RowContext {
  readonly compactClean: boolean;
  readonly expanded: boolean;
  readonly visible: boolean;
  readonly offline: boolean;
  readonly sessions: Session[];
  readonly show: RowVisibility;
  readonly forceFill: boolean;
  readonly LEAVE_MS: number;
  jumpTo(session: Session, row: HTMLElement): void;
  reducedMotion(): boolean;
  render(): void;
  syncExpandedSize(): void;
  showError(message: string): void;
  canUseTarget(id: string, identity: ActionIdentity, writing?: boolean): boolean;
}
let context: RowContext;
const displayedSessions = new WeakMap<HTMLLIElement, Session>();
const headSignatures = new WeakMap<HTMLElement, string>();
const openDetails = new Set<string>();
export function initializeRows(value: RowContext): void { context = value; }

function setHidden(element: HTMLElement, hidden: boolean): void {
  if (element.hidden !== hidden) element.hidden = hidden;
}

function setChildren(parent: HTMLElement, wanted: readonly HTMLElement[]): void {
  const current = [...parent.children];
  if (current.length === wanted.length && current.every((child, index): boolean => child === wanted[index])) return;
  parent.replaceChildren(...wanted);
}

function setText(element: HTMLElement, value: string): void {
  if (element.textContent !== value) element.textContent = value;
}

function setClass(element: HTMLElement, value: string): void {
  if (element.className !== value) element.className = value;
}

export function listKey(list: Session[]): string {
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

export interface CompactPending {
  readonly kind: "approval" | "question";
  readonly sessionId: string;
}

type NeedKind = "permission" | "question" | "done";

interface CompactNeed {
  readonly kind: NeedKind;
  readonly session: Session | undefined;
}

const compactStrip = new StateStrip(compactStripEl);
compactWordmark.textContent = strings.island.wordmark;

function compactNeedOf(list: Session[], pending: CompactPending | null): CompactNeed | null {
  if (pending !== null) {
    return {
      kind: pending.kind === "question" ? "question" : "permission",
      session: list.find((session) => session.id === pending.sessionId),
    };
  }
  const waiting = list.find((session) => session.attention === "waiting_for_input");
  if (waiting !== undefined) {
    return { kind: waiting.question_state === "pending" ? "question" : "permission", session: waiting };
  }
  const done = list.find((session) => session.attention === "needs_attention");
  return done === undefined ? null : { kind: "done", session: done };
}

function setCompactSprite(agent: string, walking: boolean): void {
  const wanted = spriteAgent(agent);
  let sprite = compactSpriteEl.firstElementChild as SVGSVGElement | null;
  if (sprite === null || sprite.dataset.agent !== wanted) {
    sprite = createSprite(agent);
    compactSpriteEl.replaceChildren(sprite);
  }
  sprite.classList.toggle("is-walking", walking);
}

function setCompactCount(n: number, tone: string): void {
  setClass(compactCount, `compact-count pixel ${tone}`.trim());
  const next = n === 0 ? "" : String(n);
  if (compactCount.textContent === next) return;
  compactCount.textContent = next;
  if (next === "") return;
  compactCount.classList.remove("is-rolling");
  void compactCount.offsetWidth;
  compactCount.classList.add("is-rolling");
}

export function renderCompact(n: number, pending: CompactPending | null, offline: boolean): void {
  const list = context.sessions;
  const need = offline ? null : compactNeedOf(list, pending);
  const lead = list[0];
  compactStrip.update(list, offline);
  setCompactCount(n, need === null ? "" : need.kind === "done" ? "needs_attention" : "waiting_for_input");
  setHidden(compactTail, n === 0);
  if (offline) {
    setCompactSprite("unknown", false);
    setClass(compactKicker, "compact-kicker pixel offline");
    setText(compactKicker, strings.island.noConnection);
    setChildren(compactNeed, [compactKicker]);
    setChildren(compactLeading, [compactSpriteEl, compactNeed]);
    setChildren(compactRowEl, [compactLeading, compactTail]);
    return;
  }
  if (need !== null) {
    setClass(compactKicker, `compact-kicker pixel ${need.kind === "done" ? "needs_attention" : "waiting_for_input"}`);
    setText(compactKicker, strings.island.kicker[need.kind]);
    const project = need.session?.title ?? "";
    setText(compactProject, project);
    setChildren(compactNeed, project === "" ? [compactKicker] : [compactKicker, compactProject]);
    if (lead !== undefined) setCompactSprite(lead.agent, strongestAttention(list) !== "idle");
    setChildren(compactLeading, lead === undefined ? [compactNeed] : [compactSpriteEl, compactNeed]);
    setChildren(compactRowEl, [compactLeading, compactTail]);
    return;
  }
  if (lead === undefined) {
    setChildren(compactRowEl, [compactWordmark]);
    return;
  }
  setCompactSprite(lead.agent, strongestAttention(list) !== "idle");
  if (context.compactClean || !context.show.project) {
    setChildren(compactLeading, [compactSpriteEl]);
  } else {
    setText(compactProject, lead.title);
    const tool = lead.current_tool ?? "";
    setText(compactActivity, tool);
    setChildren(compactText, tool === "" ? [compactProject] : [compactProject, compactActivity]);
    setChildren(compactLeading, [compactSpriteEl, compactText]);
  }
  setChildren(compactRowEl, [compactLeading, compactTail]);
}

const emptySprite = createSprite("unknown");
const emptyTitle = document.createElement("b");
emptyTitle.textContent = strings.island.empty;
const emptyHint = document.createElement("span");
emptyHint.textContent = strings.island.emptyHint;
const emptyCopy = document.createElement("span");
emptyCopy.className = "session-empty-copy";
emptyCopy.append(emptyTitle, emptyHint);
sessionEmptyEl.append(emptySprite, emptyCopy);

export function renderList(): void {
  const owner = transcriptOwner(context.sessions);
  const fill = context.expanded || context.forceFill;
  const wanted = new Map(context.sessions.map((session) => [session.id, session]));
  const alive = new Map<string, HTMLLIElement>();
  setHidden(sessionEmptyEl, context.sessions.length !== 0 || context.offline);
  if (sessionListEl.classList.contains("is-offline") !== context.offline) {
    sessionListEl.classList.toggle("is-offline", context.offline);
  }

  for (const li of [...sessionListEl.children] as HTMLLIElement[]) {
    const id = li.dataset.sessionId;
    if (id === undefined) continue;
    if (li.classList.contains("is-leaving")) continue;
    const session = wanted.get(id);
    if (session === undefined) {
      leaveRow(li);
      continue;
    }
    if (fill) fillRow(li, session, session.id === owner);
    alive.set(id, li);
  }

  let index = 0;
  let previous: HTMLLIElement | null = null;
  for (const session of context.sessions) {
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
    disposeMessageBox(li);
    openDetails.delete(li.dataset.sessionId ?? "");
    li.remove();
    context.syncExpandedSize();
  };
  if (context.reducedMotion()) {
    done();
    return;
  }
  const timer = window.setTimeout(done, context.LEAVE_MS + 120);
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

const terminalIconCache = new Map<string, string | null>();
const terminalIconPending = new Set<string>();

function ensureTerminalIcon(session: Session): void {
  const kind = session.terminal;
  if (!kind || kind === "unknown") return;
  if (terminalIconCache.has(kind) || terminalIconPending.has(kind)) return;
  terminalIconPending.add(kind);
  void invoke<unknown>("terminal_icon", { pid: session.pid })
    .then((url) => {
      const icon = typeof url === "string" && url !== "" ? url : null;
      terminalIconCache.set(kind, icon);
      terminalIconPending.delete(kind);
      if (icon !== null) context.render();
    })
    .catch(() => {
      terminalIconCache.set(kind, null);
      terminalIconPending.delete(kind);
    });
}

function blockedOnUser(session: Session): boolean {
  return session.question_state === "pending" || session.permission_state === "pending";
}

function activityText(session: Session): string {
  if (session.question_state === "pending") return strings.session.waitingAnswer;
  if (session.permission_state === "pending") return strings.session.waitingApproval;
  if (session.current_tool) return session.current_tool;
  if (session.last_message) return session.last_message;
  const attention = strings.attention[session.attention ?? "working"];
  return session.status ?? (attention === "" ? strings.session.working : attention);
}

function shortCwd(cwd: string): string {
  return cwd.replace(/^\/(?:home|Users)\/[^/]+/, "~");
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

function mascotKind(agent: string): "sprite" | "logo" {
  const logo = SESSION_ICONS[agent];
  if (context.show.mascot === "logo" && logo !== undefined) return "logo";
  if (spriteAgent(agent) !== "unknown") return "sprite";
  return logo === undefined ? "sprite" : "logo";
}

function logoImage(agent: string): HTMLImageElement {
  const image = document.createElement("img");
  image.className = "row-logo";
  image.src = SESSION_ICONS[agent] ?? "";
  image.alt = "";
  return image;
}

function fillMascot(head: HTMLElement, session: Session, walking: boolean): void {
  const kind = mascotKind(session.agent);
  const key = `${kind}:${session.agent}`;
  let mascot = head.querySelector<HTMLElement | SVGElement>(".row-mascot");
  if (mascot === null || mascot.dataset.mascot !== key) {
    const next: HTMLElement | SVGElement = kind === "logo" ? logoImage(session.agent) : createSprite(session.agent);
    next.classList.add("row-mascot");
    next.dataset.mascot = key;
    if (mascot === null) head.prepend(next);
    else mascot.replaceWith(next);
    mascot = next;
  }
  mascot.classList.toggle("is-walking", walking);
}

function metaDot(): HTMLSpanElement {
  const dot = document.createElement("span");
  dot.className = "meta-dot";
  dot.setAttribute("aria-hidden", "true");
  dot.textContent = strings.session.separator;
  return dot;
}

function metaItem(className: string, text: string): HTMLSpanElement {
  const item = document.createElement("span");
  item.className = `meta-item ${className}`;
  item.textContent = text;
  return item;
}

function buildMeta(session: Session, terminalIcon: string | null | undefined): HTMLElement[] {
  const parts: HTMLElement[] = [];
  const push = (item: HTMLElement): void => {
    if (parts.length > 0) parts.push(metaDot());
    parts.push(item);
  };
  if (session.branch && context.show.worktree) {
    const branch = document.createElement("span");
    branch.className = "meta-item meta-branch";
    branch.setAttribute("aria-label", strings.session.branchLabel(session.branch));
    const glyph = document.createElement("span");
    glyph.className = "meta-glyph";
    glyph.innerHTML = BRANCH_GLYPH;
    const name = document.createElement("span");
    name.className = "meta-branch-name";
    name.textContent = session.branch;
    branch.append(glyph, name);
    push(branch);
  }
  if (session.model && context.show.model) push(metaItem("meta-model", strings.session.model(session.model)));
  if (session.effort && context.show.effort) push(metaItem("meta-effort", session.effort));
  if (session.terminal && session.terminal !== "unknown") {
    if (context.show.terminalIcons && terminalIcon) {
      const item = document.createElement("span");
      item.className = "meta-item meta-terminal";
      const image = document.createElement("img");
      image.className = "meta-terminal-icon";
      image.src = terminalIcon;
      image.alt = session.terminal;
      item.append(image);
      push(item);
    } else {
      push(metaItem("meta-terminal", session.terminal));
    }
  }
  if (session.mode === "bypassPermissions") {
    const chip = document.createElement("span");
    chip.className = "row-state pixel bypass";
    chip.textContent = strings.session.bypass;
    parts.push(chip);
  }
  return parts;
}

function fillHead(row: HTMLElement, session: Session): void {
  const head = row.querySelector<HTMLElement>(".row-head")!;
  const project = head.querySelector<HTMLElement>(".row-project")!;
  const name = head.querySelector<HTMLElement>(".row-name")!;
  const meta = head.querySelector<HTMLElement>(".row-meta")!;
  const terminalIcon = session.terminal ? terminalIconCache.get(session.terminal) : null;
  const show = context.show;
  setText(project, session.title);
  setHidden(project, !show.project);
  setText(name, session.name ?? "");
  setHidden(name, !session.name);
  const signature = JSON.stringify([
    show.worktree ? session.branch ?? "" : "",
    show.model ? session.model ?? "" : "",
    show.effort ? session.effort ?? "" : "",
    session.terminal,
    show.terminalIcons ? terminalIcon ?? "" : "",
    session.mode ?? "",
  ]);
  if (headSignatures.get(head) === signature) return;
  headSignatures.set(head, signature);
  meta.replaceChildren(...buildMeta(session, terminalIcon));
  setHidden(meta, meta.childElementCount === 0);
}

function fillTail(row: HTMLElement, session: Session, attention: Attention): void {
  const tail = row.querySelector<HTMLElement>(".row-tail")!;
  const elapsed = tail.querySelector<HTMLElement>(".row-elapsed")!;
  let state = tail.querySelector<HTMLElement>(".row-state");
  if (attention === "working") {
    state?.remove();
  } else {
    if (state === null) {
      state = document.createElement("span");
      tail.prepend(state);
    }
    setClass(state, `row-state pixel ${attention}`);
    setText(state, strings.session.stateChip[attention]);
  }
  if (session.since_ms === undefined) {
    setHidden(elapsed, true);
    return;
  }
  setHidden(elapsed, false);
  if (elapsed.dataset.since !== String(session.since_ms)) elapsed.dataset.since = String(session.since_ms);
  setText(elapsed, strings.session.elapsed(Date.now() - session.since_ms));
  if (tail.lastElementChild !== elapsed) tail.append(elapsed);
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
  state.className = "agent-state pixel";
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

function taskCounts(list: Task[]): { done: number; total: number } {
  const done = list.filter((task) => task.status === "completed" || task.status === "cancelled").length;
  return { done, total: list.length };
}

function fillChips(row: HTMLElement, session: Session): void {
  const chips = row.querySelector<HTMLElement>(".row-chips")!;
  const tasksChip = chips.querySelector<HTMLButtonElement>('[data-chip="tasks"]')!;
  const agentsChip = chips.querySelector<HTMLButtonElement>('[data-chip="agents"]')!;
  const tasks = session.tasks ?? [];
  const agents = session.subagents ?? [];
  const open = openDetails.has(session.id);
  const showTasks = tasks.length > 0 && context.show.tasks;
  setHidden(tasksChip, !showTasks);
  if (showTasks) {
    const { done, total } = taskCounts(tasks);
    setText(tasksChip.querySelector<HTMLElement>(".chip-text")!, strings.session.tasksChip(done, total));
    const width = `${Math.round((100 * done) / total)}%`;
    const fill = tasksChip.querySelector<HTMLElement>(".chip-fill")!;
    if (fill.style.width !== width) fill.style.width = width;
  }
  const showAgents = agents.length > 0;
  setHidden(agentsChip, !showAgents);
  if (showAgents) {
    const live = agents.filter((agent) => !agent.done).length;
    setText(agentsChip.querySelector<HTMLElement>(".chip-text")!, strings.session.agentsChip(live, agents.length));
    agentsChip.classList.toggle("is-live", live > 0);
    if (agentsChip.disabled !== !context.show.subagents) agentsChip.disabled = !context.show.subagents;
  }
  setHidden(chips, !showTasks && !showAgents);
  const expanded = String(open);
  if (tasksChip.getAttribute("aria-expanded") !== expanded) tasksChip.setAttribute("aria-expanded", expanded);
  if (agentsChip.getAttribute("aria-expanded") !== expanded) agentsChip.setAttribute("aria-expanded", expanded);
}

function fillTasks(row: HTMLElement, session: Session): void {
  const block = row.querySelector<HTMLElement>(".row-tasks")!;
  const list = session.tasks ?? [];
  const hidden = list.length === 0 || !context.show.tasks || !openDetails.has(session.id);
  setHidden(block, hidden);
  if (hidden) return;
  const items = block.querySelector<HTMLElement>(".tasks-list")!;
  const signature = taskSignature(list);
  if (items.dataset.signature === signature) return;
  items.dataset.signature = signature;
  items.replaceChildren(...list.map(taskItem));
}

function fillAgents(row: HTMLElement, session: Session): void {
  const block = row.querySelector<HTMLElement>(".row-agents")!;
  const list = session.subagents ?? [];
  setHidden(block, list.length === 0 || !openDetails.has(session.id));
  const items = block.querySelector<HTMLElement>(".agents-list")!;
  setHidden(items, !context.show.subagents);
  if (list.length === 0 || !context.show.subagents) return;
  const signature = agentSignature(list);
  if (items.dataset.signature === signature) return;
  items.dataset.signature = signature;
  items.replaceChildren(...list.map(agentItem));
}

function fillTranscript(row: HTMLElement, session: Session, wanted: boolean): void {
  const card = row.querySelector<HTMLElement>(".row-transcript")!;
  if (!wanted) {
    setHidden(card, true);
    return;
  }
  if (card.hidden) {
    setHidden(card, false);
    card.classList.add("is-entering");
    clearOnAnimationEnd(card, "is-entering");
  }
  const prompt = card.querySelector<HTMLElement>(".transcript-prompt")!;
  const body = card.querySelector<HTMLElement>(".transcript-body")!;
  const promptText = session.summary
    ? `${strings.session.promptPrefix} ${session.summary}`
    : strings.session.promptPrefix;
  setText(prompt, promptText);
  setText(body, session.last_message_body ?? "");
}

export function fillRow(li: HTMLLIElement, session: Session, transcript: boolean): void {
  displayedSessions.set(li, { ...session, action_identity: session.action_identity ? { ...session.action_identity } : undefined });
  const attention = session.attention ?? "working";
  const row = li.firstElementChild as HTMLElement;
  setAttention(row, attention);
  row.classList.toggle("collapsed", attention === "idle");
  const title = session.summary
    ? `${session.agent} — ${session.cwd}\n${session.summary}`
    : `${session.agent} — ${session.cwd}`;
  if (row.title !== title) row.title = title;

  const head = row.querySelector<HTMLElement>(".row-head")!;
  fillMascot(head, session, attention !== "idle");
  ensureTerminalIcon(session);
  fillHead(row, session);
  fillTail(row, session, attention);

  const line = row.querySelector<HTMLElement>(".row-line2")!;
  setHidden(line, transcript);
  const prompt = row.querySelector<HTMLElement>(".row-prompt")!;
  setText(prompt, session.summary ? `${strings.session.promptPrefix} ${session.summary}` : shortCwd(session.cwd));
  prompt.classList.toggle("is-cwd", !session.summary);

  const activity = row.querySelector<HTMLElement>(".row-activity")!;
  setHidden(activity, !context.show.activity && !blockedOnUser(session));
  activity.classList.toggle("is-waiting", blockedOnUser(session));
  fillActivity(row.querySelector<HTMLElement>(".row-activity-label")!, session);

  fillChips(row, session);
  fillTasks(row, session);
  fillAgents(row, session);
  fillTranscript(row, session, transcript);
  fillMessageBox(li, session);
}

function toggleDetails(li: HTMLLIElement): void {
  const current = displayedSessions.get(li);
  if (current === undefined) return;
  if (openDetails.has(current.id)) openDetails.delete(current.id);
  else openDetails.add(current.id);
  const row = li.firstElementChild as HTMLElement;
  fillChips(row, current);
  fillTasks(row, current);
  fillAgents(row, current);
  context.syncExpandedSize();
}

function chip(kind: "tasks" | "agents"): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `row-chip row-chip-${kind}`;
  button.dataset.chip = kind;
  button.hidden = true;
  button.setAttribute("aria-expanded", "false");
  if (kind === "agents") {
    const live = document.createElement("span");
    live.className = "chip-live";
    live.setAttribute("aria-hidden", "true");
    button.append(live);
  }
  const text = document.createElement("span");
  text.className = "chip-text";
  button.append(text);
  if (kind === "tasks") {
    const bar = document.createElement("span");
    bar.className = "chip-bar";
    bar.setAttribute("aria-hidden", "true");
    const fill = document.createElement("i");
    fill.className = "chip-fill";
    bar.append(fill);
    button.append(bar);
  }
  return button;
}

export function createRow(session: Session, transcript: boolean): HTMLLIElement {
  const li = document.createElement("li");
  li.dataset.sessionId = session.id;

  const row = document.createElement("div");
  row.className = "session-row";
  row.setAttribute("role", "button");
  row.tabIndex = 0;

  const head = document.createElement("span");
  head.className = "row-head";
  const project = document.createElement("span");
  project.className = "row-project";
  const name = document.createElement("span");
  name.className = "row-name";
  const meta = document.createElement("span");
  meta.className = "row-meta";
  const tail = document.createElement("span");
  tail.className = "row-tail";
  const elapsed = document.createElement("span");
  elapsed.className = "row-elapsed pixel";
  tail.append(elapsed);
  head.append(project, name, meta, tail);

  const line = document.createElement("span");
  line.className = "row-line2";
  const prompt = document.createElement("span");
  prompt.className = "row-prompt";
  const activity = document.createElement("span");
  activity.className = "row-activity";
  const label = document.createElement("span");
  label.className = "row-activity-label";
  activity.append(label);
  line.append(prompt, activity);

  const chips = document.createElement("span");
  chips.className = "row-chips";
  chips.hidden = true;
  chips.append(chip("tasks"), chip("agents"));
  chips.addEventListener("click", (event) => {
    if (!(event.target instanceof Element) || event.target.closest(".row-chip") === null) return;
    event.stopPropagation();
    toggleDetails(li);
  });

  const tasks = document.createElement("span");
  tasks.className = "row-tasks";
  tasks.hidden = true;
  const tasksList = document.createElement("span");
  tasksList.className = "tasks-list";
  tasks.append(tasksList);

  const agents = document.createElement("span");
  agents.className = "row-agents";
  agents.hidden = true;
  const agentsList = document.createElement("span");
  agentsList.className = "agents-list";
  agents.append(agentsList);

  const card = document.createElement("span");
  card.className = "row-transcript";
  card.hidden = true;
  const cardHead = document.createElement("span");
  cardHead.className = "transcript-head";
  const cardPrompt = document.createElement("span");
  cardPrompt.className = "transcript-prompt";
  cardHead.append(cardPrompt);
  const cardBody = document.createElement("span");
  cardBody.className = "transcript-body";
  card.append(cardHead, cardBody);

  row.append(head, line, chips, tasks, agents, card);
  li.append(row, createMessageBox(session.id, { resize: context.syncExpandedSize, error: context.showError, canUseTarget: context.canUseTarget }));
  fillRow(li, session, transcript);
  const jump = (): void => {
    if (li.classList.contains("is-leaving")) return;
    const current = displayedSessions.get(li);
    if (current !== undefined) context.jumpTo(current, row);
  };
  row.addEventListener("click", (event) => {
    if (event.target instanceof Element && event.target.closest(".row-chip") !== null) return;
    jump();
  });
  row.addEventListener("keydown", (event) => {
    if (event.target !== row) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    jump();
  });
  return li;
}

export function tickElapsed(): void {
  if (!context.expanded || !context.visible) return;
  const now = Date.now();
  for (const element of sessionListEl.querySelectorAll<HTMLElement>(
    ".row-elapsed, .agent-elapsed",
  )) {
    const since = Number(element.dataset.since);
    if (Number.isFinite(since)) {
      const text = strings.session.elapsed(now - since);
      if (element.textContent !== text) element.textContent = text;
    }
  }
}
