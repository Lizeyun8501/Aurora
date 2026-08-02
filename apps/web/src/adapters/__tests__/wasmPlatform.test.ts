/**
 * WasmPlatform tests.
 *
 * Covers:
 *  - IndexedDB round-trip via an in-memory IDB shim (jsdom has no native IDB).
 *  - `httpRequest` against a mocked `fetch`.
 *  - Web Crypto encrypt/decrypt round-trip + SHA-256 hash.
 *  - `getPlatformInfo` / clipboard / biometric mock.
 */

import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import { WasmPlatform } from '../wasmPlatform';

// ---------------------------------------------------------------------------
// Minimal in-memory IndexedDB shim (jsdom does not implement IDB).
// ---------------------------------------------------------------------------

interface ShimRequest<T> {
  result: T | undefined;
  error: unknown;
  onsuccess: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
}

/** An open() request additionally exposes `onupgradeneeded`. */
interface ShimOpenRequest extends ShimRequest<ShimDB> {
  onupgradeneeded: ((ev: unknown) => void) | null;
}

function makeRequest<T>(result: T): ShimRequest<T> {
  const req: ShimRequest<T> = {
    result,
    error: undefined,
    onsuccess: null,
    onerror: null,
  };
  // Fire asynchronously so callers can attach handlers first.
  queueMicrotask(() => {
    if (req.onsuccess) req.onsuccess({ target: req });
  });
  return req;
}

class ShimObjectStore {
  private records = new Map<unknown, Record<string, unknown>>();
  constructor(readonly name: string, readonly keyPath: string) {}

  get(key: unknown): ShimRequest<unknown> {
    return makeRequest(this.records.get(key));
  }
  put(value: Record<string, unknown>): ShimRequest<unknown> {
    const key = value[this.keyPath];
    this.records.set(key, value);
    return makeRequest(key);
  }
  delete(key: unknown): ShimRequest<unknown> {
    this.records.delete(key);
    return makeRequest(undefined);
  }
  getAll(): ShimRequest<unknown[]> {
    return makeRequest(Array.from(this.records.values()));
  }
  clear(): ShimRequest<unknown> {
    this.records.clear();
    return makeRequest(undefined);
  }
}

class ShimDB {
  readonly objectStoreNames: { contains: (name: string) => boolean };
  private stores = new Map<string, ShimObjectStore>();

  constructor() {
    this.objectStoreNames = {
      contains: (name: string) => this.stores.has(name),
    };
  }

  createObjectStore(name: string, options: { keyPath: string }): ShimObjectStore {
    const store = new ShimObjectStore(name, options.keyPath);
    this.stores.set(name, store);
    return store;
  }

  transaction(_storeName: string, _mode: string): {
    objectStore: (name: string) => ShimObjectStore;
  } {
    return {
      objectStore: (name: string): ShimObjectStore => {
        const store = this.stores.get(name);
        if (!store) throw new Error(`Store not found: ${name}`);
        return store;
      },
    };
  }
}

class ShimIDB {
  private dbs = new Map<string, ShimDB>();

