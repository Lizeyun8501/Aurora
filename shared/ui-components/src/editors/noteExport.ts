/**
 * E6 导出引擎（TA 22:33 派单——⋮ 菜单导入导出: Word/PDF/Markdown）
 *
 * 数据源: window.__editorView（RichEditor E2 暴露通道）。
 * - Markdown: 复用 auroraMarkdown.docToMd（DK-05 S2 落库同款序列化——往返一致）
 * - Word: HTML MIME .doc（Word/WPS 原生可开 HTML——零依赖,包体红线友好）
 * - PDF: 印式窗口 window.print()（WebView 系统另存 PDF——移动端标准通路,无 jsPDF 依赖）
 * - TXT: 纯文本（「等格式」语义兜底）
 * 导入: .md/.txt → 复用 mdToDoc 解析 → 替换正文（md）/追加（txt）。
 * 落盘: Blob+a[download]。
 */
import type { EditorView } from 'prosemirror-view';
import type { Node as PMNode } from 'prosemirror-model';
import { docToMd, mdToDoc } from './auroraMarkdown';
import { auroraSchema } from '../schema/auroraSchema';

/* ============================================================
 * HTML 序列化（Word/PDF 共用）
 * ============================================================ */

function inlineToHtml(node: PMNode): string {
  let out = '';
  node.forEach((child) => {
    const t = child.type.name;
    if (t === 'text') { out += escapeHtml(child.text ?? ''); return; }
    if (t === 'hard_break') { out += '<br/>'; return; }
    const inner = inlineToHtml(child);
    const attrs = child.attrs as Record<string, unknown>;
    switch (t) {
      case 'strong': out += `<b>${inner}</b>`; break;
      case 'em': out += `<i>${inner}</i>`; break;
      case 'underline': out += `<u>${inner}</u>`; break;
      case 'strikethrough': out += `<s>${inner}</s>`; break;
      case 'code': out += `<code style="font-family:monospace;background:#f1f5f9;padding:1px 4px;border-radius:3px">${inner}</code>`; break;
      case 'highlight': out += `<mark style="background:${String(attrs.color ?? '#fef08a')}">${inner}</mark>`; break;
      case 'fontColor': out += `<span style="color:${String(attrs.color ?? '#000')}">${inner}</span>`; break;
      case 'fontSize': out += `<span style="font-size:${String(attrs.size ?? '14')}px">${inner}</span>`; break;
      case 'tagMark': out += `<span style="color:#6b7280">‹${inner}›</span>`; break;
      case 'quoteMark': out += `<span style="color:#6b7280">“${inner}”</span>`; break;
      default: out += inner;
    }
  });
  return out;
}

