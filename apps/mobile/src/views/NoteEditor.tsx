/**
 * NoteEditor — B-02 切片三: 从 MobileApp 拆出（纯搬移零逻辑改动）。
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { platform, type NoteSummary } from '../adapters/androidPlatform';
import {I, relativeTime, type Task, type ToastAction, getTitleOverlay, setTitleOverlay, snippetCache} from '../components/primitives';
import { SyncPanel } from './SyncBits';
import { RichEditor, editorPlatform, A05BridgeEffect, a05FetchBreadcrumb } from '../components/a05';
import { SnapshotList } from '../components/SnapshotList';
import { AttachmentPreview, type PreviewAttachmentDto } from '@aurora/ui-components';
import { pathOfNote } from '../components/tree';

export function NoteEditor({ noteId, title, isFav, onToggleFav, onClose, onDeleted, onOpenQueue, onOpenConflicts, showToast, onOpenNote }: {
  noteId: string;
  title: string;
  isFav: boolean;
  onToggleFav: () => void;
  onClose: () => void;
  onDeleted: () => void;
  /** A-05: 块引用卡「跳转源块」→ 父级编辑态切笔记。 */
  onOpenNote: (noteId: string, title: string) => void;
  /** DK-50：编辑态菜单的同步两入口（App 层面板 state 下传） */
  onOpenQueue: () => void;
  onOpenConflicts: () => void;
  showToast: (t: string | ToastAction) => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const [syncOpen, setSyncOpen] = useState(false);
  const [focusMode, setFocusMode] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);
  const [snapOpen, setSnapOpen] = useState(false);
  // C-13: 附件预览（attachment:// 引用卡点击开——AttachmentPreview 共享组件挂移动）
  const [previewRef, setPreviewRef] = useState<{ id: string; label: string; isImage: boolean } | null>(null);
  const [snapSaved, setSnapSaved] = useState(false);
  const [status, setStatus] = useState<'dirty' | 'saved' | null>(null);
  // 大标题可编辑（参考图布局）— overlay 存储，所有列表同步
  const [docTitle, setDocTitle] = useState(() => getTitleOverlay(noteId) || title);

  /** E6 导入导出 handler——noteExport 模块。E14（TA 18:16）: 全链 toast 反馈+防御——
   * 真机 WebView 无 DownloadListener 时 blob 静默（「无反应」根因之一）;share 双通道
   * （noteExport）+toast 保用户可感知;lazy/导出异常 catch 兜底不再静默。 */
  // E18（TA 20:24）: 导出产物内存存+查看链——toast 短暂显示[查看],点击直开内容预览
  // （「未见导出文档」根除:内容 App 内可开,不赌系统下载/share 落盘可见性）
  const [exportPreview, setExportPreview] = useState(false);
  const [lastExport, setLastExport] = useState<{ filename: string; content: string; mime: string } | null>(null);
  // E25: 保存位置——真机桥 label 反映;无桥隐藏行
  const [exportDirLabel, setExportDirLabel] = useState('');
  const e25Bridge = (window as unknown as {
    AndroidBridge?: {
      getExportDirLabel(): string;
      pickExportDir(): void;
      resetExportDir(): void;
    };
  }).AndroidBridge;
  useEffect(() => {
    if (e25Bridge) setExportDirLabel(e25Bridge.getExportDirLabel());
  }, [e25Bridge]);
  // E25: 原生导出结果回推（真文件写完/失败——toast 感知;不依赖 noteExport share 层）
  useEffect(() => {
    (window as unknown as Record<string, unknown>).__onExportDone = (ok: boolean, path: string) => {
      showToast(ok ? `已保存: ${path || '所选位置'}` : '导出失败');
    };
    return () => { delete (window as unknown as Record<string, unknown>).__onExportDone; };
  }, []);
  const pickExportDir = () => {
    if (!e25Bridge) return;
    e25Bridge.pickExportDir();
    (window as unknown as Record<string, unknown>).__onExportDirPicked = (ok: boolean) => {
      setExportDirLabel(ok ? e25Bridge.getExportDirLabel() : exportDirLabel);
      showToast(ok ? '导出位置已保存' : '已取消');
    };
  };
  const resetExportDir = () => {
    if (!e25Bridge) return;
    e25Bridge.resetExportDir();
    setExportDirLabel(e25Bridge.getExportDirLabel());
    showToast('已恢复默认位置');
  };
  const doExport = async (fmt: 'md' | 'word' | 'txt' | 'pdf') => {
    try {
      const m = await import('@aurora/ui-components');
      const t = (docTitle || title).replace(/[\\/:*?"<>|]/g, '_');
      const r = await (fmt === 'pdf' ? m.exportNotePdf(title) : m.exportNote(title, fmt));
      setStatus(r.ok ? 'saved' : null);
      if (r.ok) {
        // E25（TA 17:32）: 真文件落盘优先——有 AndroidBridge 即调原生 exportFile
        // （自定义目录 DocumentsContract 直写/默认 Download/AuroraExports/兜底系统另存面板）;
        // PDF 走 print 链无 content 不走此路。无桥（浏览器/预览）= 原 share/blob 链。
        const bridge = (window as unknown as { AndroidBridge?: { exportFile(f: string, m: string, t: string): void } }).AndroidBridge;
        if (bridge && r.content) {
          bridge.exportFile(r.filename, r.mime ?? 'text/plain', r.content);
          return; // 原生 toast+__onExportDone 回推接管反馈（noteExport share 不再叠层）
        }
        if (r.content) { setLastExport({ filename: r.filename, content: r.content, mime: r.mime ?? 'text/plain' }); showToast({ text: `已导出 ${r.filename}`, actionLabel: '查看', onAction: () => setExportPreview(true) }); }
        else showToast(`已导出 ${r.filename}`); // 桌面 print 链无内存产物
      } else showToast(`导出失败：${r.error ?? '未知错误'}`);
    } catch { showToast('导出失败（模块加载异常）'); }
  };
  const doImport = async () => {
    try {
      const m = await import('@aurora/ui-components');
      const f = await m.pickImportFile();
      if (!f) return; // 用户取消=静默
      const isMd = /\.md$|\.markdown$/i.test(f.name);
      const ok = m.importNoteText(f.text, isMd);
      showToast(ok ? `已导入 ${f.name}` : '导入失败（内容解析异常）');
    } catch { showToast('导入失败（模块加载异常）'); }
  };

  const onTitleChange = (v: string) => {
    setDocTitle(v);
    setTitleOverlay(noteId, v.trim() || title);
  };

  return (
    <div className={`editor-page ${focusMode ? 'focus' : ''}`}>
      <div className="editor-bar">
        <button className="icon-btn" onClick={onClose} aria-label="返回">{I.back}</button>
        {/* E6: status 撑中（flex:1）——⋮ 恒最右（TA 22:26 部署令） */}
        <span className={`editor-bar-status ${status ?? ''}`} style={{ flex: 1 }}>
          {status === 'dirty' ? '保存中…' : status === 'saved' ? '已保存' : ''}
        </span>
        <button className="icon-btn" onClick={() => setMenuOpen(!menuOpen)} aria-label="更多">{I.more}</button>
      </div>

      {/* 大标题区 — 参考图: 独立行 + 可编辑 + 分割线 */}
      {/* C-04: 折叠面包屑——「笔记本名 › …」（移动端省宽形态;深层只显
          根笔记本+省略号,点笔记本名回知识库树语境）。次要素文字色=视觉克制。 */}
      {(() => {
        const path = pathOfNote(noteId);
        return path.length > 0 ? (
          <nav aria-label="位置" className="editor-crumbs" style={{ display: 'flex', alignItems: 'center', gap: 4, fontSize: 12, color: 'var(--text-tertiary)', padding: '6px 16px 0' }}>
            <span style={{ cursor: 'pointer' }} onClick={() => onClose()} role="link" tabIndex={0}
              onKeyDown={(e) => e.key === 'Enter' && onClose()}
              title={path.map((c) => c.title).join(' › ')}>
              {path[0].title}
            </span>
            {path.length > 1 && <span aria-hidden="true">› …</span>}
          </nav>
        ) : null;
      })()}
      <div className="editor-head">
        <input
          className="editor-title-input"
          value={docTitle}
          onChange={(e) => onTitleChange(e.target.value)}
          placeholder="无标题"
          aria-label="笔记标题"
        />
      </div>

      {/* E27a: 删除确认独立 sheet——脱离菜单开态（真机 backdrop 触摸竞态:菜单内嵌确认链断点防御） */}
      {confirmDel && (
        <div className="ctx-sheet-backdrop" onClick={() => setConfirmDel(false)}>
          <div className="ctx-sheet" role="alertdialog" aria-label="删除确认" onClick={(e) => e.stopPropagation()}>
            <div className="ctx-sheet-title">删除笔记「{docTitle || title || '未命名'}」？</div>
            <div className="ctx-item" style={{ textAlign: 'center', color: 'var(--text-tertiary)', fontSize: 12, cursor: 'default' }}>移入回收站（30 天后自动清除）</div>
            <button className="ctx-item danger" onClick={() => {
              snippetCache.delete(noteId);
              setConfirmDel(false);
              onDeleted(); // 软删除 — 内容进回收站（失败已在 softDelete 可见化）
            }}>确认删除</button>
            <button className="ctx-item ctx-cancel" onClick={() => setConfirmDel(false)}>取消</button>
          </div>
        </div>
      )}
      {menuOpen && (
        <>
          <div style={{ position: 'fixed', inset: 0, zIndex: 65 }} onClick={() => setMenuOpen(false)} />
          <div className="menu-pop">
            <button className="menu-item" onClick={() => { onToggleFav(); setMenuOpen(false); }}>
              {I.star} {isFav ? '取消收藏' : '收藏笔记'}
            </button>
            <button className="menu-item" onClick={() => { setSyncOpen(!syncOpen); setMenuOpen(false); }}>
              {I.sync} P2P 同步
            </button>
            <button className="menu-item" onClick={() => { onOpenQueue(); setMenuOpen(false); }}>
              {I.sync} 离线队列
            </button>
            <button className="menu-item" onClick={() => { onOpenConflicts(); setMenuOpen(false); }}>
              {I.sync} 冲突处理
            </button>
            <button className="menu-item" onClick={() => { setFocusMode(!focusMode); setMenuOpen(false); }}>
              {I.focus} {focusMode ? '退出专注模式' : '专注模式'}
            </button>
            {/* E6 导入导出（TA 22:33 派单）: Word/PDF/Markdown/TXT 导出 + md/txt 导入 */}
            <div className="menu-group-title">导入导出</div>
            {/* E25（TA 17:32）: 保存位置——默认 Download/AuroraExports+系统选取（SAF）;真机桥驱动 */}
            <div className="menu-item export-dir-row" role="group" aria-label="导出保存位置">
              <span className="export-dir-label">{exportDirLabel}</span>
              <button className="export-dir-btn" onClick={pickExportDir}>选择位置</button>
              <button className="export-dir-btn" onClick={resetExportDir}>默认</button>
            </div>
            <button className="menu-item" onClick={async () => { setMenuOpen(false); await doImport(); }}>
              {I.sync} 导入笔记（md/txt）
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('md'); }}>
              {I.sync} 导出 Markdown
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('word'); }}>
              {I.sync} 导出 Word
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('pdf'); }}>
              {I.sync} 导出 PDF
            </button>
            <button className="menu-item" onClick={() => { setMenuOpen(false); doExport('txt'); }}>
              {I.sync} 导出 TXT
            </button>
            <button className="menu-item" onClick={() => { setSnapOpen(!snapOpen); setMenuOpen(false); }}>
              {I.note} 版本历史（时间机器）
            </button>
            <button className="menu-item danger" onClick={() => { setMenuOpen(false); setConfirmDel(true); }}>
              {I.trash} 删除笔记
            </button>
          </div>
        </>
      )}

      {syncOpen && (
        <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--border)' }}>
          <SyncPanel noteId={noteId} />
        </div>
      )}

      {snapOpen && (
        <div style={{ padding: '8px 16px', borderBottom: '1px solid var(--border)' }}>
          <div style={{ fontSize: 13, opacity: 0.7, lineHeight: 1.6, marginBottom: 8 }}>
            每次保存自动留版（20 版滑动窗口）· 点版本号查看存档正文
          </div>
          <button
            className="btn btn-secondary btn-block"
            onClick={() => {
              try {
                const c = platform.getNoteContent(noteId);
                platform.saveNoteSnapshot(noteId, btoa(unescape(encodeURIComponent(c))));
                setSnapSaved(true);
              } catch { setSnapSaved(false); }
            }}
          >
            {snapSaved ? '✓ 已存档（刷新列表可见）' : '手动存档当前版本'}
          </button>
          <SnapshotList noteId={noteId} refreshKey={(snapSaved ? 1 : 0) + (snapOpen ? 10 : 0)} />
        </div>
      )}

      {/* A-05: 块引用 resolver 注入 + 跳转桥（真机 AndroidBridge;无桥=提示态不空白） */}
      {/* eslint-disable-next-line react-hooks/rules-of-hooks -- 模块级注入非 hook 消费 */}
      <A05BridgeEffect noteId={noteId} onOpenNote={onOpenNote} />
      <React.Suspense fallback={<div className="editor-loading-tip"><span className="spinner" />编辑器加载中…</div>}>
        <RichEditor
          noteId={noteId}
          platform={editorPlatform}
          mobileDock
          sourceModeToggle
          fetchBreadcrumb={a05FetchBreadcrumb}
          fallbackText={(() => { try { return platform.getNoteContent(noteId); } catch { return ''; } })()}
          onDirty={() => setStatus('dirty')}
          onSaved={() => setStatus('saved')}
        />
      </React.Suspense>

      {/* C-13: 附件条（正文 attachment:// 引用——与桌面 AttachmentStrip 同模式
          移动简化: 无 shell 导出/系统打开,预览主链走 platform.readAttachment 桥） */}
      <MobileAttachmentStrip
        content={(() => { try { return platform.getNoteContent(noteId); } catch { return ''; } })()}
        onOpen={(r) => setPreviewRef(r)}
      />
      {previewRef && (
        <>
          <div className="sheet-mask" onClick={() => setPreviewRef(null)} />
          <div className="sheet" style={{ maxWidth: '100%', padding: 0 }}>
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">附件预览</span>
              <button className="icon-btn" onClick={() => setPreviewRef(null)} aria-label="关闭">{I.x}</button>
            </div>
            <div className="sheet-body">
              <MobileAttachmentPreviewPanel
                attachmentId={previewRef.id}
                fallbackName={previewRef.label}
                onClose={() => setPreviewRef(null)}
              />
            </div>
          </div>
        </>
      )}

      {/* E18 导出查看 Sheet: 内容直开（html→iframe 渲染/md·txt→pre 文本） */}
      {exportPreview && lastExport && (
        <>
          <div className="sheet-mask" onClick={() => { setExportPreview(false); setLastExport(null); }} />
          <div className="sheet export-preview-sheet">
            <div className="sheet-grab" />
            <div className="sheet-head">
              <span className="sheet-title">{lastExport.filename}</span>
              <button className="icon-btn" onClick={() => { setExportPreview(false); setLastExport(null); }} aria-label="关闭">{I.x}</button>
            </div>
            <div className="sheet-body export-preview-body">
              {/text\/html/.test(lastExport.mime) ? (
                <iframe title="导出预览" srcDoc={lastExport.content} style={{ width: '100%', height: '100%', border: 'none', borderRadius: 8, background: 'var(--preview-paper)' }} />
              ) : (
                <pre style={{ margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word', fontSize: 13, lineHeight: 1.7, fontFamily: 'inherit' }}>{lastExport.content}</pre>
              )}
            </div>
          </div>
        </>
      )}
    </div>
  );
}

