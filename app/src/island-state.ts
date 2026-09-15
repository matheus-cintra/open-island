import type { ActionIdentity, Delivery, UiSnapshot } from "./daemon-state";
import type { ApprovalRequest, QuestionRequest, Session } from "./types";

function identity(epoch: string, instance: string | null): ActionIdentity | undefined {
  return instance === null ? undefined : { daemon_epoch: epoch, session_instance_id: instance };
}
export function detachedDeliveries(snapshot: UiSnapshot): Delivery[] {
  return snapshot.message_deliveries.filter((message): boolean => message.identity?.daemon_epoch === snapshot.daemon_epoch &&
    !snapshot.sessions.some((session): boolean => session.id === message.session_id && session.session_instance_id === message.identity?.session_instance_id));
}
export function snapshotSessions(snapshot: UiSnapshot, children = false): Session[] {
  return (children ? snapshot.child_sessions ?? [] : snapshot.sessions).map((session): Session => ({
    ...session,
    action_identity: identity(snapshot.daemon_epoch, session.session_instance_id),
    message_deliveries: snapshot.message_deliveries.filter((message) => message.session_id === session.id && message.identity?.daemon_epoch === snapshot.daemon_epoch && message.identity.session_instance_id === session.session_instance_id),
    queued_messages: snapshot.message_deliveries.filter((message): boolean => message.session_id === session.id && message.state === "queued" && message.identity?.daemon_epoch === snapshot.daemon_epoch && message.identity.session_instance_id === session.session_instance_id)
      .map((message) => ({ id: message.message_id, text: message.text, queued_at_ms: message.queued_at_ms })),
  }));
}
export function snapshotApprovals(snapshot: UiSnapshot): ApprovalRequest[] {
  return snapshot.approvals.map((approval): ApprovalRequest => ({ ...approval, action_identity: identity(snapshot.daemon_epoch, approval.session_instance_id) }));
}
export function snapshotQuestions(snapshot: UiSnapshot): QuestionRequest[] {
  return snapshot.questions.map((question): QuestionRequest => ({ ...question, action_identity: identity(snapshot.daemon_epoch, question.session_instance_id) }));
}
export function pendingKey(pending: ApprovalRequest | QuestionRequest | null): string {
  if (pending === null) return "";
  return JSON.stringify(["approval_id" in pending ? pending.approval_id : pending.question_id, pending.pending_generation, pending.action_identity]);
}
