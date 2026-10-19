/**
 * DK-2.7 切片六：mermaid NodeView 样式 — 渲染态容器+编辑态 textarea。
 * 注入模式同 mathStyle（幂等 <style>）。
 */
const MERMAID_STYLE_ID = 'aurora-mermaid-style';
const MERMAID_CSS = `
.aurora-mermaid { margin: 6px 0; background: transparent; }
.aurora-mermaid-render {
  display: flex; justify-content: center; cursor: default;
  padding: 4px; border-radius: 4px; white-space: pre-wrap;
}
.aurora-mermaid-render:hover { outline: 1px solid rgba(100,140,255,.4); }
.aurora-mermaid-render svg { max-width: 100%; height: auto; }
.aurora-mermaid-edit {
  width: 100%; font-family: monospace; font-size: 13px;
  border: 1px solid rgba(100,140,255,.6); border-radius: 4px;
  padding: 4px 6px; resize: vertical; background: transparent; color: inherit;
}
`;

export function injectMermaidStyle(): void {
  if (typeof document === 'undefined') return; // SSR/browser-mock 守卫
  if (document.getElementById(MERMAID_STYLE_ID)) return; // 幂等
  const el = document.createElement('style');
  el.id = MERMAID_STYLE_ID;
  el.textContent = MERMAID_CSS;
  document.head.appendChild(el);
}
