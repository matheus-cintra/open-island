import type { ActionIdentity, Delivery, UiSnapshot } from "./daemon-state";
import type { ApprovalRequest, QuestionRequest, Session } from "./types";

function identity(epoch: string, instance: string | null): ActionIdentity | undefined {
  return instance === null ? undefined : { daemon_epoch: epoch, session_instance_id: instance };
}
function deliveryKey(sessionId: string, instanceId: string | null): string {
  return `${sessionId}\u0000${instanceId}`;
}
function deliveriesBySession(snapshot: UiSnapshot): Map<string, Delivery[]> {
  const index = new Map<string, Delivery[]>();
  for (const message of snapshot.message_deliveries) {
    if (message.identity?.daemon_epoch !== snapshot.daemon_epoch) continue;
    const key = deliveryKey(message.session_id, message.identity.session_instance_id);
    const list = index.get(key);
    if (list === undefined) index.set(key, [message]);
    else list.push(message);
  }
  return index;
}
export function detachedDeliveries(snapshot: UiSnapshot): Delivery[] {
  const attached = new Set(snapshot.sessions.map((session): string => deliveryKey(session.id, session.session_instance_id)));
  return snapshot.message_deliveries.filter((message): boolean => message.identity?.daemon_epoch === snapshot.daemon_epoch &&
    !attached.has(deliveryKey(message.session_id, message.identity.session_instance_id)));
}
export function snapshotSessions(snapshot: UiSnapshot, children = false): Session[] {
  const index = deliveriesBySession(snapshot);
  return (children ? snapshot.child_sessions ?? [] : snapshot.sessions).map((session): Session => {
    const mine = session.session_instance_id === null
      ? []
      : index.get(deliveryKey(session.id, session.session_instance_id)) ?? [];
    return {
      ...session,
      action_identity: identity(snapshot.daemon_epoch, session.session_instance_id),
      message_deliveries: mine,
      queued_messages: mine.filter((message): boolean => message.state === "queued")
        .map((message) => ({ id: message.message_id, text: message.text, queued_at_ms: message.queued_at_ms })),
    };
  });
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
