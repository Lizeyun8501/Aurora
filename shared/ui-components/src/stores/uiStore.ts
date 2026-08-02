import { create } from 'zustand';
import type { ThemeMode } from '@aurora/shared-types';

export interface UIState {
  sidebarOpen: boolean;
  activeView: string;
  theme: ThemeMode;
  commandPaletteOpen: boolean;
  toggleSidebar: () => void;
  setSidebarOpen: (open: boolean) => void;
  setActiveView: (view: string) => void;
  setTheme: (theme: ThemeMode) => void;
  toggleCommandPalette: () => void;
  setCommandPaletteOpen: (open: boolean) => void;
}

export const useUIStore = create<UIState>()((set) => ({
  sidebarOpen: true,
  activeView: 'today',
  theme: 'light',
  commandPaletteOpen: false,
  toggleSidebar: () => set((state) => ({ sidebarOpen: !state.sidebarOpen })),
  setSidebarOpen: (open) => set({ sidebarOpen: open }),
  setActiveView: (view) => set({ activeView: view }),
  setTheme: (theme) => set({ theme }),
  toggleCommandPalette: () =>
    set((state) => ({ commandPaletteOpen: !state.commandPaletteOpen })),
  setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),
}));
