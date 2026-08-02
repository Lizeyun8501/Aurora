import { useEffect, useState } from 'react';
import type { SyncStatus } from '@aurora/shared-types';

export interface SyncState {
  status: SyncStatus;
  progress: number;
  lastSyncedAt: string | null;
}

export type SyncListener = (state: SyncState) => void;

/* -------------------------------------------------------------------------- */
/* Mock sync source (subscribe callback pattern).                             */
/* -------------------------------------------------------------------------- */

let currentState: SyncState = {
  status: 'idle',
  progress: 0,
  lastSyncedAt: null,
};

const listeners = new Set<SyncListener>();

function emit(): void {
  listeners.forEach((l) => l(currentState));
}

function setState(next: SyncState): void {
  currentState = next;
  emit();
}

/** Subscribe to sync-state changes. Immediately emits the current state. */
export function subscribeSync(listener: SyncListener): () => void {
  listeners.add(listener);
  listener(currentState);
  return () => {
    listeners.delete(listener);
  };
}

/** Triggers a mock sync run: `syncing` → `idle` after a short delay. */
export function syncNow(): void {
  setState({
    status: 'syncing',
    progress: 0,
    lastSyncedAt: currentState.lastSyncedAt,
  });
  setTimeout(() => {
    setState({
      status: 'idle',
      progress: 1,
      lastSyncedAt: new Date().toISOString(),
    });
  }, 100);
}

/** Reset the mock sync source to its initial state (useful for tests). */
export function resetSyncSource(): void {
  setState({ status: 'idle', progress: 0, lastSyncedAt: null });
}

/* -------------------------------------------------------------------------- */
/* Hook                                                                       */
/* -------------------------------------------------------------------------- */

export interface UseSyncStatusResult {
  status: SyncStatus;
  progress: number;
  lastSyncedAt: string | null;
  syncNow: () => void;
}

/** React hook exposing the (mock) sync status and a `syncNow` trigger. */
export function useSyncStatus(): UseSyncStatusResult {
  const [state, setStateInternal] = useState<SyncState>(currentState);

  useEffect(() => subscribeSync(setStateInternal), []);

  return {
    status: state.status,
    progress: state.progress,
    lastSyncedAt: state.lastSyncedAt,
    syncNow,
  };
}
