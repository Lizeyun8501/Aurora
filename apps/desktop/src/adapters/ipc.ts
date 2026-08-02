/**
 * Tauri v2 IPC bridge.
 *
 * Wraps `@tauri-apps/api/core` `invoke` and `@tauri-apps/api/event` `listen`
 * into a typed surface. When the Tauri runtime is not present (e.g. running
 * the desktop TS in a plain Node/browser context for tests), the bridge falls
 * back to a mock that records invocations and resolves empty results so the
 * adapter layer remains testable without a Tauri host.
 */

import type { CoreEvent } from '@aurora/shared-types';

/** Generic command argument map. */
export type CommandArgs = Record<string, unknown>;

/** A recorded mock invocation (only populated when running mocked). */
export interface RecordedInvocation {
  command: string;
  args: CommandArgs;
}

/**
 * A pluggable invoke backend. In production this is the Tauri `invoke`
 * function; in tests it is replaced with a mock.
 */
export interface InvokeBackend {
  <T>(command: string, args?: CommandArgs): Promise<T>;
}

/** A recorded mock event emission. */
export interface RecordedEmission {
  event: string;
  payload: unknown;
}

/** A pluggable emit backend. */
export interface EmitBackend {
  (event: string, payload?: unknown): Promise<void>;
}

/** An unsubscribe function returned by `listen`. */
export type Unlisten = () => void;

/** A listen backend. */
export interface ListenBackend {
  (event: string, handler: (payload: unknown) => void): Promise<Unlisten>;
}

/** Mock invoke that records calls and resolves a configurable value. */
export function createMockInvoke(
  resolver: (command: string, args: CommandArgs) => unknown = () => undefined,
): { backend: InvokeBackend; calls: RecordedInvocation[] } {
  const calls: RecordedInvocation[] = [];
  const backend: InvokeBackend = async <T>(
    command: string,
    args: CommandArgs = {},
  ): Promise<T> => {
    calls.push({ command, args });
    return resolver(command, args) as T;
  };
  return { backend, calls };
}

/** Mock emit that records emissions. */
export function createMockEmit(): { backend: EmitBackend; calls: RecordedEmission[] } {
  const calls: RecordedEmission[] = [];
  const backend: EmitBackend = async (event: string, payload?: unknown) => {
    calls.push({ event, payload });
  };
  return { backend, calls };
}

/** Mock listen that immediately resolves a no-op unlisten. */
export function createMockListen(): {
  backend: ListenBackend;
  handlers: Map<string, Array<(payload: unknown) => void>>;
} {
  const handlers = new Map<string, Array<(payload: unknown) => void>>();
  const backend: ListenBackend = async (
    event: string,
    handler: (payload: unknown) => void,
  ) => {
    const list = handlers.get(event) ?? [];
    list.push(handler);
    handlers.set(event, list);
    return () => {
      const arr = handlers.get(event);
      if (!arr) return;
      const idx = arr.indexOf(handler);
      if (idx >= 0) arr.splice(idx, 1);
    };
  };
  return { backend, handlers };
}

/**
 * Tauri IPC bridge.
 *
 * Holds pluggable invoke/emit/listen backends. By default it tries to use the
 * real Tauri APIs (lazy-loaded); if they are unavailable it falls back to
 * mocks so the adapter remains usable in non-Tauri contexts.
 */
export class TauriIpcBridge {
  private invokeImpl: InvokeBackend;
  private emitImpl: EmitBackend;
  private listenImpl: ListenBackend;

  constructor(options?: {
    invoke?: InvokeBackend;
    emit?: EmitBackend;
    listen?: ListenBackend;
  }) {
    this.invokeImpl = options?.invoke ?? createMockInvoke().backend;
    this.emitImpl = options?.emit ?? createMockEmit().backend;
    this.listenImpl = options?.listen ?? createMockListen().backend;
  }

  /** Invoke a Tauri command. */
  invoke<T>(command: string, args?: CommandArgs): Promise<T> {
    return this.invokeImpl<T>(command, args);
  }

  /** Emit an event to the Tauri backend / other windows. */
  emit(event: string, payload?: unknown): Promise<void> {
    return this.emitImpl(event, payload);
  }

  /** Subscribe to a Tauri event. Returns an unlisten function. */
  async listen(
    event: string,
    handler: (payload: unknown) => void,
  ): Promise<Unlisten> {
    return this.listenImpl(event, handler);
  }

  /**
   * Subscribe to a Tauri event carrying a {@link CoreEvent} payload.
   * Convenience wrapper that narrows the payload type.
   */
  async listenCoreEvent(
    event: string,
    handler: (event: CoreEvent) => void,
  ): Promise<Unlisten> {
    return this.listenImpl(event, (payload) => {
      handler(payload as CoreEvent);
    });
  }

  /** Replace the invoke backend (used by tests / mock injection). */
  setInvokeBackend(backend: InvokeBackend): void {
    this.invokeImpl = backend;
  }

  /** Replace the emit backend. */
  setEmitBackend(backend: EmitBackend): void {
    this.emitImpl = backend;
  }

  /** Replace the listen backend. */
  setListenBackend(backend: ListenBackend): void {
    this.listenImpl = backend;
  }
}

/**
 * Lazily attempt to load the real Tauri v2 `invoke`/`event` APIs.
 *
 * Returns `null` when the runtime is not a Tauri host (so callers can fall
 * back to a mock). The module specifiers are intentionally non-literal
 * (`as string`) so TypeScript does not require the packages to be present at
 * compile time — the adapter typechecks whether or not the native Tauri
 * packages are installed, and falls back to web mocks at runtime when the
 * dynamic import rejects.
 */
export async function tryLoadTauriApis(): Promise<{
  invoke: InvokeBackend | null;
  emit: EmitBackend | null;
  listen: ListenBackend | null;
}> {
  const coreSpec = '@tauri-apps/api/core' as string;
  const eventSpec = '@tauri-apps/api/event' as string;
  try {
    const core = (await import(coreSpec)) as {
      invoke?: InvokeBackend;
    };
    const event = (await import(eventSpec)) as {
      emit?: EmitBackend;
      listen?: ListenBackend;
    };
    return {
      invoke: core.invoke ?? null,
      emit: event.emit ?? null,
      listen: event.listen ?? null,
    };
  } catch {
    return { invoke: null, emit: null, listen: null };
  }
}

/** Shared default IPC bridge. Uses mocks until real Tauri APIs are loaded. */
export const ipcBridge = new TauriIpcBridge();

/**
 * Wire the shared {@link ipcBridge} to the real Tauri APIs when available.
 * Safe to call multiple times.
 */
export async function initTauriIpc(): Promise<boolean> {
  const apis = await tryLoadTauriApis();
  if (apis.invoke) ipcBridge.setInvokeBackend(apis.invoke);
  if (apis.emit) ipcBridge.setEmitBackend(apis.emit);
  if (apis.listen) ipcBridge.setListenBackend(apis.listen);
  return apis.invoke !== null;
}
