/**
 * DK-2.7 切片一：code_block 语法着色 token 着色——运行时 <style> 幂等注入
 * （desktop 无全局 css 基建——inline style + tokens 先例；createAuroraEditor
 * 装配时注入一次）。颜色取 One Dark 语义近似值（亮暗主题通用——诚实边界：
 * 未接 design tokens 变量，主题化延后）。
 */
export const HIGHLIGHT_STYLE_ID = 'aurora-code-highlight';

const HIGHLIGHT_CSS = `
.tok-comment { color: #7f848e; font-style: italic; opacity: 0.85; }
.tok-string  { color: #98c379; }
.tok-number  { color: #d19a66; }
.tok-keyword { color: #c678dd; }
`;

export function injectHighlightStyle(): void {
  if (typeof document === 'undefined') return; // SSR/browser-mock 守卫
  if (document.getElementById(HIGHLIGHT_STYLE_ID)) return; // 幂等
  const el = document.createElement('style');
  el.id = HIGHLIGHT_STYLE_ID;
  el.textContent = HIGHLIGHT_CSS;
  document.head.appendChild(el);
}
