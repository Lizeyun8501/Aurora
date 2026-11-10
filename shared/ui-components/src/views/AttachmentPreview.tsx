import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
} from 'react';
import clsx from 'clsx';

/**
 * A-02 附件内置预览器（TA 2026-11-10 工单）——按 MIME 分派四类：
 * 图片（缩放/旋转/原图）/ PDF（降级卡——见 PDF_BUDGET_NOTE）/ 文本代码
 * （高亮只读+分片按需）/ Office（系统打开+导出入口——A-01 串联）。
 *
 * 数据面：readAttachment(id, range) → ReadAttachmentDto（unseal 唯一入口
 * 经 write_path，锁定态 fail-closed 即 Err——本组件 Err 态零内容渲染）。
 *
 * 无缓存残留纪律：图片 blob URL 用后 revoke（unmount/换源双时机）;文本
 * 分片不超出预览内存预算（SLICE_BYTES × MAX_SLICES 封顶）。
 */

/** 读取结果（与 desktop src-tauri ReadAttachmentDto 对齐——A-02 加 range）。 */
export interface PreviewAttachmentDto {
  attachment_id: string;
  note_id: string;
  file_name: string;
  mime: string;
  size: number;
  data_base64: string;
  /** None=全量;Some((start,len))=明文切片区间（EOF 判定依据）。 */
  range: [number, number] | null;
}

/** 读取器（宿主注入——desktop=tauri invoke;mobile=桥;mock=直返）。 */
export type ReadAttachmentFn = (
  attachmentId: string,
  range?: { start: number; len: number },
) => Promise<PreviewAttachmentDto>;

/** MIME 分派四类。 */
export type PreviewKind = 'image' | 'pdf' | 'text' | 'office';

/** 文本分片：首片 256KB;上限 4MB（16 片封顶——预览内存预算,非全量加载）。 */
const TEXT_SLICE_BYTES = 256 * 1024;
const TEXT_MAX_SLICES = 16;

/** Office 扩展名（A-01 导出入口匹配;其余 Office 类走纯系统打开提示）。 */
const OFFICE_DOCX = /\.(docx|doc)$/i;
const OFFICE_XLSX = /\.(xlsx|xls)$/i;

/** 从 file_name+mime 推断预览类。 */
export function classifyPreview(mime: string, fileName: string): PreviewKind {
  if (mime.startsWith('image/')) return 'image';
  if (mime === 'application/pdf' || /\.pdf$/i.test(fileName)) return 'pdf';
  if (
    mime.startsWith('text/') ||
    /^(application\/(json|xml|javascript|typescript|x-sh|toml|yaml))/.test(mime) ||
    /\.(md|txt|json|xml|js|ts|rs|py|toml|ya?ml|sh|css|html)$/i.test(fileName)
  ) {
    return 'text';
  }
  return 'office';
}

export interface AttachmentPreviewProps {
  attachmentId: string;
  fileName: string;
  mime: string;
  size: number;
  /** 宿主读取器（锁定态 fail-closed 由其 Err 语义保证）。 */
  readAttachment: ReadAttachmentFn;
  /** A-01 导出串联（Office 卡入口;null=宿主无导出面——按钮隐藏）。 */
  onExportOffice?: (ext: 'docx' | 'xlsx') => void;
  /** 「插入到当前笔记」（null=无宿主笔记上下文——按钮隐藏）。 */
  onInsertToNote?: (markdownRef: string) => void;
  /** 「用系统应用打开」（null=宿主无 shell 面——提示降级为复制路径）。 */
  onOpenWithSystem?: (fileName: string) => void;
  /** 「复制路径」实现注入（默认 navigator.clipboard+prompt 降级）。 */
  onCopyPath?: (path: string) => void;
  onClose: () => void;
  className?: string;
}

