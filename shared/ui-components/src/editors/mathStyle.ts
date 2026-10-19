/**
 * DK-2.7 切片五：公式 NodeView 样式 — KaTeX 渲染态 + 编辑态。
 * 注入模式同 editorHighlightStyle（幂等 <style>）。
 */
const MATH_STYLE_ID = 'aurora-math-style';
const MATH_CSS = `
.aurora-math-inline, .aurora-math-block { display: inline-block; }
.aurora-math-block { display: block; text-align: center; margin: 4px 0; }
.aurora-math-render { cursor: default; }
.aurora-math-inline .aurora-math-render:hover,
.aurora-math-block .aurora-math-render:hover { outline: 1px solid rgba(100,140,255,.4); border-radius: 3px; }
.aurora-math-edit {
  width: 95%; font-family: monospace; font-size: 13px;
  border: 1px solid rgba(100,140,255,.6); border-radius: 4px;
  padding: 2px 4px; resize: vertical; background: transparent; color: inherit;
}
.katex { font-size: 1.05em; }
`;

export function injectMathStyle(): void {
  if (typeof document === 'undefined') return; // SSR/browser-mock 守卫
  if (document.getElementById(MATH_STYLE_ID)) return; // 幂等
  const el = document.createElement('style');
  el.id = MATH_STYLE_ID;
  el.textContent = MATH_CSS;
  document.head.appendChild(el);
}
