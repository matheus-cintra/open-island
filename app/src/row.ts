import { invoke } from "@tauri-apps/api/core";
import { strings } from "./strings";
import { SESSION_ICONS } from "./session-icons";
import { createSprite, spriteAgent } from "./sprites";
import {
  compactCount,
  compactLabel,
  compactProject,
  compactRowEl,
  compactSpriteEl,
  compactTail,
  sessionListEl,
} from "./elements";
import { createMessageBox, fillMessageBox } from "./message";
import { Attention, BadgeSpec, Session, Subagent, Task } from "./types";
import {
  compactClean,
  expanded,
  jumpTo,
  LEAVE_MS,
  reducedMotion,
  render,
  sessions,
  show,
  syncExpandedSize,
} from "./main";

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

export function renderCompact(n: number): void {
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

export function renderList(): void {
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
export function tickElapsed(): void {
  if (!expanded) return;
  const now = Date.now();
  for (const element of sessionListEl.querySelectorAll<HTMLElement>(
    ".row-badge-elapsed, .agent-elapsed",
  )) {
    const since = Number(element.dataset.since);
    if (Number.isFinite(since)) element.textContent = strings.session.elapsed(now - since);
  }
}