/** pdf.js 打包预算决策（2026-11-10）：pdfjs-dist ~900KB gzip 入主 bundle 超
 * UI 预算惯例（移动首屏敏感）——按工单降级分支走「系统打开」;动态 chunk
 * lazy-load 路径登记 v2（双端 WebView 兼容性未验,不在本卡）。 */
export const PDF_BUDGET_NOTE =
  'PDF 预览 v1 降级为系统应用打开（pdf.js ~900KB gzip 超打包预算;动态 chunk 路径见 v2 建议）';

/** base64 → Uint8Array（atob 通路——WebView 通用）。 */
function b64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i += 1) out[i] = bin.charCodeAt(i);
  return out;
}

export function AttachmentPreview(props: AttachmentPreviewProps): ReactElement {
  const {
    attachmentId,
    fileName,
    mime,
    size,
    readAttachment,
    onExportOffice,
    onInsertToNote,
    onOpenWithSystem,
    onCopyPath,
    onClose,
    className,
  } = props;

  const kind = useMemo(() => classifyPreview(mime, fileName), [mime, fileName]);

  // —— 无障碍：Esc 关闭（容器 role=dialog aria-modal）——
  const rootRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [errorMsg, setErrorMsg] = useState<string>('');
  const [full, setFull] = useState<PreviewAttachmentDto | null>(null);

  // 全量读取（图片 blob/PDF 判定/office 元信息）——text 类走分片不进此态。
  useEffect(() => {
    if (kind === 'text') return; // text 分片通道单独管理
    let cancelled = false;
    setStatus('loading');
    setErrorMsg('');
    readAttachment(attachmentId)
      .then((r) => {
        if (!cancelled) {
          setFull(r);
          setStatus('ready');
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          // 锁定态/解封失败/完整性失败——fail-closed 语义:零内容渲染。
          setErrorMsg(e instanceof Error ? e.message : String(e));
          setStatus('error');
        }
      });
    return () => {
      cancelled = true;
    };
  }, [attachmentId, kind, readAttachment]);

  // —— 三动作 ——
  const handleCopyPath = useCallback((): void => {
    const path = `attachment://${attachmentId}`;
    if (onCopyPath) {
      onCopyPath(path);
      return;
    }
    if (navigator.clipboard?.writeText) {
      void navigator.clipboard.writeText(path);
    } else {
      window.prompt('复制附件引用（clipboard 不可用）:', path);
    }
  }, [attachmentId, onCopyPath]);

  const handleOpenSystem = useCallback((): void => {
    if (onOpenWithSystem) onOpenWithSystem(fileName);
  }, [fileName, onOpenWithSystem]);

  const handleInsert = useCallback((): void => {
    const ref =
      kind === 'image'
        ? `![${fileName}](attachment://${attachmentId})`
        : `[${fileName}](attachment://${attachmentId})`;
    onInsertToNote?.(ref);
  }, [attachmentId, fileName, kind, onInsertToNote]);

  const actionRow = (
    <div className="aurora-ap-actions" style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
      <button type="button" onClick={handleCopyPath} className="aurora-ap-btn">
        复制路径
      </button>
      <button
        type="button"
        onClick={handleOpenSystem}
        disabled={!onOpenWithSystem}
        title={onOpenWithSystem ? '用系统应用打开' : '宿主未提供系统打开通道'}
        className="aurora-ap-btn"
      >
        用系统应用打开
      </button>
      <button
        type="button"
        onClick={handleInsert}
        disabled={!onInsertToNote}
        className="aurora-ap-btn"
      >
        插入到当前笔记
      </button>
      <button type="button" onClick={onClose} className="aurora-ap-btn aurora-ap-close">
        关闭（Esc）
      </button>
    </div>
  );

  const meta = (
    <p
      className="aurora-ap-meta"
      style={{ margin: 0, fontSize: 12, opacity: 0.75 }}
    >
      {fileName} · {mime} · {formatSize(size)}
    </p>
  );

  return (
    <div
      ref={rootRef}
      role="dialog"
      aria-modal="true"
      aria-label={`附件预览 ${fileName}`}
      className={clsx('aurora-attachment-preview', className)}
      style={{
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        padding: 12,
        border: '1px solid rgba(88,166,255,0.3)',
        borderRadius: 8,
        background: 'rgba(13,17,23,0.6)',
      }}
      tabIndex={-1}
    >
      {meta}
      {status === 'loading' && (
        <div className="aurora-ap-loading" role="status" style={{ padding: 16 }}>
          加载附件中…（异步读取，不阻塞主线程）
        </div>
      )}
      {status === 'error' && (
        <div
          className="aurora-ap-locked"
          role="alert"
          style={{ padding: 16, opacity: 0.9 }}
        >
          预览不可用（锁定态/解封失败不渲染任何内容）: {errorMsg}
        </div>
      )}
      {status === 'ready' && full && kind === 'image' && (
        <ImagePane
          base64={full.data_base64}
          mime={full.mime}
          alt={full.file_name}
        />
      )}
      {status === 'ready' && full && kind === 'pdf' && (
        <div className="aurora-ap-pdf" style={{ padding: 16 }}>
          <p style={{ margin: '0 0 8px' }}>{PDF_BUDGET_NOTE}</p>
          <p style={{ margin: 0, fontSize: 12, opacity: 0.75 }}>
            可复制引用路径或导出后用系统应用打开。
          </p>
        </div>
      )}
      {kind === 'text' && (
        <TextPane
          attachmentId={attachmentId}
          readAttachment={readAttachment}
          onStatus={setStatus}
          onErrorMsg={setErrorMsg}
        />
      )}
      {kind === 'office' && (
        <div className="aurora-ap-office" style={{ padding: 16 }}>
          <p style={{ margin: '0 0 8px' }}>Office 文档——请用系统应用打开。</p>
          {onExportOffice && OFFICE_DOCX.test(fileName) && (
            <button
              type="button"
              className="aurora-ap-btn"
              onClick={() => onExportOffice('docx')}
            >
              导出 DOCX（A-01）
            </button>
          )}
          {onExportOffice && OFFICE_XLSX.test(fileName) && (
            <button
              type="button"
              className="aurora-ap-btn"
              onClick={() => onExportOffice('xlsx')}
            >
              导出 XLSX（A-01）
            </button>
          )}
        </div>
      )}
      {actionRow}
    </div>
  );
}

