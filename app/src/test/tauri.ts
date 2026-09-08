type InvokeArgs = Record<string, unknown>;
type EventHandler = (event: { payload: unknown }) => void;

export interface InvokeCall {
  command: string;
  args: InvokeArgs;
}

export interface TauriMock {
  invoke: (command: string, args?: InvokeArgs) => Promise<unknown>;
  listen: (name: string, handler: EventHandler) => Promise<() => void>;
  calls: InvokeCall[];
  emit: (name: string, payload: unknown) => void;
}

export function tauriMock(handle: (call: InvokeCall) => unknown = () => ({})): TauriMock {
  const calls: InvokeCall[] = [];
  const handlers = new Map<string, EventHandler>();
  return {
    calls,
    invoke: async (command, args = {}) => {
      const call = { command, args };
      calls.push(call);
      return handle(call);
    },
    listen: async (name, handler) => {
      handlers.set(name, handler);
      return () => handlers.delete(name);
    },
    emit: (name, payload) => {
      const handler = handlers.get(name);
      if (handler === undefined) throw new Error(`nothing is listening to "${name}"`);
      handler({ payload });
    },
  };
}
