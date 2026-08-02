/**
 * Web/WASM PlatformAPI implementation.
 *
 * Bridges the {@link PlatformAPI} surface to browser-standard APIs:
 *  - File system  -> IndexedDB (paths mapped to Uint8Array blobs)
 *  - HTTP         -> `fetch`
 *  - Notifications -> Web Notifications API
 *  - Crypto       -> Web Crypto API (`crypto.subtle`)
 *  - Biometrics   -> mock (Web has no biometric; returns `{ success: false }`)
 *  - Clipboard    -> `navigator.clipboard`
 *
 * The implementation is async-by-default and degrades gracefully when a
 * capability is unavailable (e.g. clipboard in a non-secure context).
 */

import type {
  BiometricAuthResult,
  CryptoResult,
  HttpRequest,
  HttpResponse,
  NotificationOptions,
  PlatformAPI,
  PlatformInfo,
} from '@aurora/shared-types';
import {
  openStore,
  put as idbPut,
  get as idbGet,
  del as idbDel,
  type DBHandle,
} from './indexeddb';

/** IndexedDB record shape used to emulate a file. */
interface FileRecord {
  path: string;
  data: Uint8Array;
}

const DB_NAME = 'aurora-web-fs';
const DB_VERSION = 1;
const STORE_FILES = 'files' as const;
const FILE_KEY = 'path';

const APP_VERSION = '0.1.0';

/** Decode a byte buffer to a base64 string (no chunking issues). */
function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** Decode a base64 string to a Uint8Array. */
function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    out[i] = binary.charCodeAt(i);
  }
  return out;
}

/** Convert a `BufferSource` (e.g. ArrayBuffer) to a Uint8Array. */
function bufferToBytes(buffer: ArrayBuffer | ArrayBufferView): Uint8Array {
  if (ArrayBuffer.isView(buffer)) {
    const view = buffer as ArrayBufferView;
    return new Uint8Array(
      view.buffer.slice(view.byteOffset, view.byteOffset + view.byteLength),
    );
  }
  return new Uint8Array(buffer as ArrayBuffer);
}

/**
 * Web/WASM {@link PlatformAPI} implementation.
 *
 * A single shared instance is exported as {@link wasmPlatform} and is the
 * default platform used by the web `PlatformProvider`.
 */
export class WasmPlatform implements PlatformAPI {
  readonly info: PlatformInfo;
  private handlePromise: Promise<DBHandle<typeof STORE_FILES>> | null = null;

  constructor() {
    this.info = {
      platform: detectPlatform(),
      host: 'web',
      app_version: APP_VERSION,
      os_version: null,
      device_id: null,
      online: typeof navigator !== 'undefined' ? navigator.onLine : true,
    };
    // NOTE: the IndexedDB handle is created lazily on first use so that
    // constructing this class (and the module-level `wasmPlatform` singleton)
    // has no side effects — important for tests that install an IDB shim
    // after import.
  }

  /** Lazily open (once) the IndexedDB handle backing the file system. */
  private getHandle(): Promise<DBHandle<typeof STORE_FILES>> {
    if (!this.handlePromise) {
      this.handlePromise = openStore(
        {
          name: DB_NAME,
          version: DB_VERSION,
          stores: [{ name: STORE_FILES, keyPath: FILE_KEY }],
        },
        STORE_FILES,
      );
    }
    return this.handlePromise;
  }

  async getPlatformInfo(): Promise<PlatformInfo> {
    return {
      ...this.info,
      online: typeof navigator !== 'undefined' ? navigator.onLine : true,
    };
  }

  // --- File system (IndexedDB-backed) ---

  async readFile(path: string): Promise<Uint8Array> {
    const handle = await this.getHandle();
    const record = (await idbGet(handle, path)) as FileRecord | undefined;
    if (!record) {
      throw new Error(`File not found: ${path}`);
    }
    return record.data;
  }

  async writeFile(path: string, data: Uint8Array): Promise<void> {
    const handle = await this.getHandle();
    await idbPut(handle, { path, data } satisfies FileRecord);
  }

  async deleteFile(path: string): Promise<void> {
    const handle = await this.getHandle();
    await idbDel(handle, path);
  }

  async fileExists(path: string): Promise<boolean> {
    const handle = await this.getHandle();
    const record = (await idbGet(handle, path)) as FileRecord | undefined;
    return record !== undefined;
  }

  // --- Network (fetch) ---

