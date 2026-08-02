/**
 * SubTask 5.4.4 — Sync cross-cutting concern.
 *
 * The {@link SyncOrchestrator} subscribes to ALL {@link CoreEvent}s on the
 * {@link EventBus}, enqueues each event into a local offline sync queue, and
 * dispatches the queue to configured sync targets (P2P / Cloud / LAN).
 *
 * Dispatch is mock: each target receives the queue via a pluggable
 * `SyncTargetDispatcher` function. Real implementations wire this to the
 * Rust sync engine (`crates/aurora-sync`).
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import type { CoreEvent, SyncTargetKind } from '@aurora/shared-types';
import { EventBus, type Subscription } from './eventBus';

/** A queued sync item: the event plus enqueue metadata. */
export interface SyncQueueItem {
  /** Monotonic sequence number. */
  seq: number;
  /** When the item was enqueued (epoch ms). */
  enqueuedAt: number;
  /** The originating event. */
  event: CoreEvent;
}

/** Status of a dispatch attempt against a target. */
export type DispatchStatus = 'pending' | 'dispatched' | 'failed';

/** A dispatcher for a single sync target kind. */
export interface SyncTargetDispatcher {
  readonly kind: SyncTargetKind;
  /** Dispatch a batch of queue items; return the items' statuses in order. */
  dispatch(items: SyncQueueItem[]): Promise<DispatchStatus[]>;
}

/** A no-op dispatcher used as the default (records calls but does nothing). */
export class MockSyncDispatcher implements SyncTargetDispatcher {
  readonly kind: SyncTargetKind;
  readonly calls: SyncQueueItem[][] = [];

  constructor(kind: SyncTargetKind) {
    this.kind = kind;
  }

  async dispatch(items: SyncQueueItem[]): Promise<DispatchStatus[]> {
    this.calls.push(items);
    return items.map(() => 'dispatched' satisfies DispatchStatus);
  }
}

/** Sync queue surface (mirrors a sliver of `aurora-sync::offline_queue`). */
export interface SyncQueueStore {
  enqueue(item: SyncQueueItem): void;
  drain(): SyncQueueItem[];
  size(): number;
  peekAll(): readonly SyncQueueItem[];
}

/** In-memory FIFO {@link SyncQueueStore}. */
export class InMemorySyncQueue implements SyncQueueStore {
  private readonly items: SyncQueueItem[] = [];

  enqueue(item: SyncQueueItem): void {
    this.items.push(item);
  }

  drain(): SyncQueueItem[] {
    const out = this.items.splice(0, this.items.length);
    return out;
  }

  size(): number {
    return this.items.length;
  }

  peekAll(): readonly SyncQueueItem[] {
    return this.items;
  }
}

export interface SyncOrchestratorOptions {
  bus?: EventBus;
  queue?: SyncQueueStore;
  /** Dispatchers keyed by target kind. Defaults to P2P + Cloud + LAN mocks. */
  dispatchers?: SyncTargetDispatcher[];
  /** Whether to auto-dispatch on every enqueue (default `false`). */
  autoDispatch?: boolean;
}

/**
 * Orchestrates cross-cutting sync: subscribe to all events, enqueue, dispatch.
 */
export class SyncOrchestrator {
  private readonly bus: EventBus;
  private readonly queue: SyncQueueStore;
  private readonly dispatchers: SyncTargetDispatcher[];
  private readonly autoDispatch: boolean;
  private readonly sub: Subscription;
  private seq = 0;

  constructor(options: SyncOrchestratorOptions = {}) {
    this.bus = options.bus ?? new EventBus();
    this.queue = options.queue ?? new InMemorySyncQueue();
    this.dispatchers =
      options.dispatchers ??
      ([
        new MockSyncDispatcher('p2p'),
        new MockSyncDispatcher('cloud'),
        new MockSyncDispatcher('lan'),
      ] as SyncTargetDispatcher[]);
    this.autoDispatch = options.autoDispatch ?? false;

    this.sub = this.bus.subscribe('*', (event) => {
      // Skip our own `SyncProgress` emissions to avoid a feedback loop
      // (dispatch emits SyncProgress, which would otherwise re-enqueue and
      // re-dispatch indefinitely when autoDispatch is on).
      if (event.type === 'SyncProgress') return;
      this.enqueue(event);
    });
  }

  get eventBus(): EventBus {
    return this.bus;
  }

  get syncQueue(): SyncQueueStore {
    return this.queue;
  }

  /** Registered dispatchers. */
  get targets(): readonly SyncTargetDispatcher[] {
    return this.dispatchers;
  }

  /** Enqueue an event and optionally publish a SyncProgress event. */
  enqueue(event: CoreEvent): SyncQueueItem {
    const item: SyncQueueItem = {
      seq: ++this.seq,
      enqueuedAt: Date.now(),
      event,
    };
    this.queue.enqueue(item);
    if (this.autoDispatch) {
      void this.dispatch();
    }
    return item;
  }

  /** Current queue depth. */
  pendingCount(): number {
    return this.queue.size();
  }

  /**
   * Drain the queue and dispatch the batch to every target. Emits a
   * `SyncProgress` event per target.
   */
  async dispatch(): Promise<void> {
    const batch = this.queue.drain();
    if (batch.length === 0) return;
    for (const target of this.dispatchers) {
      const statuses = await target.dispatch(batch);
      const failed = statuses.filter((s) => s === 'failed').length;
      const progress =
        failed === 0 ? 1 : (batch.length - failed) / batch.length;
      const event: CoreEvent = {
        type: 'SyncProgress',
        target_id: target.kind,
        progress,
      };
      this.bus.publish(event);
    }
  }

  dispose(): void {
    this.sub.unsubscribe();
  }
}

/** React hook return shape. */
export interface UseSyncOrchestrator {
  /** Number of items currently pending dispatch. */
  pending: number;
  /** Last reported per-target progress (0..1). */
  progress: Record<string, number>;
  /** Force a dispatch cycle. */
  dispatch: () => Promise<void>;
}

/**
 * Expose the sync orchestrator state to React.
 */
export function useSyncOrchestrator(
  orchestrator?: SyncOrchestrator,
): UseSyncOrchestrator {
  const ref = useRef<SyncOrchestrator | null>(orchestrator ?? null);
  if (!ref.current) {
    ref.current = new SyncOrchestrator();
  }
  const orch = ref.current;

  const [pending, setPending] = useState<number>(() => orch.pendingCount());
  const [progress, setProgress] = useState<Record<string, number>>({});

  useEffect(() => {
    const onProgress = orch.eventBus.subscribe('SyncProgress', (event) => {
      if (event.type !== 'SyncProgress') return;
      setProgress((prev) => ({ ...prev, [event.target_id]: event.progress }));
      setPending(orch.pendingCount());
    });
    // Refresh pending count on any event enqueue.
    const onAny = orch.eventBus.subscribe('*', () => {
      setPending(orch.pendingCount());
    });
    return () => {
      onProgress.unsubscribe();
      onAny.unsubscribe();
    };
  }, [orch]);

  return useMemo(
    () => ({
      pending,
      progress,
      dispatch: () => orch.dispatch(),
    }),
    [pending, progress, orch],
  );
}