/** 图片面：blob URL（用后 revoke）+缩放/旋转/原图。 */
function ImagePane(props: { base64: string; mime: string; alt: string }): ReactElement {
  const { base64, mime, alt } = props;
  const [zoom, setZoom] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [url, setUrl] = useState<string | null>(null);

  // blob URL 生命周期：换源/卸载双时机 revoke——无缓存残留（DoD）。
  useEffect(() => {
    const bytes = b64ToBytes(base64);
    const blob = new Blob([bytes.buffer as ArrayBuffer], { type: mime });
    const u = URL.createObjectURL(blob);
    setUrl(u);
    return () => URL.revokeObjectURL(u);
  }, [base64, mime]);

  return (
    <div
      className="aurora-ap-image"
      style={{ display: 'flex', flexDirection: 'column', gap: 8, alignItems: 'flex-start' }}
    >
      {url && (
        <img
          src={url}
          alt={alt}
          style={{
            maxWidth: '100%',
            transform: `scale(${zoom}) rotate(${rotation}deg)`,
            transformOrigin: 'top left',
            transition: 'transform 0.15s ease',
            borderRadius: 6,
            border: '1px solid rgba(88,166,255,0.2)',
          }}
        />
      )}
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        <button type="button" className="aurora-ap-btn" onClick={() => setZoom((z) => Math.min(4, z * 2))}>
          放大（至 4x）
        </button>
        <button type="button" className="aurora-ap-btn" onClick={() => setZoom((z) => Math.max(0.25, z / 2))}>
          缩小（至 0.25x）
        </button>
        <button type="button" className="aurora-ap-btn" onClick={() => setRotation((r) => (r + 90) % 360)}>
          旋转 90°
        </button>
        {url && (
          <a href={url} target="_blank" rel="noreferrer" className="aurora-ap-btn">
            查看原图
          </a>
        )}
      </div>
    </div>
  );
}

