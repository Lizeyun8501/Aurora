/**
 * Aurora Desktop (Tauri v2) entry point.
 *
 * In a real Tauri v2 app the JS entry is the same as the web bundle
 * (Tauri loads `frontendDist`), and the Rust entry lives under
 * `src-tauri/src/main.rs` (or `lib.rs`). This module provides the typed
 * adapter bootstrap used by the desktop shell: it initialises the IPC bridge
 * against the real Tauri APIs (when present) and exports the desktop
 * {@link PlatformAPI}.
 */

import { initTauriIpc } from './adapters/ipc';
import { tauriPlatform } from './adapters/tauriPlatform';
import type { PlatformAPI } from '@aurora/shared-types';

/** Whether the Tauri IPC backend was successfully loaded. */
export let ipcReady = false;

/**
 * Initialise the desktop platform: wire the IPC bridge to the real Tauri
 * APIs (or fall back to mocks). Idempotent.
 */
export async function initDesktopPlatform(): Promise<PlatformAPI> {
  ipcReady = await initTauriIpc();
  return tauriPlatform;
}

/** Default desktop {@link PlatformAPI}. */
export const platform: PlatformAPI = tauriPlatform;

export { tauriPlatform, ipcBridge, TauriPlatform, TauriIpcBridge } from './adapters';
