/**
 * Aurora Desktop (Tauri v2) barrel.
 */

export {
  TauriPlatform,
  tauriPlatform,
} from './adapters/tauriPlatform';
export {
  TauriIpcBridge,
  ipcBridge,
  initTauriIpc,
  tryLoadTauriApis,
  createMockInvoke,
  createMockEmit,
  createMockListen,
  type InvokeBackend,
  type EmitBackend,
  type ListenBackend,
  type CommandArgs,
  type Unlisten,
} from './adapters/ipc';
export { initDesktopPlatform, platform } from './main';
