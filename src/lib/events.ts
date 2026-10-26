import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * Subscribes to a Tauri event and returns a synchronous cleanup. Safe to call
 * from effects: cleanup that runs before `listen` resolves still unlistens.
 */
export function listenWhileMounted<T>(event: string, handler: EventCallback<T>): () => void {
  const pending = listen<T>(event, handler);
  return () => {
    void pending.then((off) => off());
  };
}

/** Register the listener before subscribing so a replayed snapshot is not lost. */
export function listenThen<T>(
  event: string,
  handler: (payload: T) => void,
  afterListen: () => Promise<void>,
): () => void {
  let cancelled = false;
  let unlisten: UnlistenFn | undefined;
  const pending = listen<T>(event, (e) => handler(e.payload));
  void pending.then(async (off) => {
    if (cancelled) {
      off();
      return;
    }
    unlisten = off;
    await afterListen();
  });
  return () => {
    cancelled = true;
    if (unlisten) unlisten();
    else void pending.then((off) => off());
  };
}