  async httpRequest(request: HttpRequest): Promise<HttpResponse> {
    const response = await fetch(request.url, {
      method: request.method,
      headers: request.headers,
      body: request.body,
    });
    const body = await response.text();
    const headers: Record<string, string> = {};
    response.headers.forEach((value, key) => {
      headers[key] = value;
    });
    return {
      status: response.status,
      headers,
      body,
    };
  }

  // --- Notifications (Web Notifications API) ---

  async showNotification(options: NotificationOptions): Promise<void> {
    if (typeof Notification === 'undefined') {
      // No notification API — silently no-op (mock behavior).
      return;
    }
    if (Notification.permission === 'granted') {
      new Notification(options.title, {
        body: options.body,
        icon: options.icon ?? undefined,
        tag: options.tag ?? undefined,
      });
      return;
    }
    if (Notification.permission !== 'denied') {
      const permission = await Notification.requestPermission();
      if (permission === 'granted') {
        new Notification(options.title, {
          body: options.body,
          icon: options.icon ?? undefined,
          tag: options.tag ?? undefined,
        });
      }
    }
  }

  // --- Crypto (Web Crypto API) ---

  async generateKey(algorithm: string): Promise<string> {
    const subtle = ensureSubtle();
    const algName = algorithm === 'aes-256-gcm' ? 'AES-GCM' : algorithm;
    const cryptoKey = await subtle.generateKey(
      { name: algName, length: 256 },
      true,
      ['encrypt', 'decrypt'],
    );
    const raw = await subtle.exportKey('raw', cryptoKey as CryptoKey);
    return bytesToBase64(bufferToBytes(raw));
  }

  async encrypt(key: string, plaintext: Uint8Array): Promise<CryptoResult> {
    const subtle = ensureSubtle();
    const cryptoKey = await importAesKey(subtle, key);
    const nonce = crypto.getRandomValues(new Uint8Array(12));
    const cipherBuffer = await subtle.encrypt(
      { name: 'AES-GCM', iv: nonce },
      cryptoKey,
      plaintext,
    );
    return {
      ciphertext: bytesToBase64(bufferToBytes(cipherBuffer)),
      nonce: bytesToBase64(nonce),
    };
  }

  async decrypt(
    key: string,
    ciphertext: string,
    nonce: string,
  ): Promise<Uint8Array> {
    const subtle = ensureSubtle();
    const cryptoKey = await importAesKey(subtle, key);
    const plainBuffer = await subtle.decrypt(
      { name: 'AES-GCM', iv: base64ToBytes(nonce) },
      cryptoKey,
      base64ToBytes(ciphertext),
    );
    return bufferToBytes(plainBuffer);
  }

  async hash(data: Uint8Array): Promise<string> {
    const subtle = ensureSubtle();
    const digest = await subtle.digest('SHA-256', data);
    return bytesToHex(bufferToBytes(digest));
  }

  // --- Biometrics (mock — Web has no biometric) ---

  async authenticateBiometric(_reason: string): Promise<BiometricAuthResult> {
    return {
      success: false,
      error: 'Biometric authentication is not available on web',
    };
  }

  // --- Clipboard ---

  async writeClipboard(text: string): Promise<void> {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      await navigator.clipboard.writeText(text);
    }
  }

  async readClipboard(): Promise<string> {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      return navigator.clipboard.readText();
    }
    return '';
  }
}

/** Shared default web platform instance. */
export const wasmPlatform: PlatformAPI = new WasmPlatform();

// --- Helpers ---

function detectPlatform(): PlatformInfo['platform'] {
  if (typeof navigator === 'undefined') return 'linux';
  const ua = navigator.userAgent.toLowerCase();
  if (ua.includes('mac')) return 'mac';
  if (ua.includes('win')) return 'windows';
  return 'linux';
}

function ensureSubtle(): SubtleCrypto {
  const subtle =
    typeof crypto !== 'undefined' ? crypto.subtle : undefined;
  if (!subtle) {
    throw new Error('Web Crypto API (crypto.subtle) is not available');
  }
  return subtle;
}

async function importAesKey(
  subtle: SubtleCrypto,
  base64Key: string,
): Promise<CryptoKey> {
  const raw = base64ToBytes(base64Key);
  return subtle.importKey('raw', raw, { name: 'AES-GCM' }, false, [
    'encrypt',
    'decrypt',
  ]);
}

function bytesToHex(bytes: Uint8Array): string {
  let hex = '';
  for (let i = 0; i < bytes.length; i += 1) {
    hex += bytes[i].toString(16).padStart(2, '0');
  }
  return hex;
}
