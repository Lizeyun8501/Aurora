import { create } from 'zustand';
import type { Device, SyncStatus } from '@aurora/shared-types';

export interface SyncStoreState {
  status: SyncStatus;
  devices: Device[];
  lastSync: string | null;
  startSync: () => void;
  addDevice: (device: Device) => void;
  revokeDevice: (deviceId: string) => void;
}

export const useSyncStore = create<SyncStoreState>()((set) => ({
  status: 'idle',
  devices: [],
  lastSync: null,
  startSync: () =>
    set({ status: 'syncing', lastSync: new Date().toISOString() }),
  addDevice: (device) =>
    set((state) => ({ devices: [...state.devices, device] })),
  revokeDevice: (deviceId) =>
    set((state) => ({
      devices: state.devices.filter((d) => d.id !== deviceId),
    })),
}));