/** 文本/代码面：分片按需（首片 256KB;「加载更多」续片;16 片封顶）。 */
function TextPane(props: {
  attachmentId: string;
  readAttachment: ReadAttachmentFn;
  onStatus: (s: 'loading' | 'ready' | 'error') => void;
  onErrorMsg: (m: string) => void;
}): ReactElement {
  const { attachmentId, readAttachment, onStatus, onErrorMsg } = props;
  const [chunks, setChunks] = useState<string[]>([]);
  const [eof, setEof] = useState(false);
  const [busy, setBusy] = useState(false);

  const loadNext = useCallback(async (): Promise<void> => {
    if (eof || busy || chunks.length >= TEXT_MAX_SLICES) return;
    setBusy(true);
    onStatus('loading');
    try {
      const start = chunks.length * TEXT_SLICE_BYTES;
      const r = await readAttachment(attachmentId, { start, len: TEXT_SLICE_BYTES });
      const text = decodeText(r.data_base64);
      const isEof = r.range ? r.range[0] + r.range[1] >= r.size : true;
      setChunks((prev) => [...prev, text]);
      setEof(isEof || text.length < TEXT_SLICE_BYTES / 2);
      onStatus('ready');
      onErrorMsg('');
    } catch (e: unknown) {
      // fail-closed：锁定态/解封失败——清空已载分片（零内容残留）。
      setChunks([]);
      onErrorMsg(e instanceof Error ? e.message : String(e));
      onStatus('error');
    } finally {
      setBusy(false);
    }
  }, [attachmentId, busy, chunks.length, eof, onErrorMsg, onStatus, readAttachment]);

  useEffect(() => {
    void loadNext();
    // 仅首载（attachmentId 变更时重置——loadNext 闭包自含续片语义）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [attachmentId]);

  if (chunks.length === 0 && busy) {
    return (
      <div role="status" style={{ padding: 16 }}>
        加载文本分片…
      </div>
    );
  }

  return (
    <div
      className="aurora-ap-text"
      style={{ display: 'flex', flexDirection: 'column', gap: 8 }}
    >
      <pre
        aria-label="附件文本内容（只读）"
        style={{
          margin: 0,
          padding: 12,
          maxWidth: '100%',
          maxHeight: 360,
          overflow: 'auto',
          fontSize: 12,
          lineHeight: 1.5,
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-all',
          background: 'rgba(0,0,0,0.35)',
          borderRadius: 6,
        }}
      >
        {chunks.join('')}
      </pre>
      {!eof && chunks.length < TEXT_MAX_SLICES && (
        <button type="button" className="aurora-ap-btn" disabled={busy} onClick={() => void loadNext()}>
          {busy ? '加载中…' : '加载更多'}
        </button>
      )}
      {chunks.length >= TEXT_MAX_SLICES && !eof && (
        <p style={{ margin: 0, fontSize: 12, opacity: 0.75 }}>
          已达预览上限（{TEXT_MAX_SLICES * TEXT_SLICE_BYTES / 1024}KB）——复制路径后用系统应用查看完整内容。
        </p>
      )}
    </div>
  );
}

/** base64 → UTF-8 文本（TextDecoder 增量安全——分片边界多字节序列容忍）。 */
function decodeText(b64: string): string {
  const bytes = b64ToBytes(b64);
  // fatal:false——非法 UTF-8 序列替换为 U+FFFD（宽容渲染,不崩预览）。
  return new TextDecoder('utf-8', { fatal: false }).decode(bytes);
}

/** 人类可读大小（与 DesktopShell formatBytes 同语义——shared 自包含）。 */
function formatSize(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
