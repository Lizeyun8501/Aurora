/**
 * Capacitor v8 PlatformAPI implementation (Mobile).
 *
 * Implements {@link PlatformAPI} by routing through the Capacitor bridge
 * (`./bridge`) to native plugins: filesystem (mock), HTTP (mock via fetch
 * fallback), notifications (Push notifications API mock), biometrics
 * (`@capacitor-community/biometric-auth` mock), offline storage (mock),
 * camera (mock). The Bridge Call pattern is used so every native call flows
 * through a single, mockable seam.
 *
 * Module specifiers for the Capacitor packages are non-literal so the file
 * typechecks whether or not the native packages are installed; at runtime a
 * missing plugin rejects and the adapter falls back to a web mock.
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
import { capacitorBridge, type BridgeArgs } from './bridge';

const APP_VERSION = '0.1.0';

/** Lazily-loaded biometric-auth plugin surface (nullable). */
interface BiometricAuthPlugin {
  checkBiometrics(): Promise<{ available: boolean }>;
  authenticate(options: { reason: string }): Promise<void>;
}

/** Lazily load `@capacitor-community/biometric-auth`. */
async function loadBiometricAuth(): Promise<BiometricAuthPlugin | null> {
  const spec = '@capacitor-community/biometric-auth' as string;
  try {
    const mod = (await import(spec)) as {
      BiometricAuth?: BiometricAuthPlugin;
    };
    return mod.BiometricAuth ?? null;
  } catch {
    return null;
  }
}

/** Encode bytes to base64. */
function bytesToBase64(bytes: Uint8Array): string {
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** Decode base64 to bytes. */
function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
}

function bufferToBytes(buffer: ArrayBuffer | ArrayBufferView): Uint8Array {
  if (ArrayBuffer.isView(buffer)) {
    const view = buffer as ArrayBufferView;
    return new Uint8Array(
      view.buffer.slice(view.byteOffset, view.byteOffset + view.byteLength),
    );
  }
  return new Uint8Array(buffer as ArrayBuffer);
}

function bytesToHex(bytes: Uint8Array): string {
  let hex = '';
  for (let i = 0; i < bytes.length; i += 1)
    hex += bytes[i].toString(16).padStart(2, '0');
  return hex;
}

/**
 * Capacitor mobile {@link PlatformAPI}.
 *
 * Native calls are routed through {@link capacitorBridge}; when a plugin is
 * unavailable the adapter falls back to web-standard mocks (fetch, Web
 * Crypto, in-memory FS). Offline storage is mocked as an in-memory map (in
 * production backed by the Capacitor Preferences/Filesystem plugin).
 */
export class CapacitorPlatform implements PlatformAPI {
  readonly info: PlatformInfo;
  /** In-memory offline storage mock (path -> bytes). */
  private readonly mockFs = new Map<string, Uint8Array>();

  constructor() {
    this.info = {
      platform: detectMobilePlatform(),
      host: 'mobile',
      app_version: APP_VERSION,
      os_version: null,
      device_id: null,
      online: typeof navigator !== 'undefined' ? navigator.onLine : true,
    };
  }

  async getPlatformInfo(): Promise<PlatformInfo> {
    return {
      ...this.info,
      online: typeof navigator !== 'undefined' ? navigator.onLine : true,
    };
  }

  // --- File system / offline storage (mock) ---

  async readFile(path: string): Promise<Uint8Array> {
    try {
      return await capacitorBridge.call<BridgeArgs, { data: string }>(
        'Filesystem',
        'readFile',
        { path },
      ).then((r) => base64ToBytes(r.data));
    } catch {
      const data = this.mockFs.get(path);
      if (!data) throw new Error(`File not found (mock): ${path}`);
      return data;
    }
  }

  async writeFile(path: string, data: Uint8Array): Promise<void> {
    try {
      await capacitorBridge.call<BridgeArgs, void>('Filesystem', 'writeFile', {
        path,
        data: bytesToBase64(data),
      });
    } catch {
      this.mockFs.set(path, data);
    }
  }

  async deleteFile(path: string): Promise<void> {
    try {
      await capacitorBridge.call<BridgeArgs, void>('Filesystem', 'deleteFile', {
        path,
      });
    } catch {
      this.mockFs.delete(path);
    }
  }

