/**
 * Platform adapter types.
 *
 * The `PlatformAPI` interface abstracts the host capabilities (filesystem,
 * network, crypto, biometrics, notifications) that the view layer depends on.
 * Different hosts (web, desktop/Tauri, mobile, extension) provide their own
 * implementation of this interface.
 */

import type { Platform } from './settings';

/** Information about the current host platform. */
export interface PlatformInfo {
  platform: Platform;
  /** Host flavor: web, desktop (Tauri), mobile, extension. */
  host: 'web' | 'desktop' | 'mobile' | 'extension';
  /** Host application version. */
  app_version: string;
  /** OS version string, if known. */
  os_version: string | null;
  /** Unique device identifier, if available. */
  device_id: string | null;
  /** Whether the host is currently online. */
  online: boolean;
}

/** HTTP request descriptor. */
export interface HttpRequest {
  url: string;
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  headers: Record<string, string>;
  body: string | null;
}

/** HTTP response. */
export interface HttpResponse {
  status: number;
  headers: Record<string, string>;
  body: string;
}

/** Notification options. */
export interface NotificationOptions {
  title: string;
  body: string;
  icon: string | null;
  tag: string | null;
}

/** Result of a biometric authentication attempt. */
export interface BiometricAuthResult {
  success: boolean;
  /** Error code/message when unsuccessful. */
  error: string | null;
}

/** Result of an encryption operation. */
export interface CryptoResult {
  /** Ciphertext (base64). */
  ciphertext: string;
  /** Nonce / IV (base64). */
  nonce: string;
}

/**
 * Platform API surface (mirrors the capabilities the view/adapter layer needs).
 * All methods are async and reject on failure.
 */
export interface PlatformAPI {
  readonly info: PlatformInfo;

  /** Resolve the current {@link PlatformInfo} (async variant of {@link info}). */
  getPlatformInfo(): Promise<PlatformInfo>;

  // --- File system ---
  readFile(path: string): Promise<Uint8Array>;
  writeFile(path: string, data: Uint8Array): Promise<void>;
  deleteFile(path: string): Promise<void>;
  fileExists(path: string): Promise<boolean>;

  // --- Network ---
  httpRequest(request: HttpRequest): Promise<HttpResponse>;

  // --- Notifications ---
  showNotification(options: NotificationOptions): Promise<void>;

  // --- Crypto ---
  /** Generate a new symmetric key (base64). */
  generateKey(algorithm: string): Promise<string>;
  encrypt(key: string, plaintext: Uint8Array): Promise<CryptoResult>;
  decrypt(key: string, ciphertext: string, nonce: string): Promise<Uint8Array>;
  /** Compute a SHA-256 digest (hex). */
  hash(data: Uint8Array): Promise<string>;

  // --- Biometrics ---
  authenticateBiometric(reason: string): Promise<BiometricAuthResult>;

  // --- Clipboard ---
  writeClipboard(text: string): Promise<void>;
  readClipboard(): Promise<string>;
}
