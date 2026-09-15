import { SnapshotFixture } from "./snapshot-fixture";
import { paintFrame } from "./dom";
type InvokeArgs = Record<string, unknown>;
type EventHandler = (event: { payload: unknown }) => void;

export interface InvokeCall {
  command: string;
  args: InvokeArgs;
}

export interface TauriMock {
  state: SnapshotFixture;
  invoke: (command: string, args?: InvokeArgs) => Promise<unknown>;
  listen: (name: string, handler: EventHandler) => Promise<() => void>;
  calls: InvokeCall[];
  /** Event followed by one simulated browser paint, for ordinary interaction tests. */
  emit: (name: string, payload: unknown) => void;
  /** Delivers state without advancing RAF, for burst and pre-paint guard tests. */
  emitBeforePaint: (name: string, payload: unknown) => void;
}

export function tauriMock(handle: (call: InvokeCall) => unknown = () => ({})): TauriMock {
  const calls: InvokeCall[] = [];
  const handlers = new Map<string, EventHandler>();
  const emitBeforePaint = (name: string, payload: unknown): void => { handlers.get(name)?.({ payload }); };
  const emit = (name: string, payload: unknown): void => { emitBeforePaint(name, payload); paintFrame(); };
  const state = new SnapshotFixture((cache): void => { emit("daemon-ui-state", cache); });
  return {
    state,
    calls,
    invoke: async (command, args = {}) => {
      const call = { command, args };
      calls.push(call);
      const result = handle(call);
      if (command === "get_daemon_ui_state" && result && typeof result === "object" && !("phase" in result)) return state.read();
      return result;
    },
    listen: async (name, handler) => {
      handlers.set(name, handler);
      return () => handlers.delete(name);
    },
    emit, emitBeforePaint,
  };
}