// ===== C-13: 移动附件面（attachment:// 引用卡+预览面板——桌面 AttachmentStrip 对称） =====

interface MobileAttachmentRef {
  id: string;
  label: string;
  isImage: boolean;
}

/** 附件引用解析（桌面 ATTACHMENT_LINK_RE 同模式——白名单 id 防注入）。 */
const C13_ATTACHMENT_LINK_RE = /(!?)\[([^\]\n]*)\]\(attachment:\/\/([^)\s]+)\)/g;
export function extractAttachmentRefs(content: string): MobileAttachmentRef[] {
  const seen = new Map<string, MobileAttachmentRef>();
  for (const m of content.matchAll(C13_ATTACHMENT_LINK_RE)) {
    const [, bang, label, id] = m;
    if (seen.has(id)) continue;
    if (!/^[A-Za-z0-9_-]{1,128}$/.test(id)) continue;
    seen.set(id, { id, label: label || id, isImage: bang === '!' });
  }
  return [...seen.values()];
}

function formatBytesC13(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** 附件条：无引用不渲染;图片卡缩略+文件卡通用（点击开预览 sheet）。 */
function MobileAttachmentStrip(props: { content: string; onOpen: (r: MobileAttachmentRef) => void }) {
  const refs = React.useMemo(() => extractAttachmentRefs(props.content), [props.content]);
  if (refs.length === 0) return null;
  return (
    <div style={{
      display: 'flex', flexDirection: 'column', gap: 8,
      // 底部避让 fixed 编辑工具栏（48px 视觉高+8 余量——F-02: 列表尾 chip 被遮挡不可点）
      padding: '0 14px calc(56px + env(safe-area-inset-bottom))',
    }}>
      {refs.map((r) => (
        <button
          key={r.id}
          type="button"
          onClick={() => props.onOpen(r)}
          aria-label={`预览附件 ${r.label}`}
          style={{
            display: 'flex', alignItems: 'center', gap: 8,
            padding: '8px 12px', borderRadius: 10,
            border: '1px solid var(--export-border)',
            background: 'var(--export-bg)',
            color: 'inherit', cursor: 'pointer', textAlign: 'left', fontSize: 13,
          }}
        >
          <span aria-hidden>{r.isImage ? '🖼️' : '📎'}</span>
          <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
            {r.isImage ? r.label : `📎 ${r.label}`}
          </span>
          <span style={{ marginLeft: 'auto', opacity: 0.6, fontSize: 11 }}>预览</span>
        </button>
      ))}
    </div>
  );
}

/**
 * 预览面板：AttachmentPreview 共享组件挂移动桥（fail-closed Err 语义）。
 * 组件 kind 分派（image/pdf/text）依赖 props.mime——移动无 list 元信息面,
 * 先探测 DTO 拿真值（file_name/mime/size）再挂载;探测失败显式错误条
 * （空标签/空 MIME 硬挂会被 classify 误判 office——禁静默降级）。
 */
function MobileAttachmentPreviewPanel(props: {
  attachmentId: string;
  fallbackName: string;
  onClose: () => void;
}) {
  const [meta, setMeta] = React.useState<{ fileName: string; mime: string; size: number } | null>(null);
  const [probeError, setProbeError] = React.useState<string | null>(null);
  const readAttachment = React.useCallback(
    async (attachmentId: string, range?: { start: number; len: number }): Promise<PreviewAttachmentDto> => {
      const dto = platform.readAttachment(attachmentId, range);
      if (!dto) throw new Error('附件读取失败（未装配或已移动）');
      return dto;
    },
    [],
  );
  // 探测：全量 DTO 单次取（text 类组件内再分片——range 通道由组件驱动）
  React.useEffect(() => {
    let cancelled = false;
    readAttachment(props.attachmentId)
      .then((dto) => {
        if (!cancelled) {
          if (!dto.mime) {
            setProbeError('附件元信息缺失（mime 空——数据异常）');
          } else {
            setMeta({ fileName: dto.file_name || props.fallbackName, mime: dto.mime, size: dto.size });
          }
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) setProbeError(e instanceof Error ? e.message : String(e));
      });
    return () => { cancelled = true; };
  }, [props.attachmentId, props.fallbackName, readAttachment]);

  if (probeError) {
    return (
      <div role="alert" style={{ padding: 16, color: 'var(--danger)', fontSize: 13 }}>
        附件预览失败: {probeError}
      </div>
    );
  }
  if (!meta) {
    return <div style={{ padding: 16, opacity: 0.6, fontSize: 13 }}>附件加载中…</div>;
  }
  return (
    <AttachmentPreview
      attachmentId={props.attachmentId}
      fileName={meta.fileName}
      mime={meta.mime}
      size={meta.size}
      readAttachment={readAttachment}
      onClose={props.onClose}
    />
  );
}
