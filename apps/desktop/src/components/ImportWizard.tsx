/**
 * ImportWizard — DK-09 迁移向导前端（Bravo 授权切片）
 *
 * 边界（request `bravo-request-migration-wizard-ui` Alpha 裁决 · 2026-09-21）：
 * 仅新增本组件 + 对 import_commands 的调用接线；不触碰既有笔记视图与
 * lib.rs command 注册行。
 *
 * 流程：选源 → 预扫预览（cmd_plan_import，复选 only 白名单）→ 执行
 * （cmd_import_markdown_dir / cmd_import_enex / cmd_import_opml，可选
 * manifest_dir 防重会话目录）→ 报告。M2 语义：执行同步返回完整
 * ImportReport（progress channel 留后续切片）。
 *
 * 裁决确认的防重语义：manifest 键 (source, content_hash)，内容变更即重导。
 */
import { useCallback, useMemo, useState } from 'react';
import tokens from '../design/tokens';

interface InvokeFn {
  (cmd: string, args?: Record<string, unknown>): Promise<unknown>;
}

/** 预扫条目（Rust `PlanItem` serde 直通，snake_case 与 IPC 一致）。 */
interface PlanItem {
  source: string;
  title: string;
  bytes: number;
  resources: number;
}

interface ImportPlan {
  kind: string;
  items: PlanItem[];
  total_bytes: number;
}

/** 导入报告条目错误。 */
interface ImportError {
  path: string;
  reason: string;
}

interface ImportReport {
  scanned: number;
  imported: number;
  failed: number;
  skipped: number;
  warnings: string[];
  errors: ImportError[];
  note_ids: string[];
  duration_ms: number;
}

type Kind = 'markdown' | 'enex' | 'opml';

const KINDS: Array<{ kind: Kind; label: string; hint: string; placeholder: string }> = [
  { kind: 'markdown', label: 'Markdown 目录', hint: '逐文件 → 一篇笔记', placeholder: '/path/to/notes' },
  { kind: 'enex', label: '印象笔记 ENEX', hint: '逐 note → 一篇笔记（资源经附件挂接）', placeholder: '/path/to/export.enex' },
  { kind: 'opml', label: 'OPML 大纲', hint: '逐顶层 outline → 一篇笔记（幕布/Workflowy）', placeholder: '/path/to/outline.opml' },
];

const card: React.CSSProperties = {
  background: tokens.color.bgSurface,
  border: '1px solid rgba(255,255,255,0.10)',
  borderRadius: tokens.radius.md,
  padding: tokens.spacing.md,
};

const input: React.CSSProperties = {
  width: '100%',
  background: tokens.color.bgElevated,
  border: '1px solid rgba(255,255,255,0.10)',
  borderRadius: tokens.radius.sm,
  padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
  color: tokens.color.textPrimary,
  fontSize: tokens.typography.body.size,
  outline: 'none',
};

const primaryBtn: React.CSSProperties = {
  background: tokens.color.primary,
  color: tokens.color.textPrimary,
  border: 'none',
  borderRadius: tokens.radius.sm,
  padding: `${tokens.spacing.sm}px ${tokens.spacing.lg}px`,
  fontSize: tokens.typography.body.size,
  cursor: 'pointer',
};

const ghostBtn: React.CSSProperties = {
  ...primaryBtn,
  background: tokens.color.bgElevated,
};

/**
 * 迁移向导。`invoke` 为 null（browser-mock 模式）时展示不可用态，
 * 组件保持可渲染（不抛错，与 AppShell 容错语义一致）。
 */
