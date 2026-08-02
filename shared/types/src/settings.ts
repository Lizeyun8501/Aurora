/**
 * System Settings domain types.
 * Mirrors `crates/aurora-core/src/l3_domain/system_settings.rs`.
 */

import type { JsonObject } from './blocks';

/** Settings layer (mirrors `SettingsLayer`, serde `snake_case`). */
export type SettingsLayer = 'system' | 'user' | 'workspace';

/** Theme mode (mirrors `ThemeMode`, serde `snake_case`). */
export type ThemeMode = 'light' | 'dark' | 'sepia' | 'high_contrast' | 'auto';

/** Platform (mirrors `Platform`, serde `snake_case`). */
export type Platform = 'mac' | 'windows' | 'linux';

/** Shortcut scope (mirrors `ShortcutScope`, serde `snake_case`). */
export type ShortcutScope = 'global' | 'editor';

/**
 * Design tokens (mirrors `DesignTokens`).
 * Values map 1:1 to CSS custom properties emitted by `to_css_variables()`.
 */
export interface DesignTokens {
  bg_primary: string;
  bg_secondary: string;
  text_primary: string;
  text_secondary: string;
  accent: string;
  border: string;
  font_size_base: string;
}

/** Theme definition (mirrors `Theme`). */
export interface Theme {
  name: string;
  mode: ThemeMode;
  tokens: DesignTokens;
}

/** Shortcut binding (mirrors `ShortcutBinding`). */
export interface ShortcutBinding {
  scope: ShortcutScope;
  /** Modifier keys, e.g. `["Ctrl", "Shift"]`. */
  modifiers: string[];
  /** Primary key, e.g. `"K"`, `"Enter"`. */
  key: string;
}

/** Shortcut definition (mirrors `Shortcut`). */
export interface Shortcut {
  id: string;
  name: string;
  description: string;
  binding: ShortcutBinding;
  platform: Platform;
}

/** Shortcut conflict detection result (mirrors `ShortcutConflict`). */
export interface ShortcutConflict {
  signature: string;
  shortcuts: string[];
}

/** Settings version (for migrations, mirrors `SettingsVersion`). */
export interface SettingsVersion {
  schema_version: number;
  migrated_at: string;
  migrations_applied: string[];
}

/** One layer of settings data (mirrors `LayerSettings`). */
export interface LayerSettings {
  layer: SettingsLayer;
  /** Workspace id (only meaningful for the `workspace` layer). */
  workspace_id: string | null;
  values: JsonObject;
}

/** Settings schema entry (mirrors `SettingsSchema`). */
export interface SettingsSchema {
  key: string;
  description: string;
  default_value: unknown;
  layer: SettingsLayer;
  editable: boolean;
}

/**
 * System settings aggregate (mirrors `SystemSettings`).
 * Expressed as an interface describing the persisted configuration shape.
 */
export interface SystemSettings {
  version: SettingsVersion;
  layers: LayerSettings[];
  theme_mode: ThemeMode;
  token_overrides: Partial<DesignTokens>;
  shortcuts: Shortcut[];
}
