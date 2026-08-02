/**
 * Desktop (Tauri v2) adapter barrel.
 */

export {
  TauriPlatform,
  tauriPlatform,
} from './tauriPlatform';
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
  type RecordedInvocation,
  type RecordedEmission,
} from './ipc';
