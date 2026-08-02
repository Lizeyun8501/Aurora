/**
 * Web platform adapter barrel.
 *
 * Re-exports the Web/WASM {@link PlatformAPI} implementation, the IndexedDB
 * wrapper, the PWA service worker helper, and the React platform context.
 * The cross-module interaction controllers live under `./interactions`.
 */

export {
  WasmPlatform,
  wasmPlatform,
} from './wasmPlatform';
export {
  openDB,
  openStore,
  get,
  put,
  del,
  getAll,
  clear,
  type DBHandle,
  type DBSchema,
  type DBStoreDef,
} from './indexeddb';
export {
  registerServiceWorker,
  unregisterServiceWorkers,
} from './pwaServiceWorker';
export {
  PlatformProvider,
  usePlatform,
  PlatformContext,
  type PlatformProviderProps,
} from './platformContext';
export * from './interactions';