  open(name: string, _version: number): ShimOpenRequest {
    let db = this.dbs.get(name);
    const request: ShimOpenRequest = {
      result: undefined,
      error: undefined,
      onsuccess: null,
      onerror: null,
      onupgradeneeded: null,
    };
    const isNew = !db;
    if (!db) {
      db = new ShimDB();
      this.dbs.set(name, db);
    }
    queueMicrotask(() => {
      request.result = db;
      if (isNew && request.onupgradeneeded) {
        request.onupgradeneeded({ target: request });
      }
      if (request.onsuccess) request.onsuccess({ target: request });
    });
    return request;
  }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

let originalIndexedDB: unknown;
let originalFetch: unknown;

beforeAll(() => {
  // Ensure Web Crypto (crypto.subtle) is available — jsdom may not expose it.
  if (!globalThis.crypto || !(globalThis.crypto as Crypto).subtle) {
    vi.stubGlobal('crypto', webcrypto);
  }
});

beforeEach(() => {
  originalIndexedDB = (globalThis as unknown as { indexedDB?: unknown }).indexedDB;
  originalFetch = (globalThis as unknown as { fetch?: unknown }).fetch;
  (globalThis as unknown as { indexedDB: unknown }).indexedDB = new ShimIDB();
});

afterEach(() => {
  (globalThis as unknown as { indexedDB: unknown }).indexedDB = originalIndexedDB;
  (globalThis as unknown as { fetch: unknown }).fetch = originalFetch;
  vi.unstubAllGlobals();
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('WasmPlatform — IndexedDB file system', () => {
  it('writes and reads a file round-trip', async () => {
    const p = new WasmPlatform();
    const payload = new Uint8Array([1, 2, 3, 4, 5]);
    await p.writeFile('/notes/a.txt', payload);
    const read = await p.readFile('/notes/a.txt');
    expect(Array.from(read)).toEqual([1, 2, 3, 4, 5]);
  });

  it('reports file existence correctly', async () => {
    const p = new WasmPlatform();
    expect(await p.fileExists('/missing.txt')).toBe(false);
    await p.writeFile('/missing.txt', new Uint8Array([9]));
    expect(await p.fileExists('/missing.txt')).toBe(true);
  });

  it('deletes a file', async () => {
    const p = new WasmPlatform();
    await p.writeFile('/doomed.txt', new Uint8Array([0]));
    await p.deleteFile('/doomed.txt');
    expect(await p.fileExists('/doomed.txt')).toBe(false);
  });

  it('rejects when reading a missing file', async () => {
    const p = new WasmPlatform();
    await expect(p.readFile('/nope.txt')).rejects.toThrow(/not found/i);
  });
});

describe('WasmPlatform — HTTP', () => {
  it('performs an HTTP request via fetch', async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response('hello body', {
        status: 200,
        headers: { 'x-test': 'yes' },
      }),
    );
    (globalThis as unknown as { fetch: unknown }).fetch = fetchMock;

    const p = new WasmPlatform();
    const res = await p.httpRequest({
      url: 'https://example.com/api',
      method: 'GET',
      headers: {},
      body: null,
    });
    expect(res.status).toBe(200);
    expect(res.body).toBe('hello body');
    expect(res.headers['x-test']).toBe('yes');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});

describe('WasmPlatform — Web Crypto', () => {
  it('encrypts and decrypts a round-trip with AES-GCM', async () => {
    const p = new WasmPlatform();
    const key = await p.generateKey('aes-256-gcm');
    expect(typeof key).toBe('string');
    expect(key.length).toBeGreaterThan(0);

    const plaintext = new TextEncoder().encode('secret message');
    const { ciphertext, nonce } = await p.encrypt(key, plaintext);
    expect(ciphertext).not.toBe('');
    expect(nonce).not.toBe('');

    const decrypted = await p.decrypt(key, ciphertext, nonce);
    expect(new TextDecoder().decode(decrypted)).toBe('secret message');
  });

  it('computes a SHA-256 hex digest', async () => {
    const p = new WasmPlatform();
    const data = new TextEncoder().encode('abc');
    const hex = await p.hash(data);
    // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
    expect(hex).toBe(
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
  });
});

describe('WasmPlatform — misc surface', () => {
  it('returns web platform info', async () => {
    const p = new WasmPlatform();
    const info = await p.getPlatformInfo();
    expect(info.host).toBe('web');
    expect(info.platform).toMatch(/^(mac|windows|linux)$/);
  });

  it('mocks biometric authentication as unavailable', async () => {
    const p = new WasmPlatform();
    const result = await p.authenticateBiometric('unlock');
    expect(result.success).toBe(false);
    expect(result.error).toMatch(/biometric/i);
  });

  it('writes and reads the clipboard when available', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn().mockResolvedValue('clip-value');
    vi.stubGlobal('navigator', {
      clipboard: { writeText, readText },
      onLine: true,
      userAgent: 'node',
    });
    const p = new WasmPlatform();
    await p.writeClipboard('clip-value');
    expect(writeText).toHaveBeenCalledWith('clip-value');
    const text = await p.readClipboard();
    expect(text).toBe('clip-value');
  });
});
