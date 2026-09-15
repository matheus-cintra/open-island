import type { Session, SubagentTiming } from "./types";

function key(session: Session): string {
  return JSON.stringify([session.id, session.action_identity?.daemon_epoch, session.action_identity?.session_instance_id]);
}

interface Completion { attention: string; id?: string; }
/** Input must be the ordered, accepted daemon snapshots, never raw transport events. */
export class CompletionEdges {
  private previous = new Map<string, Completion>();
  get retained(): number { return this.previous.size; }
  observe(sessions: Session[], hydrate = false): boolean {
    if (hydrate) this.previous.clear();
    const next = new Map<string, Completion>();
    let fired = false;
    for (const session of sessions) {
      const identity = key(session);
      const before = this.previous.get(identity);
      const id = session.completion_id;
      if (before) {
        if (id) fired ||= id !== before.id && session.attention !== "working";
        else fired ||= before.attention === "working" && session.attention === "needs_attention";
      }
      // The daemon issues a fresh opaque token for each accepted Stop. Keep the last
      // token across working snapshots, which intentionally clear completion_id.
      next.set(identity, { attention: session.attention ?? "working", id: id || before?.id });
    }
    this.previous = next;
    return fired;
  }
}

export class SubagentEdges {
  private previous = new Map<string, Set<string>>();
  get retained(): number { return this.previous.size; }
  observe(sessions: Session[], timing: SubagentTiming, hydrate = false): boolean {
    if (hydrate) this.previous.clear();
    const next = new Map<string, Set<string>>();
    let fired = false;
    for (const session of sessions) {
      const identity = key(session);
      const done = new Set((session.subagents ?? []).filter((agent): boolean => agent.done === true).map((agent): string => agent.id));
      next.set(identity, done);
      const before = this.previous.get(identity);
      if (timing === "root_responses" || !before) continue;
      const arrived = [...done].some((id): boolean => !before.has(id));
      if (arrived && (timing === "every_completion" || done.size === (session.subagents ?? []).length)) fired = true;
    }
    this.previous = next;
    return fired;
  }
}