  async fileExists(path: string): Promise<boolean> {
    try {
      const r = await capacitorBridge.call<BridgeArgs, { exists: boolean }>(
        'Filesystem',
        'stat',
        { path },
      );
      return r.exists;
    } catch {
      return this.mockFs.has(path);
    }
  }

  // --- Network (fetch fallback) ---

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
    return { status: response.status, headers, body };
  }

  // --- Notifications (Push notifications mock) ---

  async showNotification(options: NotificationOptions): Promise<void> {
    try {
      await capacitorBridge.call<BridgeArgs, void>(
        'PushNotifications',
        'createChannel',
        { id: options.tag ?? 'aurora', name: options.title, description: options.body },
      );
    } catch {
      // Mock fallback: web Notification API if available.
      if (typeof Notification !== 'undefined') {
        new Notification(options.title, { body: options.body });
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

  // --- Biometrics (via @capacitor-community/biometric-auth mock) ---

  async authenticateBiometric(reason: string): Promise<BiometricAuthResult> {
    const plugin = await loadBiometricAuth();
    if (!plugin) {
      return {
        success: false,
        error: 'Biometric plugin not available on this device',
      };
    }
    try {
      const check = await plugin.checkBiometrics();
      if (!check.available) {
        return { success: false, error: 'Biometrics not enrolled' };
      }
      await plugin.authenticate({ reason });
      return { success: true, error: null };
    } catch (err) {
      return {
        success: false,
        error: err instanceof Error ? err.message : 'Biometric authentication failed',
      };
    }
  }

  // --- Clipboard ---

  async writeClipboard(text: string): Promise<void> {
    try {
      await capacitorBridge.call<BridgeArgs, void>('Clipboard', 'write', { string: text });
    } catch {
      if (typeof navigator !== 'undefined' && navigator.clipboard) {
        await navigator.clipboard.writeText(text);
      }
    }
  }

  async readClipboard(): Promise<string> {
    try {
      const r = await capacitorBridge.call<BridgeArgs, { value: string }>(
        'Clipboard',
        'read',
        {},
      );
      return r.value;
    } catch {
      if (typeof navigator !== 'undefined' && navigator.clipboard) {
        return navigator.clipboard.readText();
      }
      return '';
    }
  }

  // --- Mobile-only platform-specific extras (mock) ---

  /** Request camera capture (mock: returns a placeholder data URL). */
  async capturePhoto(): Promise<string> {
    try {
      const r = await capacitorBridge.call<BridgeArgs, { dataUrl: string }>(
        'Camera',
        'getPhoto',
        { quality: 80, resultType: 'DataUrl' },
      );
      return r.dataUrl;
    } catch {
      return 'data:image/png;base64,';
    }
  }

  /** Register for push notifications (mock). */
  async registerPushNotifications(): Promise<string> {
    try {
      const r = await capacitorBridge.call<BridgeArgs, { value: string }>(
        'PushNotifications',
        'register',
        {},
      );
      return r.value;
    } catch {
      return 'mock-push-token';
    }
  }
}

/** Shared default mobile platform instance. */
export const capacitorPlatform: PlatformAPI = new CapacitorPlatform();

// --- Helpers ---

function detectMobilePlatform(): PlatformInfo['platform'] {
  if (typeof navigator !== 'undefined') {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('iphone') || ua.includes('ipad') || ua.includes('mac')) {
      return 'mac';
    }
    if (ua.includes('android') || ua.includes('win')) {
      return 'windows';
    }
  }
  return 'linux';
}

function ensureSubtle(): SubtleCrypto {
  const subtle = typeof crypto !== 'undefined' ? crypto.subtle : undefined;
  if (!subtle) throw new Error('Web Crypto API (crypto.subtle) is not available');
  return subtle;
}

async function importAesKey(
  subtle: SubtleCrypto,
  base64Key: string,
): Promise<CryptoKey> {
  return subtle.importKey(
    'raw',
    base64ToBytes(base64Key),
    { name: 'AES-GCM' },
    false,
    ['encrypt', 'decrypt'],
  );
}