export default function ImportWizard({ invoke, onClose }: { invoke: InvokeFn | null; onClose?: () => void }) {
  const [kind, setKind] = useState<Kind>('markdown');
  const [source, setSource] = useState('');
  const [manifestDir, setManifestDir] = useState('');
  const [attachmentsDir, setAttachmentsDir] = useState('');
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [phase, setPhase] = useState<'select' | 'preview' | 'running' | 'done'>('select');
  /** 提交中标志（独立于 phase，避免 JSX 内对收窄字面量的无效比较）。 */
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);

  const meta = KINDS.find((k) => k.kind === kind)!;
  const call = useCallback(
    async <T,>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
      if (!invoke) throw new Error('未运行在 Tauri 环境（browser-mock 模式不可用）');
      return (await invoke(cmd, args)) as T;
    },
    [invoke],
  );

  const reset = () => {
    setPlan(null);
    setSelected(new Set());
    setPhase('select');
    setError(null);
    setReport(null);
  };

  const runPlan = useCallback(async () => {
    setError(null);
    setPlan(null);
    try {
      const p = await call<ImportPlan>('cmd_plan_import', { source, kind });
      setPlan(p);
      setSelected(new Set(p.items.map((i) => i.source)));
      setPhase('preview');
    } catch (e) {
      setError(String(e));
    }
  }, [call, source, kind]);

  const runImport = useCallback(async () => {
    setPhase('running');
    setSubmitting(true);
    setError(null);
    const only = [...selected];
    const cmd =
      kind === 'markdown' ? 'cmd_import_markdown_dir' : kind === 'enex' ? 'cmd_import_enex' : 'cmd_import_opml';
    const args: Record<string, unknown> =
      kind === 'enex'
        ? { file: source, manifest_dir: manifestDir || null, only, attachments_dir: attachmentsDir || null }
        : kind === 'markdown'
          ? { dir: source, manifest_dir: manifestDir || null, only }
          : { file: source, manifest_dir: manifestDir || null, only };
    try {
      const r = await call<ImportReport>(cmd, args);
      setReport(r);
      setPhase('done');
    } catch (e) {
      setError(String(e));
      setPhase('preview');
    } finally {
      setSubmitting(false);
    }
  }, [call, kind, source, manifestDir, attachmentsDir, selected]);

  const selectedBytes = useMemo(
    () => plan?.items.filter((i) => selected.has(i.source)).reduce((a, i) => a + Number(i.bytes), 0) ?? 0,
    [plan, selected],
  );

  return (
    <div
      data-testid="import-wizard"
      style={{ ...card, display: 'flex', flexDirection: 'column', gap: tokens.spacing.md, maxWidth: 720 }}
    >
      <div style={{ display: 'flex', alignItems: 'baseline', gap: tokens.spacing.sm }}>
        <strong>迁移导入</strong>
        <span style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
          预扫 → 选择 → 导入（会话目录可防重）
        </span>
        {onClose && (
          <button
            style={{ ...ghostBtn, marginLeft: 'auto', padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px` }}
            onClick={() => {
              onClose();
              reset();
            }}
          >
            关闭
          </button>
        )}
      </div>

      {error && <div style={{ color: tokens.color.danger, fontSize: tokens.typography.caption.size }}>{error}</div>}

      {phase === 'select' && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.sm }}>
          <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
            {KINDS.map((k) => (
              <button
                key={k.kind}
                onClick={() => setKind(k.kind)}
                style={{
                  ...ghostBtn,
                  outline: kind === k.kind ? `2px solid ${tokens.color.focus}` : 'none',
                }}
              >
                {k.label}
              </button>
            ))}
          </div>
          <p style={{ margin: 0, color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
            {meta.hint}
          </p>
          <input style={input} placeholder={meta.placeholder} value={source} onChange={(e) => setSource(e.target.value)} />
          <input
            style={input}
            placeholder="防重会话目录（可选，如 ~/.aurora-imports/session-1）"
            value={manifestDir}
            onChange={(e) => setManifestDir(e.target.value)}
          />
          {kind === 'enex' && (
            <input
              style={input}
              placeholder="资源 sidecar 目录（可选 fallback；附件能力可用时经 attach_to_note 挂接）"
              value={attachmentsDir}
              onChange={(e) => setAttachmentsDir(e.target.value)}
            />
          )}
          <div>
            <button style={primaryBtn} disabled={!source.trim() || !invoke} onClick={runPlan}>
              预扫
            </button>
            {!invoke && (
              <span style={{ marginLeft: tokens.spacing.sm, color: tokens.color.warning, fontSize: tokens.typography.caption.size }}>
                未运行在 Tauri 环境（browser-mock）
              </span>
            )}
          </div>
        </div>
      )}

      {phase === 'preview' && plan && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.sm }}>
          <div style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
            {plan.kind} · {plan.items.length} 条 · 合计 {(plan.total_bytes / 1024).toFixed(1)} KiB
          </div>
          <div style={{ maxHeight: 280, overflow: 'auto', display: 'flex', flexDirection: 'column', gap: 2 }}>
            {plan.items.map((it) => (
              <label
                key={it.source}
                style={{ display: 'flex', gap: tokens.spacing.sm, alignItems: 'center', cursor: 'pointer', padding: `2px ${tokens.spacing.xs}px` }}
              >
                <input
                  type="checkbox"
                  checked={selected.has(it.source)}
                  onChange={(e) => {
                    const next = new Set(selected);
                    if (e.target.checked) next.add(it.source);
                    else next.delete(it.source);
                    setSelected(next);
                  }}
                />
                <span style={{ flex: 1, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{it.title}</span>
                <span style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
                  {Number(it.bytes)} B{it.resources > 0 ? ` · ${it.resources} 资源` : ''}
                </span>
              </label>
            ))}
          </div>
          <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
            <button style={primaryBtn} disabled={selected.size === 0 || submitting} onClick={runImport}>
              {submitting ? '导入中…' : `导入所选 ${selected.size} 条（${(selectedBytes / 1024).toFixed(1)} KiB）`}
            </button>
            <button style={ghostBtn} onClick={reset}>
              返回
            </button>
          </div>
        </div>
      )}

      {phase === 'done' && report && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.sm }}>
          <div>
            成功 <strong style={{ color: tokens.color.success }}>{report.imported}</strong> · 跳过{' '}
            <strong style={{ color: tokens.color.warning }}>{report.skipped}</strong> · 失败{' '}
            <strong style={{ color: report.failed > 0 ? tokens.color.danger : tokens.color.textPrimary }}>{report.failed}</strong> ·{' '}
            {report.duration_ms} ms
          </div>
          {report.errors.length > 0 && (
            <div style={{ ...card, color: tokens.color.danger, fontSize: tokens.typography.caption.size, maxHeight: 140, overflow: 'auto' }}>
              {report.errors.map((e, i) => (
                <div key={i}>{e.path}: {e.reason}</div>
              ))}
            </div>
          )}
          {report.warnings.length > 0 && (
            <div style={{ ...card, color: tokens.color.warning, fontSize: tokens.typography.caption.size, maxHeight: 140, overflow: 'auto' }}>
              {report.warnings.map((w, i) => (
                <div key={i}>{w}</div>
              ))}
            </div>
          )}
          <button style={ghostBtn} onClick={reset}>
            继续导入
          </button>
        </div>
      )}
    </div>
  );
}
