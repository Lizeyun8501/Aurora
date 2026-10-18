/**
 * DK-2.10 快捷键自定义（v1）— 应用内键位注册表+录入门。
 *
 * - 基线：DEFAULT_SHORTCUTS（palette=Mod+K / newNote=Mod+N / today=Mod+J）
 *   ——cmd_get_shortcuts() 覆盖表（None=未自定义）合并生效；
 * - 录入：ShortcutSettingsModal——点击动作行「改」进入捕获态，按下新
 *   组合键（Mod=meta||ctrl 统一语义，Shift 可叠加）即时预览，保存走
 *   cmd_set_shortcuts 全量覆盖（空对象=恢复默认）；Esc 取消捕获；
 * - 边界：无系统级全局快捷键（3.3 单列）；无冲突检测（后写覆盖）；
 *   browser-mock（invoke=null）走默认表只读。
 */

export type ShortcutTable = Record<string, string>;

export const DEFAULT_SHORTCUTS: ShortcutTable = {
  palette: 'Mod+K',
  newNote: 'Mod+N',
  today: 'Mod+J',
};

export const SHORTCUT_ACTIONS = Object.keys(DEFAULT_SHORTCUTS);

export const SHORTCUT_LABELS: Record<string, string> = {
  palette: '命令面板',
  newNote: '新建笔记',
  today: '今日视图',
};

/** 组合串 → 谓词（Mod=metaKey||ctrlKey；Shift=shiftKey；键名不区分大小写）。 */
export function comboMatches(combo: string, e: KeyboardEvent): boolean {
  const parts = combo.split('+').map((p) => p.trim());
  const key = parts[parts.length - 1].toLowerCase();
  const needMod = parts.includes('Mod');
  const needShift = parts.includes('Shift');
  const mod = e.metaKey || e.ctrlKey;
  if (needMod !== mod) return false;
  if (needShift !== e.shiftKey) return false;
  return e.key.toLowerCase() === key;
}

/** KeyboardEvent → 规范化组合串（录入门用；无 Mod 前缀的拒绝——防单键劫持）。 */
export function eventToCombo(e: KeyboardEvent): string | null {
  const mod = e.metaKey || e.ctrlKey;
  if (!mod) return null;
  const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (['Meta', 'Control', 'Shift', 'Alt'].includes(key)) return null;
  return `Mod+${e.shiftKey ? 'Shift+' : ''}${key}`;
}
