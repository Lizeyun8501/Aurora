/**
 * Plugin system domain types.
 * Mirrors the concepts in `crates/aurora-plugin/`
 * (wasm/iframe runtime, lifecycle, permission, marketplace, hot-update).
 */

/** Plugin unique identifier. */
export type PluginId = string;

/** Plugin runtime mode (mirrors the dual wasm/iframe runtimes). */
export type PluginMode = 'wasm' | 'iframe';

/** Plugin lifecycle status. */
export type PluginStatus =
  | 'installed'
  | 'enabled'
  | 'disabled'
  | 'error'
  | 'updating'
  | 'uninstalled';

/** A capability the plugin may request access to. */
export type Capability =
  | 'file_system'
  | 'network'
  | 'clipboard'
  | 'notifications'
  | 'database'
  | 'editor'
  | 'ai'
  | 'sync'
  | 'system_settings'
  | (string & {});

/** Permission state for a capability. */
export type PermissionState = 'granted' | 'denied' | 'prompt';

/** A permission entry mapping a capability to its current state. */
export interface Permission {
  capability: Capability;
  state: PermissionState;
  /** Optional scope restriction (e.g. a path prefix). */
  scope: string | null;
}

/** Plugin manifest (declared by the plugin, validated on load). */
export interface PluginManifest {
  id: PluginId;
  name: string;
  version: string;
  description: string;
  author: string;
  mode: PluginMode;
  /** Entry point (wasm URL or iframe HTML URL). */
  entry: string;
  permissions: Permission[];
  capabilities: Capability[];
  /** Minimum host API version required. */
  min_host_version: string;
  homepage: string | null;
  license: string | null;
}

/** A loaded plugin instance. */
export interface PluginInstance {
  id: PluginId;
  manifest: PluginManifest;
  status: PluginStatus;
  installed_at: string;
  updated_at: string;
  /** Installed path / origin. */
  origin: string;
  /** Last error message, if status is `error`. */
  error: string | null;
}

/** A marketplace listing for a plugin. */
export interface MarketplaceListing {
  id: PluginId;
  name: string;
  version: string;
  description: string;
  author: string;
  mode: PluginMode;
  icon_url: string | null;
  download_url: string;
  homepage: string | null;
  downloads: number;
  rating: number;
  verified: boolean;
  permissions: Permission[];
  categories: string[];
  updated_at: string;
}