function blockToHtml(node: PMNode, depth = 0): string {
  const parts: string[] = [];
  node.forEach((child) => {
    const t = child.type.name;
    switch (t) {
      case 'heading': {
        const lv = Math.min(6, Math.max(1, Number((child.attrs as { level?: number }).level ?? 1)));
        parts.push(`<h${lv}>${inlineToHtml(child)}</h${lv}>`);
        break;
      }
      case 'paragraph': parts.push(`<p>${inlineToHtml(child)}</p>`); break;
      case 'blockquote': parts.push(`<blockquote style="border-left:3px solid #d1d5db;margin:8px 0;padding:4px 12px;color:#4b5563">${blockToHtml(child, depth)}</blockquote>`); break;
      case 'code_block': parts.push(`<pre style="background:#f1f5f9;padding:10px;border-radius:6px;font-family:monospace;font-size:13px;white-space:pre-wrap">${escapeHtml(child.textContent)}</pre>`); break;
      case 'horizontal_rule': parts.push('<hr/>'); break;
      case 'bullet_list': parts.push(`<ul>${blockToHtml(child, depth)}</ul>`); break;
      case 'ordered_list': parts.push(`<ol>${blockToHtml(child, depth)}</ol>`); break;
      case 'list_item': parts.push(`<li>${blockToHtml(child, depth + 1)}</li>`); break;
      case 'task_block': {
        const checked = (child.attrs as { checked?: boolean }).checked;
        parts.push(`<p style="margin:2px 0"><input type="checkbox" ${checked ? 'checked' : ''} disabled/> ${inlineToHtml(child)}</p>`);
        break;
      }
      case 'table': {
        const rows: string[] = [];
        child.forEach((tr) => {
          const cells: string[] = [];
          tr.forEach((cell) => {
            const tag = cell.type.name === 'table_header' ? 'th' : 'td';
            cells.push(`<${tag} style="border:1px solid #d1d5db;padding:6px 10px;text-align:left">${inlineToHtml(cell)}</${tag}>`);
          });
          rows.push(`<tr>${cells.join('')}</tr>`);
        });
        parts.push(`<table style="border-collapse:collapse;margin:8px 0">${rows.join('')}</table>`);
        break;
      }
      case 'math_block': parts.push(`<pre style="background:#fefce8;padding:10px;border-radius:6px;font-family:monospace">${escapeHtml(child.textContent)}</pre>`); break;
      case 'mermaid_block': parts.push(`<pre style="background:#ecfdf5;padding:10px;border-radius:6px;font-family:monospace">[流程图]\n${escapeHtml(child.textContent)}</pre>`); break;
      default: {
        if (child.isTextblock) parts.push(`<p>${inlineToHtml(child)}</p>`);
        else if (child.textContent) parts.push(`<p>${escapeHtml(child.textContent)}</p>`);
        break;
      }
    }
  });
  return parts.join('\n');
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

function docShell(title: string, body: string): string {
  return `<!DOCTYPE html><html><head><meta charset="utf-8"><title>${escapeHtml(title)}</title>
<style>body{font-family:-apple-system,"PingFang SC","Microsoft YaHei",sans-serif;max-width:720px;margin:24px auto;padding:0 16px;color:#1f2937;line-height:1.65}h1,h2,h3{color:#111827}pre{overflow-x:auto}</style>
</head><body>${body}</body></html>`;
}

/** E15 真机判据: Android WebView（pdf print 死机/下载面差异的分流锚） */
function isAndroidWebView(): boolean {
  return /Android/.test(navigator.userAgent) && !/Firefox/.test(navigator.userAgent);
}

/**
 * E15 双通道 v2（TA 18:58）: share **await**——失败/取消不再静默假成功
 * （word「显示导出未见文件」根因: share reject 被吞→用户以为成了）;reject→blob 真兜底。
 * 桌面/headless 无 share→blob（既有 PW 断言面稳定）。
 */
function download(filename: string, content: string, mime: string): Promise<boolean> {
  return (async () => {
    try {
      const file = new File([content], filename, { type: mime });
      if (navigator.canShare?.({ files: [file] })) {
        try {
          await navigator.share({ files: [file], title: filename });
          return true; // 用户经系统面板保存/分享（真机文件落 Download/所选位置）
        } catch (e) {
          if (e instanceof Error && e.name === 'AbortError') return true; // 用户取消=意图,非失败
          // 真失败（无组件/组件拒绝）→blob 兜底不吞
        }
      }
    } catch { /* File/canShare 缺失防御 */ }
    const blob = new Blob([content], { type: mime });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = filename;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 3000);
    return true;
  })();
}

/* ============================================================
 * 导入
 * ============================================================ */

export function pickImportFile(): Promise<{ name: string; text: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.md,.markdown,.txt';
    input.onchange = () => {
      const f = input.files?.[0];
      if (!f) { resolve(null); return; }
      const r = new FileReader();
      r.onload = () => resolve({ name: f.name, text: String(r.result ?? '') });
      r.onerror = () => resolve(null);
      r.readAsText(f);
    };
    input.click();
  });
}

/* ============================================================
 * 对外 API
 * ============================================================ */

export type ExportFmt = 'md' | 'word' | 'txt' | 'pdf';

export interface ExportResult { ok: boolean; fmt: ExportFmt; filename: string; size: number; error?: string
  /** E18（TA 20:24）: 导出产物内容——App 内「查看」直开（不依赖系统下载/share 落盘可见性） */
  content?: string; mime?: string; }

function getView(): EditorView | null {
  const w = window as unknown as Record<string, unknown>;
  return (w.__editorView as EditorView | undefined) ?? null;
}

