/**
 * A-05（TA 2026-11-12 工单）: 块引用 NodeView — ((noteId#anchor)) 只读嵌入卡。
 *
 * 三态分流（fail-closed 语义分离）:
 * - ok: 源笔记标题 + 章节预览 + 「跳转源块」
 * - not_found: 「引用已失效」（源已删——无空白、无崩溃）
 * - locked: 「源笔记锁定/不可读」（可恢复态——不误报失效）
 *
 * 解析通道注入（宿主接线——桌面 invoke / 移动桥;未注入=显式提示态）:
 * - setBlockRefResolver(fn) 模块级单次注入（宿主启动期）
 * - 刷新: 'aurora:doc-refresh' DOM CustomEvent（宿主桥接保存事件——
 *   桌面 Tauri 'aurora:doc-updated' → 此处 DOM 事件;detail.noteId 匹配
 *   才重解析——增量刷新,不阻塞编辑）
 * - 跳转: 'aurora:open-note' DOM CustomEvent（宿主监听——打开源笔记+锚点滚动）
 *
 * 停手边界: v1 只读（无双向编辑）;无图算法。
 */
import type { Node as ProseNode } from 'prosemirror-model';
import type { NodeView } from 'prosemirror-view';

/** 解析结果（与桌面 cmd_resolve_block_resolution / 移动桥 JSON 同构）。 */
export interface BlockRefData {
  status: 'ok' | 'not_found' | 'locked';
  title?: string;
  section_md?: string;
}

/** 解析器签名（宿主注入）。 */
export type BlockRefResolver = (noteId: string, anchor: string | null) => Promise<BlockRefData>;

let resolver: BlockRefResolver | null = null;

/** 宿主注入解析通道（启动期单次;编辑器组件零依赖宿主 API）。 */
export function setBlockRefResolver(fn: BlockRefResolver): void {
  resolver = fn;
}

/** 刷新事件名（宿主桥接保存事件→DOM;detail={noteId}）。 */
export const BLOCK_REF_REFRESH_EVENT = 'aurora:doc-refresh';
/** 跳转事件名（NodeView→宿主;detail={noteId, anchor}）。 */
export const BLOCK_REF_OPEN_EVENT = 'aurora:open-note';

/** 卡片样式（幂等注入——focusBlock/highlight 惯例）。 */
let styleInjected = false;
function injectBlockRefStyle(): void {
  if (styleInjected) return;
  styleInjected = true;
  const el = document.createElement('style');
  el.id = 'aurora-block-ref-style';
  el.textContent = `
.aurora-block-ref-card {
  border-left: 3px solid rgba(88,166,255,0.55);
  background: rgba(88,166,255,0.07);
  border-radius: 6px;
  padding: 8px 12px;
  margin: 4px 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-width: 100%;
}
.aurora-block-ref-card .bref-title {
  font-size: 12px;
  font-weight: 600;
  opacity: 0.9;
  display: flex;
  align-items: center;
  gap: 6px;
}
.aurora-block-ref-card .bref-section {
  font-size: 13px;
  opacity: 0.85;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 160px;
  overflow: hidden;
}
.aurora-block-ref-card .bref-jump {
  align-self: flex-start;
  font-size: 11px;
  padding: 1px 10px;
  border-radius: 999px;
  border: 1px solid rgba(88,166,255,0.4);
  background: rgba(88,166,255,0.12);
  cursor: pointer;
}
.aurora-block-ref-card.bref-stale {
  border-left-color: rgba(229,72,77,0.55);
  background: rgba(229,72,77,0.07);
  opacity: 0.85;
}
.aurora-block-ref-card .bref-hint { font-size: 11px; opacity: 0.65; }
`;
  document.head.appendChild(el);
}

export class BlockRefNodeView implements NodeView {
  readonly dom: HTMLElement;
  private readonly noteId: string;
  private readonly anchor: string | null;
  private readonly onRefresh: (e: Event) => void;
  private destroyed = false;
  private seq = 0;

  constructor(node: ProseNode) {
    injectBlockRefStyle();
    this.noteId = String(node.attrs.noteId || '');
    this.anchor = node.attrs.anchor ? String(node.attrs.anchor) : null;
    this.dom = document.createElement('div');
    this.dom.className = 'aurora-block-ref-card';
    this.dom.setAttribute('role', 'blockquote');
    this.dom.setAttribute('aria-label', `块引用 ${this.noteId}`);
    this.renderLoading();
    void this.resolve();
    this.onRefresh = (e: Event): void => {
      const detail = (e as CustomEvent<{ noteId?: string }>).detail;
      if (detail?.noteId === this.noteId) void this.resolve();
    };
    window.addEventListener(BLOCK_REF_REFRESH_EVENT, this.onRefresh);
  }

  /** update: attrs 变（引用目标改）→ 重建（返回 false）。 */
  update(node: ProseNode): boolean {
    return (
      String(node.attrs.noteId) === this.noteId &&
      (node.attrs.anchor ?? null) === this.anchor
    );
  }

  destroy(): void {
    this.destroyed = true;
    window.removeEventListener(BLOCK_REF_REFRESH_EVENT, this.onRefresh);
  }

  private renderLoading(): void {
    this.dom.classList.remove('bref-stale');
    this.dom.textContent = '⤵ 解析块引用…';
  }

  private async resolve(): Promise<void> {
    const mySeq = ++this.seq;
    if (!resolver) {
      this.renderHint('（引用解析通道未接入——宿主未注入 resolver）');
      return;
    }
    try {
      const data = await resolver(this.noteId, this.anchor);
      if (this.destroyed || mySeq !== this.seq) return; // 过期响应丢弃
      this.render(data);
    } catch {
      if (this.destroyed || mySeq !== this.seq) return;
      this.renderStale('引用解析失败（通道异常）');
    }
  }

  private render(data: BlockRefData): void {
    if (data.status === 'not_found') {
      this.renderStale('引用已失效——源笔记不存在或已删除');
      return;
    }
    if (data.status === 'locked') {
      this.renderStale('源笔记锁定/不可读——解锁后自动可恢复');
      return;
    }
    this.dom.classList.remove('bref-stale');
    this.dom.textContent = '';
    const title = document.createElement('div');
    title.className = 'bref-title';
    title.textContent = `⤵ ${data.title || '未命名'}`;
    const section = document.createElement('div');
    section.className = 'bref-section';
    section.textContent = data.section_md ?? '';
    const jump = document.createElement('button');
    jump.type = 'button';
    jump.className = 'bref-jump';
    jump.textContent = '跳转源块 →';
    jump.addEventListener('click', () => {
      window.dispatchEvent(
        new CustomEvent(BLOCK_REF_OPEN_EVENT, {
          detail: { noteId: this.noteId, anchor: this.anchor },
        }),
      );
    });
    this.dom.append(title, section, jump);
  }

  private renderStale(msg: string): void {
    this.dom.classList.add('bref-stale');
    this.dom.textContent = `⛓ ${msg}`;
  }

  private renderHint(msg: string): void {
    this.dom.classList.remove('bref-stale');
    this.dom.textContent = `⛓ ((${this.noteId}${this.anchor ? '#' + this.anchor : ''})) ${msg}`;
  }
}

/** nodeViews 装配面（auroraEditor nodeViews 合并用——同 math/mermaid 惯例）。 */
export function blockRefNodeViews(): Record<string, (node: ProseNode) => BlockRefNodeView> {
  return { block_ref: (node: ProseNode) => new BlockRefNodeView(node) };
}
