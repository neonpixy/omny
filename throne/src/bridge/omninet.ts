/**
 * Tauri bridge for window.omninet.
 *
 * Serves two consumers:
 *   Castle (scoper):  run({ method, params }) → returns result directly, throws on error
 *   Legacy (_shared): run(jsonString)         → returns parsed {ok, result} response
 *
 * Both route through Tauri invoke → Rust backend → Chancellor IPC.
 */
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

type EventHandler = (data: unknown) => void;

// Track listeners for off() compat
const activeListeners = new Map<string, { handler: EventHandler; unlisten: UnlistenFn }[]>();

const bridge = {
  /**
   * Execute an operation via the daemon.
   *
   * Castle format: run({ method: 'chamberlain.state', params: {} })
   *   Returns the operation result directly. Throws on error.
   *
   * Legacy format: run('{"steps":[{"op":"chamberlain.state","input":{}}]}')
   *   Returns the full parsed response {ok, result/error}.
   */
  async run(input: string | { method: string; params: Record<string, unknown> }): Promise<unknown> {
    if (typeof input === 'string') {
      // Legacy: pass through pipeline JSON, return parsed response
      const raw = await invoke<string>('omninet_run', { pipeline: input });
      return JSON.parse(raw);
    }

    // Castle: single operation → wrap in pipeline, extract result
    const pipeline = JSON.stringify({
      steps: [{ op: input.method, input: input.params }],
    });
    const raw = await invoke<string>('omninet_run', { pipeline });
    const response = JSON.parse(raw);
    console.log(`[bridge] ${input.method}:`, response.ok ? response.result : response);
    if (!response.ok) {
      throw new Error(response.error ?? 'Operation failed');
    }
    return response.result;
  },

  /**
   * Call a platform capability.
   * Returns the full parsed response {ok, result/error}.
   */
  async platform(op: string, input?: string | Record<string, unknown>): Promise<unknown> {
    const inputJson = typeof input === 'string' ? input : JSON.stringify(input ?? {});
    const raw = await invoke<string>('omninet_platform', { op, input: inputJson });
    return JSON.parse(raw);
  },

  /** Discover available platform capabilities. */
  async capabilities(): Promise<string> {
    return JSON.stringify([]);
  },

  /**
   * Subscribe to a push event from the daemon.
   * Returns a cleanup function to unsubscribe.
   */
  on(event: string, handler: EventHandler): () => void {
    const eventName = `omninet:${event.replaceAll('.', '/')}`;
    console.log(`[bridge] on('${event}') → listening for '${eventName}'`);
    let unlistenFn: UnlistenFn | null = null;
    let removed = false;

    listen(eventName, (e) => {
      console.log(`[bridge] ← event '${eventName}':`, typeof e.payload === 'object' ? JSON.stringify(e.payload).slice(0, 200) : e.payload);
      handler(e.payload);
    }).then((unlisten) => {
      if (removed) {
        // Cleanup was called before listener registered — tear down immediately
        unlisten();
        return;
      }
      unlistenFn = unlisten;
      if (!activeListeners.has(event)) {
        activeListeners.set(event, []);
      }
      activeListeners.get(event)!.push({ handler, unlisten });
    });

    return () => {
      removed = true;
      if (unlistenFn) {
        unlistenFn();
        const listeners = activeListeners.get(event);
        if (listeners) {
          const idx = listeners.findIndex((l) => l.handler === handler);
          if (idx !== -1) listeners.splice(idx, 1);
        }
      }
    };
  },

  /** Unsubscribe from a push event (legacy — prefer the return value of on()). */
  off(event: string, handler: EventHandler): void {
    const listeners = activeListeners.get(event);
    if (!listeners) return;
    const idx = listeners.findIndex((l) => l.handler === handler);
    if (idx !== -1) {
      listeners[idx].unlisten();
      listeners.splice(idx, 1);
    }
  },
};

// Install on window before any program code loads
(window as any).omninet = Object.freeze(bridge);

export default bridge;
