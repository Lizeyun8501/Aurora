/**
 * Capacitor bridge call wrapper.
 *
 * Wraps the Capacitor `Bridge`/plugin-call pattern into a typed surface.
 * Each plugin call is represented as a `BridgeCall<Args, Result>` so the
 * mobile adapter can dispatch through a single, mockable seam.
 *
 * When the Capacitor runtime is not present (running the mobile TS outside a
 * native host), the bridge falls back to a mock that records calls and
 * resolves configurable results — this keeps the adapter typecheckable and
 * unit-testable without a device.
 */

/** Generic plugin argument map. */
export type BridgeArgs = Record<string, unknown>;

/** A typed Capacitor plugin call. */
export interface BridgeCall<Args extends BridgeArgs, Result> {
  readonly plugin: string;
  readonly method: string;
  (args?: Args): Promise<Result>;
}

/** A recorded mock bridge call. */
export interface RecordedBridgeCall {
  plugin: string;
  method: string;
  args: BridgeArgs | undefined;
}

/** A pluggable bridge backend (in production this is `Capacitor.Plugins`). */
export interface BridgeBackend {
  call<Args extends BridgeArgs, Result>(
    plugin: string,
    method: string,
    args?: Args,
  ): Promise<Result>;
}

/** A mock bridge backend that records calls and resolves a configurable value. */
export class MockBridgeBackend implements BridgeBackend {
  readonly calls: RecordedBridgeCall[] = [];
  private readonly resolver: (
    plugin: string,
    method: string,
    args: BridgeArgs | undefined,
  ) => unknown;

  constructor(
    resolver: (
      plugin: string,
      method: string,
      args: BridgeArgs | undefined,
    ) => unknown = () => undefined,
  ) {
    this.resolver = resolver;
  }

  async call<Args extends BridgeArgs, Result>(
    plugin: string,
    method: string,
    args?: Args,
  ): Promise<Result> {
    this.calls.push({ plugin, method, args });
    return this.resolver(plugin, method, args) as Result;
  }
}

/**
 * The Capacitor bridge. Holds a pluggable {@link BridgeBackend}; defaults to a
 * {@link MockBridgeBackend} until real Capacitor APIs are loaded via
 * {@link initCapacitorBridge}.
 */
export class CapacitorBridge {
  private backend: BridgeBackend;

  constructor(backend?: BridgeBackend) {
    this.backend = backend ?? new MockBridgeBackend();
  }

  /** Issue a raw bridge call. */
  call<Args extends BridgeArgs, Result>(
    plugin: string,
    method: string,
    args?: Args,
  ): Promise<Result> {
    return this.backend.call<Args, Result>(plugin, method, args);
  }

  /** Bind a typed `BridgeCall` for a specific plugin/method. */
  bind<Args extends BridgeArgs, Result>(
    plugin: string,
    method: string,
  ): BridgeCall<Args, Result> {
    const call: BridgeCall<Args, Result> = Object.assign(
      async (args?: Args): Promise<Result> =>
        this.backend.call<Args, Result>(plugin, method, args),
      { plugin, method },
    );
    return call;
  }

  /** Replace the backend (used by tests / mock injection). */
  setBackend(backend: BridgeBackend): void {
    this.backend = backend;
  }
}

/** Shared default bridge. Mocks until real Capacitor APIs are loaded. */
export const capacitorBridge = new CapacitorBridge();

/**
 * Attempt to load the real Capacitor core runtime.
 *
 * The module specifier is intentionally non-literal so TypeScript does not
 * require `@capacitor/core` to be present at compile time. Returns `true`
 * when the Capacitor global is available and the bridge could be wired.
 */
export async function initCapacitorBridge(): Promise<boolean> {
  const spec = '@capacitor/core' as string;
  try {
    const core = (await import(spec)) as {
      Capacitor?: { isNativePlatform?: () => boolean };
    };
    return Boolean(core.Capacitor?.isNativePlatform?.());
  } catch {
    return false;
  }
}
