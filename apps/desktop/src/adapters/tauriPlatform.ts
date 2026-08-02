/**
 * Tauri v2 PlatformAPI implementation (Desktop).
 *
 * Implements {@link PlatformAPI} by delegating to the Tauri v2 plugin surface
 * (`@tauri-apps/plugin-fs`, `plugin-http`, `plugin-notification`) and the
 * shared IPC bridge (`./ipc`). When a plugin is unavailable (the desktop TS
 * running outside a real Tauri host, or the native package not installed), the
 * adapter falls back to browser/web-standard mocks — this keeps the desktop
 * adapter typecheckable and unit-testable without a Rust host.
 *
 * The platform-specific extras (system tray, global shortcut, native menu) are
 * exposed as desktop-only methods on {@link TauriPlatform} and are mock-wired
 * through the IPC bridge (`invoke`).
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
import { ipcBridge } from './ipc';

const APP_VERSION = '0.1.0';

/** Lazily-loaded Tauri plugin surface (all nullable; mocked when absent). */
interface TauriPluginSurface {
  readFile(path: string): Promise<Uint8Array>;
  writeFile(path: string, data: Uint8Array): Promise<void>;
  removeFile(path: string): Promise<void>;
  exists(path: string): Promise<boolean>;
  fetch(
    url: string,
    init: {
      method: string;
      headers?: Record<string, string>;
      body?: BodyInit | null;
    },
  ): Promise<Response>;
  sendNotification(options: { title: string; body?: string }): Promise<void>;
}

/**
 * Attempt to load the Tauri v2 plugin modules. Non-literal specifiers are used
 * so the file typechecks without the packages installed; at runtime a missing
 * module rejects and we return `null` (caller falls back to a mock).
 */
async function loadTauriPlugins(): Promise<TauriPluginSurface | null> {
  const fsSpec = '@tauri-apps/plugin-fs' as string;
  const httpSpec = '@tauri-apps/plugin-http' as string;
  const notifSpec = '@tauri-apps/plugin-notification' as string;
  try {
    const fs = (await import(fsSpec)) as {
      readFile?: (p: string) => Promise<Uint8Array>;
      writeFile?: (p: string, d: Uint8Array) => Promise<void>;
      removeFile?: (p: string) => Promise<void>;
      exists?: (p: string) => Promise<boolean>;
    };
    const http = (await import(httpSpec)) as {
      fetch?: typeof fetch;
    };
    const notif = (await import(notifSpec)) as {
      sendNotification?: (o: { title: string; body?: string }) => Promise<void>;
    };
    if (!fs.readFile || !fs.writeFile || !http.fetch || !notif.sendNotification) {
      return null;
    }
    return {
      readFile: (p) => fs.readFile!(p),
      writeFile: (p, d) => fs.writeFile!(p, d),
      removeFile: (p) => fs.removeFile!(p),
      exists: (p) => fs.exists!(p),
      fetch: (url, init) => http.fetch!(url, init),
      sendNotification: (o) => notif.sendNotification!(o),
    };
  } catch {
    return null;
  }
}

/** Decode base64 to bytes. */
function base64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const out = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) out[i] = binary.charCodeAt(i);
  return out;
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
 * Tauri v2 desktop {@link PlatformAPI}.
 *
 * Lazily loads the Tauri plugins on first use; falls back to web-standard
 * mocks (fetch, Web Crypto, IndexedDB-free in-memory FS) when the plugins
 * are not available.
 */
export class TauriPlatform implements PlatformAPI {
  readonly info: PlatformInfo;
  private pluginsPromise: Promise<TauriPluginSurface | null> | null = null;
  /** In-memory FS mock used when the Tauri fs plugin is unavailable. */
  private readonly mockFs = new Map<string, Uint8Array>();

