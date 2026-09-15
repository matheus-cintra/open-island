import type { UiCache, UiSession, UiSnapshot } from "../daemon-state";
import type { ApprovalRequest, Question, QuestionRequest, Session, UsageReport } from "../types";
type QuestionInput = Omit<QuestionRequest, "questions"> & { questions: (Omit<Question, "multi_select" | "custom"> & Partial<Pick<Question, "multi_select" | "custom">>)[] };

export class SnapshotFixture {
  private value: UiSnapshot = {
    schema_version: 1, daemon_epoch: "fixture-epoch", publication_revision: 1,
    sessions: [], child_sessions: [], approvals: [], questions: [], message_deliveries: [],
    config: {}, usage: { providers: [] }, update: null,
    quiet_scenes: { active: false, focus_mode: false, screen_off: false },
  };
  constructor(private readonly emit: (cache: UiCache) => void) {}
  read(): UiCache {
    return structuredClone({ phase: "connected", generation: 1, snapshot: { generation: 1, snapshot: this.value } });
  }
  private publish(): void { this.value.publication_revision += 1; this.emit(this.read()); }
  sessions(sessions: Session[]): void {
    this.value.sessions = sessions.map((session): UiSession => ({ ...session, session_instance_id: session.action_identity?.session_instance_id ?? `instance:${session.id}` }));
    this.publish();
  }
  children(sessions: Session[]): void {
    this.value.child_sessions = sessions.map((session): UiSession => ({ ...session, session_instance_id: session.action_identity?.session_instance_id ?? `instance:${session.id}` }));
    this.publish();
  }
  config(config: Record<string, unknown>): void { this.value.config = config; this.publish(); }
  usage(usage: UsageReport): void { this.value.usage = usage; this.publish(); }
  update(update: { version: string } | null): void { this.value.update = update; this.publish(); }
  quiet(active: boolean): void { this.value.quiet_scenes.active = active; this.publish(); }
  approval(approval: ApprovalRequest): void {
    const index = this.value.approvals.findIndex((item): boolean => item.approval_id === approval.approval_id);
    const item = { ...approval, session_instance_id: `instance:${approval.session_id}`, pending_generation: approval.pending_generation ?? 1 };
    if (index < 0) this.value.approvals.push(item); else this.value.approvals[index] = item;
    this.publish();
  }
  resolveApproval(approvalId: string): void {
    this.value.approvals = this.value.approvals.filter((item): boolean => item.approval_id !== approvalId);
    this.publish();
  }
  clearQuestions(): void { this.value.questions = []; this.publish(); }
  question(question: QuestionInput): void {
    const index = this.value.questions.findIndex((item): boolean => item.question_id === question.question_id);
    const item = { ...question, questions: question.questions.map((question): Question => ({ multi_select: false, custom: false, ...question })), session_instance_id: `instance:${question.session_id}`, pending_generation: question.pending_generation ?? 1 };
    if (index < 0) this.value.questions.push(item); else this.value.questions[index] = item;
    this.publish();
  }
  resolveQuestion(questionId: string): void {
    this.value.questions = this.value.questions.filter((item): boolean => item.question_id !== questionId);
    this.publish();
  }
}