export async function exportNote(title: string, fmt: Exclude<ExportFmt, 'pdf'>): Promise<ExportResult> {
  const view = getView();
  if (!view) return { ok: false, fmt, filename: '', size: 0, error: '编辑器未就绪' };
  const safeName = (title.trim() || '未命名笔记').replace(/[\\/:*?"<>|]/g, '_');
  try {
    const doc = view.state.doc;
    if (fmt === 'md') {
      const md = docToMd(doc) + '\n';
      await download(`${safeName}.md`, md, 'text/markdown;charset=utf-8');
      return { ok: true, fmt, filename: `${safeName}.md`, size: md.length, content: md, mime: 'text/markdown' };
    }
    if (fmt === 'txt') {
      const txt = doc.textContent + '\n';
      await download(`${safeName}.txt`, txt, 'text/plain;charset=utf-8');
      return { ok: true, fmt, filename: `${safeName}.txt`, size: txt.length, content: txt, mime: 'text/plain' };
    }
    const html = docShell(safeName, blockToHtml(doc));
    await download(`${safeName}.doc`, html, 'application/msword;charset=utf-8');
    return { ok: true, fmt: 'word', filename: `${safeName}.doc`, size: html.length, content: html, mime: 'text/html' };
  } catch (e) {
    return { ok: false, fmt, filename: '', size: 0, error: String(e) };
  }
}

/** PDF（E15 · TA 18:58 死机修复）: Android WebView 撤 window.open+print——新 WebView
 *  窗口无 navigation 绑定=全屏死窗（「App 死掉无法退出」根因）;print 主线程阻塞雪上加霜。
 *  真机改走 share 印式 HTML（.html 真文件,系统面板保存;浏览器/印式打开即可另存 PDF）;
 *  桌面保 window.open+print 链（Chromium 印式面板存 PDF——语义不变）。 */
export async function exportNotePdf(title: string): Promise<ExportResult> {
  const view = getView();
  if (!view) return { ok: false, fmt: 'pdf', filename: '', size: 0, error: '编辑器未就绪' };
  const safeName = (title.trim() || '未命名笔记').replace(/[\\/:*?"<>|]/g, '_');
  try {
    const html = docShell(safeName, blockToHtml(view.state.doc));
    if (isAndroidWebView()) {
      const filename = `${safeName}.html`;
      await download(filename, html, 'text/html;charset=utf-8');
      return { ok: true, fmt: 'pdf', filename, size: html.length, content: html, mime: 'text/html' };
    }
    const w = window.open('', '_blank');
    if (!w) return { ok: false, fmt: 'pdf', filename: '', size: 0, error: '弹出窗口被拦截' };
    w.document.write(docShell(safeName, blockToHtml(view.state.doc)));
    w.document.close();
    w.focus();
    setTimeout(() => w.print(), 400);
    return { ok: true, fmt: 'pdf', filename: `${safeName}.pdf`, size: 0 };
  } catch (e) {
    return { ok: false, fmt: 'pdf', filename: '', size: 0, error: String(e) };
  }
}

/** 导入: md 全文替换正文;txt 逐行追加为段落（追加保内容语义）。 */
export function importNoteText(text: string, isMd: boolean): boolean {
  const view = getView();
  if (!view) return false;
  try {
    const { state } = view;
    if (isMd) {
      // mdToDoc 解析→doc 全量替换（range 0..size）
      const newDoc = mdToDoc(auroraSchema, text);
      const tr = state.tr.replaceWith(0, state.doc.content.size, newDoc.content);
      view.dispatch(tr.scrollIntoView());
      return true;
    }
    // txt: 尾部追加
    const tr = state.tr;
    const lines = text.split(/\r?\n/).filter((l) => l.trim().length > 0);
    const paragraphs = lines.map((l) => state.schema.nodes.paragraph.create(null, l ? state.schema.text(l) : undefined));
    tr.insert(state.doc.content.size, paragraphs);
    view.dispatch(tr.scrollIntoView());
    return true;
  } catch (e) {
    console.error('importNoteText failed', e);
    return false;
  }
}
