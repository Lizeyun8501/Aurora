/**
 * Minimal typed IndexedDB wrapper used by the Web/WASM platform adapter.
 *
 * Provides a tiny Promise-based API over the raw IDBRequest surface:
 * `openDB`, `get`, `put`, `delete`, `getAll`, `clear`. The wrapper is
 * intentionally small — it covers the read/write/delete needs of the
 * `wasmPlatform` file-system emulation (paths -> Uint8Array blobs) and the
 * offline sync queue, nothing more.
 *
 * The wrapper is environment-tolerant: when `indexedDB` is unavailable
 * (SSR, non-DOM test env without a shim) the helpers reject with a clear
 * error so callers can fall back gracefully.
 */

/** A versioned object-store descriptor used to open a database. */
export interface DBSchema {
  /** Database name. */
  name: string;
  /** Database version (positive integer). */
  version: number;
  /** Object stores to ensure exist on upgrade. */
  stores: DBStoreDef[];
}

/** Object-store definition. */
export interface DBStoreDef {
  /** Store name. */
  name: string;
  /** Key path for the store (e.g. `"path"` or `"id"`). */
  keyPath: string;
}

/** A handle to an opened database + a known store. */
export interface DBHandle<S extends string = string> {
  readonly db: IDBDatabase;
  readonly store: S;
}

type DBValue = unknown;

/**
 * Open (or create) an IndexedDB database matching `schema`.
 *
 * On upgrade the listed stores are created if missing (idempotent).
 * Resolves with the opened `IDBDatabase`.
 */
export function openDB(schema: DBSchema): Promise<IDBDatabase> {
  return new Promise<IDBDatabase>((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new Error('IndexedDB is not available in this environment'));
      return;
    }
    const request = indexedDB.open(schema.name, schema.version);
    request.onupgradeneeded = () => {
      const db = request.result;
      for (const store of schema.stores) {
        if (!db.objectStoreNames.contains(store.name)) {
          db.createObjectStore(store.name, { keyPath: store.keyPath });
        }
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(request.error ?? new Error('Failed to open IndexedDB'));
  });
}

/**
 * Begin a readwrite transaction on `storeName` and run `fn` against the
 * object store. Resolves with the value produced by `fn`.
 */
function withStore<S extends string, T>(
  db: IDBDatabase,
  storeName: S,
  mode: IDBTransactionMode,
  fn: (store: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const tx = db.transaction(storeName, mode);
    const store = tx.objectStore(storeName);
    const request = fn(store);
    request.onsuccess = () => resolve(request.result);
    request.onerror = () =>
      reject(request.error ?? new Error('IndexedDB request failed'));
  });
}

/** Get a single record by key. Resolves `undefined` when missing. */
export function get<S extends string>(
  handle: DBHandle<S>,
  key: IDBValidKey,
): Promise<DBValue | undefined> {
  return withStore(handle.db, handle.store, 'readonly', (store) =>
    store.get(key),
  );
}

/** Put (upsert) a record. */
export function put<S extends string>(
  handle: DBHandle<S>,
  value: DBValue,
): Promise<IDBValidKey> {
  return withStore(handle.db, handle.store, 'readwrite', (store) =>
    store.put(value),
  );
}

/** Delete a record by key. */
export function del<S extends string>(
  handle: DBHandle<S>,
  key: IDBValidKey,
): Promise<undefined> {
  return withStore(handle.db, handle.store, 'readwrite', (store) =>
    store.delete(key),
  ).then(() => undefined);
}

/** Retrieve all records in the store. */
export function getAll<S extends string>(
  handle: DBHandle<S>,
): Promise<DBValue[]> {
  return withStore(handle.db, handle.store, 'readonly', (store) =>
    store.getAll(),
  );
}

/** Clear all records in the store. */
export function clear<S extends string>(handle: DBHandle<S>): Promise<undefined> {
  return withStore(handle.db, handle.store, 'readwrite', (store) =>
    store.clear(),
  ).then(() => undefined);
}

/** Convenience: open a single-store database and return a handle bound to it. */
export async function openStore<S extends string>(
  schema: DBSchema,
  storeName: S,
): Promise<DBHandle<S>> {
  const db = await openDB(schema);
  return { db, store: storeName };
}
