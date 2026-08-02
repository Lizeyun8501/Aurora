/**
 * TypeScript EventBus implementation.
 *
 * Mirrors the Rust `crates/aurora-core/src/event_bus/event_bus.rs` contract:
 * strongly-typed subscribe/publish/unsubscribe over the {@link CoreEvent}
 * discriminated union. Handlers are invoked synchronously on publish; a
 * throwing handler is isolated (its error is reported via `onError` and
 * does not break sibling handlers).
 *
 * Each `subscribe()` call returns an `unsubscribe` function so callers can
 * bind cleanup to component lifecycles without keeping token objects around.
 */

import type { CoreEvent, CoreEventHandler } from '@aurora/shared-types';

/** Extract the discriminant literal set of `CoreEvent`. */
export type CoreEventType = CoreEvent['type'];

/** A handler filterable by event `type`. */
export type TypedEventHandler<E extends CoreEvent = CoreEvent> = (
  event: E,
) => void;

/** Subscription token returned by `subscribe`. */
export interface Subscription {
  /** Detach this subscription from the bus. */
  unsubscribe: () => void;
  /** Whether this subscription is still attached. */
  readonly active: boolean;
}

export interface EventBusOptions {
  /** Called when a handler throws. Defaults to `console.error`. */
  onError?: (error: unknown, event: CoreEvent) => void;
}

/**
 * A strongly-typed pub/sub bus for {@link CoreEvent}.
 *
 * @example
 * const bus = new EventBus();
 * const sub = bus.subscribe('BlockChanged', (e) => updateLinks(e));
 * bus.publish({ type: 'BlockChanged', doc_id: 'd1', block_id: 'b1', ... });
 * sub.unsubscribe();
 */
export class EventBus {
  private readonly handlers = new Map<
    CoreEventHandler,
    { type: CoreEventType | '*' }
  >();
  private readonly onError: (error: unknown, event: CoreEvent) => void;

  constructor(options: EventBusOptions = {}) {
    this.onError =
      options.onError ??
      ((err) => {
        // eslint-disable-next-line no-console
        console.error('[EventBus] handler error:', err);
      });
  }

  /**
   * Subscribe to a specific event type, or to all events (`'*'`).
   *
   * The typed overload narrows the event payload to the matching variant, so
   * `bus.subscribe('TaskCreated', (e) => e.title)` type-checks.
   *
   * @returns a {@link Subscription} whose `unsubscribe()` detaches the handler.
   */
  subscribe<T extends CoreEventType>(
    type: T,
    handler: (event: Extract<CoreEvent, { type: T }>) => void,
  ): Subscription;
  subscribe(type: '*', handler: CoreEventHandler): Subscription;
  subscribe(
    type: CoreEventType | '*',
    handler: CoreEventHandler,
  ): Subscription {
    this.handlers.set(handler, { type });
    let active = true;
    return {
      unsubscribe: () => {
        if (active) {
          this.handlers.delete(handler);
          active = false;
        }
      },
      get active(): boolean {
        return active;
      },
    };
  }

  /** Detach a previously-attached handler directly. */
  unsubscribe(handler: CoreEventHandler): void {
    this.handlers.delete(handler);
  }

  /** Publish an event to all matching handlers. */
  publish(event: CoreEvent): void {
    for (const [handler, meta] of this.handlers) {
      if (meta.type !== '*' && meta.type !== event.type) continue;
      try {
        handler(event);
      } catch (err) {
        this.onError(err, event);
      }
    }
  }

  /** Number of currently-attached handlers (any type). */
  get listenerCount(): number {
    return this.handlers.size;
  }

  /** Detach every handler. Useful for tests / teardown. */
  clear(): void {
    this.handlers.clear();
  }
}

/**
 * Shared singleton bus used by the interaction controllers.
 * Tests should construct their own `new EventBus()` to keep state isolated.
 */
export const sharedEventBus = new EventBus();
