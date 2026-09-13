export type TerminalKind = "kitty" | "alacritty" | "unknown" | "wezterm" | "ghostty" | "zed" | "code" | "cursor" | "windsurf" | "codium";

export interface QueuedMessage {
  id: number;
  text: string;
  queued_at_ms: number;
}

export interface Task {
  content: string;
  status: "pending" | "in_progress" | "completed" | "cancelled";
}

export interface Session {
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
  completion_id?: string;
}

export type Attention = "waiting_for_input" | "needs_attention" | "working" | "idle";
export type SubagentTiming = "root_responses" | "all_finished" | "every_completion";

export interface PointerState {
  inside: boolean;
  x?: number;
  y?: number;
}

export interface FocusState {
  pid: number | null;
}

export interface QuietScenes {
  active: boolean;
}

export type ApprovalDecision = "allow" | "deny" | "allow_always";

export interface Subagent {
  id: string;
  kind: string;
  description?: string;
  tool?: string;
  summary?: string;
  since_ms?: number;
  done?: boolean;
}

export interface RowVisibility {
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

export interface ApprovalRequest {
  approval_id: string;
  session_id: string;
  tool_name?: string;
  tool_input?: unknown;
  reason?: string;
}

export interface ApprovalResolved {
  approval_id: string;
  session_id: string;
  decision: ApprovalDecision;
}

export interface QuestionOption {
  label: string;
  description?: string;
}

export interface Question {
  question: string;
  header?: string;
  options: QuestionOption[];
  multi_select: boolean;
  custom: boolean;
  id?: string;
}

export interface QuestionRequest {
  question_id: string;
  session_id: string;
  agent: string;
  questions: Question[];
  answerable: boolean;
  expires_in_ms?: number;
}

export interface QuestionResolved {
  question_id: string;
  session_id: string;
  outcome: "answered" | "cancelled" | "expired";
}

export interface Size {
  w: number;
  h: number;
}

export interface UsageWindow {
  key: string;
  label: string;
  percent: number;
  resets_at_ms?: number;
}

export interface UsageModelWindow {
  model: string;
  percent: number;
  resets_at_ms?: number;
}

export interface UsageSnapshot {
  provider: string;
  windows: UsageWindow[];
  models?: UsageModelWindow[];
  reset_cards?: { id: string; title?: string; expires_at_ms?: number }[];
  credits?: { balance: number; unlimited: boolean };
  fetched_at_ms: number;
}

export interface ProviderUsage {
  provider: string;
  detected: boolean;
  snapshot?: UsageSnapshot;
  error?: string;
  checked_at_ms: number;
}

export interface UsageReport {
  providers: ProviderUsage[];
}

export type BadgeSpec = [string, string, string | undefined, string?];

export interface DiffLine {
  sign: "-" | "+";
  text: string;
}