  constructor() {
    this.info = {
      platform: detectDesktopPlatform(),
      host: 'desktop',
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

  private plugins(): Promise<TauriPluginSurface | null> {
    if (!this.pluginsPromise) {
      this.pluginsPromise = loadTauriPlugins();
    }
    return this.pluginsPromise;
  }

  // --- File system ---

  async readFile(path: string): Promise<Uint8Array> {
    const plugins = await this.plugins();
    if (plugins) return plugins.readFile(path);
    const data = this.mockFs.get(path);
    if (!data) throw new Error(`File not found (mock): ${path}`);
    return data;
  }

  async writeFile(path: string, data: Uint8Array): Promise<void> {
    const plugins = await this.plugins();
    if (plugins) {
      await plugins.writeFile(path, data);
      return;
    }
    this.mockFs.set(path, data);
  }

  async deleteFile(path: string): Promise<void> {
    const plugins = await this.plugins();
    if (plugins) {
      await plugins.removeFile(path);
      return;
    }
    this.mockFs.delete(path);
  }

  async fileExists(path: string): Promise<boolean> {
    const plugins = await this.plugins();
    if (plugins) return plugins.exists(path);
    return this.mockFs.has(path);
  }

  // --- Network ---

  async httpRequest(request: HttpRequest): Promise<HttpResponse> {
    const plugins = await this.plugins();
    const fetcher = plugins?.fetch ?? fetch;
    const response = await fetcher(request.url, {
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

  // --- Notifications ---

  async showNotification(options: NotificationOptions): Promise<void> {
    const plugins = await this.plugins();
    if (plugins) {
      await plugins.sendNotification({
        title: options.title,
        body: options.body,
      });
      return;
    }
    // Mock fallback: web Notification API if available.
    if (typeof Notification !== 'undefined') {
      new Notification(options.title, { body: options.body });
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

  // --- Biometrics (mock — desktop biometric wired via Tauri command) ---

  async authenticateBiometric(reason: string): Promise<BiometricAuthResult> {
    try {
      const result = await ipcBridge.invoke<{ success: boolean; error: string | null }>(
        'plugin:biometric|authenticate',
        { reason },
      );
      return { success: result.success, error: result.error };
    } catch {
      return {
        success: false,
        error: 'Biometric authentication unavailable on this desktop host',
      };
    }
  }

  // --- Clipboard ---

  async writeClipboard(text: string): Promise<void> {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      await navigator.clipboard.writeText(text);
      return;
    }
    await ipcBridge.invoke('plugin:clipboard|write_text', { text });
  }

  async readClipboard(): Promise<string> {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      return navigator.clipboard.readText();
    }
    return ipcBridge.invoke<string>('plugin:clipboard|read_text', {});
  }

  // --- Desktop-only platform-specific extras (mock-wired via IPC) ---

  /** Configure the system tray (mock: invokes `app:set_tray`). */
  async setSystemTray(icon: string, tooltip: string): Promise<void> {
    await ipcBridge.invoke('app:set_tray', { icon, tooltip });
  }

  /** Register a global keyboard shortcut (mock: invokes `app:register_shortcut`). */
  async registerGlobalShortcut(
    accelerator: string,
    handler: () => void,
  ): Promise<() => void> {
    await ipcBridge.invoke('app:register_shortcut', { accelerator });
    const unlisten = await ipcBridge.listen(`shortcut:${accelerator}`, () =>
      handler(),
    );
    return unlisten;
  }

  /** Set the native application menu (mock: invokes `app:set_menu`). */
  async setNativeMenu(menu: unknown): Promise<void> {
    await ipcBridge.invoke('app:set_menu', { menu });
  }
}

/** Shared default desktop platform instance. */
export const tauriPlatform: PlatformAPI = new TauriPlatform();

// --- Helpers ---

function detectDesktopPlatform(): PlatformInfo['platform'] {
  if (typeof navigator !== 'undefined') {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('mac')) return 'mac';
    if (ua.includes('win')) return 'windows';
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
