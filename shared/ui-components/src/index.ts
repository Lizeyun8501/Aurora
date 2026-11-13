/**
 * @aurora/ui-components
 *
 * View-layer core components for Aurora Note: block renderers, editor shells,
 * layout primitives, page-level views, custom hooks, Zustand stores,
 * and Aurora ProseMirror Schema (V19 §35).
 */
export * from './blocks';
export * from './editors';
export * from './layouts';
export * from './schema';
export * from './views';
export * from './hooks';
export * from './stores';
export {
  THEME_WHITELIST, THEME_DEFAULTS, THEME_PRESETS,
  isValidHex, relativeLuminance, contrastRatio,
  validateTheme, applyTheme, clearTheme, exportThemeJSON, importThemeJSON,
} from './theme/themeVm';
export type { ThemeTokenKey, ThemeOverrides, ThemeValidation } from './theme/themeVm';
