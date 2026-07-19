// Mock of @tauri-apps/api/event `listen` for the mock build. A tiny in-process
// event bus; `emit` is used by core.ts to fake `site-event` progress.

export type UnlistenFn = () => void;

type Handler<T> = (event: { payload: T }) => void;

const handlers = new Map<string, Set<Handler<unknown>>>();

export async function listen<T>(event: string, cb: Handler<T>): Promise<UnlistenFn> {
  let set = handlers.get(event);
  if (!set) handlers.set(event, (set = new Set()));
  set.add(cb as Handler<unknown>);
  return () => set.delete(cb as Handler<unknown>);
}

export function emit(event: string, payload: unknown): void {
  handlers.get(event)?.forEach((cb) => cb({ payload }));
}
