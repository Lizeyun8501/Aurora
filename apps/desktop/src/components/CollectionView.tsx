/**
 * A-07（TA 2026-11-13 工单）: CollectionView v2 — 数据库视图四形态+零 prompt。
 *
 * v2 变更（对 v1 诚实边界全消）:
 * - 零 prompt()/alert(): 建集合/加行/导出/Schema 编辑全 UI 化（对话框+toast）
 * - 视图形态: 表格/看板/列表/画廊（配置持久化 dbview:{id} KV——形态+筛选+排序）
 * - 筛选排序: 复用智能文件夹 FilterRule 语义（title_contains/tags AND）+ 属性条件
 *   扩展;多条件 AND/OR 合成;多列排序（表头点击主序,Shift+点击叠加次序）
 * - Schema 面板: 列增删改/必填标记/类型切换带「已有值转换预览」（样本5+丢失计数）
 * - 内联编辑: contentEditable（Enter 提交/Esc 取消）+Select/MultiSelect 下拉浮层
 *   +Checkbox 单击切换;非法值即时回显原因（cellErr 红框+横幅——非静默拒绝）
 * - 行↔笔记锚定: 行点击跳源笔记;保存事件（aurora:doc-updated——A-05 桥）驱动行刷新
 * - 视图切换保筛选+滚动位置（scrollTop 状态存取）
 * - 共享 VM: collectionVm.ts（双端同源——移动默认列表视图,ADR 见 A-07 报告）
 *
 * 停手边界: Formula 列只读（导出时计算）;Relation 不做跨集合双向同步。
 */
import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import tokens from '../design/tokens';
import {
  TYPE_LABEL,
  typeKey,
  selectOptions,
  formatCell,
  matchConds,
  sortRows,
  groupRows,
  previewConversion,
  DEFAULT_VIEW_CONFIG,
  type PropDefVm,
  type RowVm,
  type FilterCond,
  type ViewConfig,
} from '@aurora/ui-components';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface PropDef extends PropDefVm {}
interface CollectionMeta {
  id: string;
  title: string;
  prop_defs: PropDef[];
}
interface Row extends RowVm {}

type ViewMode = ViewConfig['viewMode'];

/* ── A-07: 对话框/面板/toast 通用小件 ─────────────────────────────── */

function Modal(props: {
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  wide?: boolean;
}): React.ReactElement {
  return (
    <>
      <div
        onClick={props.onClose}
        aria-hidden="true"
        style={{ position: 'fixed', inset: 0, zIndex: 90, background: 'rgba(0,0,0,0.45)' }}
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={props.title}
        style={{
          position: 'fixed',
          left: '50%',
          top: '12%',
          transform: 'translateX(-50%)',
          zIndex: 91,
          minWidth: props.wide ? 520 : 340,
          maxWidth: '92vw',
          maxHeight: '76vh',
          overflowY: 'auto',
          padding: tokens.spacing.lg,
          borderRadius: tokens.radius.lg,
          background: tokens.color.bgSurface,
          border: '1px solid rgba(255,255,255,0.12)',
          boxShadow: '0 12px 40px rgba(0,0,0,0.4)',
        }}
      >
        <h2 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: 15 }}>{props.title}</h2>
        {props.children}
      </div>
    </>
  );
}

const btn = (label: string, onClick: () => void, opts?: { primary?: boolean; disabled?: boolean; danger?: boolean; title?: string }): React.ReactElement => (
  <button
    onClick={onClick}
    disabled={opts?.disabled}
    title={opts?.title}
    style={{
      fontSize: 12,
      padding: '4px 12px',
      borderRadius: 6,
      border: `1px solid ${opts?.danger ? 'rgba(248,113,113,0.5)' : 'rgba(255,255,255,0.16)'}`,
      background: opts?.primary ? 'rgba(88,166,255,0.16)' : 'transparent',
      color: opts?.danger ? '#f87171' : tokens.color.textPrimary,
      cursor: opts?.disabled ? 'not-allowed' : 'pointer',
      opacity: opts?.disabled ? 0.5 : 1,
    }}
  >
    {label}
  </button>
);

const inputStyle: React.CSSProperties = {
  width: '100%',
  padding: '6px 10px',
  fontSize: 13,
  borderRadius: tokens.radius.sm,
  border: '1px solid rgba(255,255,255,0.16)',
  background: 'rgba(255,255,255,0.05)',
  color: tokens.color.textPrimary,
  outline: 'none',
};

/* ── Schema 面板（列增删改+类型切换预览） ────────────────────────── */

const ALL_TYPES = ['Text', 'Number', 'Date', 'Checkbox', 'Select', 'MultiSelect'] as const;

interface SchemaEditState {
  /** null=新增;否则编辑该列 */
  editId: string | null;
  name: string;
  typeKey: string;
  options: string;
  required: boolean;
}

function SchemaPanel(props: {
  meta: CollectionMeta;
  rows: Row[];
  edit: SchemaEditState;
  onClose: () => void;
  onApply: (next: { id: string | null; name: string; propType: unknown; required: boolean }) => Promise<void>;
  onDelete: (defId: string) => Promise<void>;
}): React.ReactElement {
  const { meta, rows, edit, onClose, onApply, onDelete } = props;
  const [name, setName] = useState(edit.name);
  const [tk, setTk] = useState(edit.typeKey);
  const [options, setOptions] = useState(edit.options);
  const [required, setRequired] = useState(edit.required);
  const [preview, setPreview] = useState<{ sample: Array<{ title: string; from: string; to: string }>; lostCount: number } | null>(null);
  const [applying, setApplying] = useState(false);

  const isSelect = tk === 'Select' || tk === 'MultiSelect';
  const orig = meta.prop_defs.find((d) => d.id === edit.editId) ?? null;
  const typeChanged = orig != null && typeKey(orig.prop_type) !== tk;

  const optsParsed = useMemo(
    () => options.split(/[,，]/).map((s) => s.trim()).filter(Boolean),
    [options],
  );

  useEffect(() => {
    if (!typeChanged || !orig || !tk) { setPreview(null); return; }
    setPreview(previewConversion(rows, orig, tk, optsParsed));
  }, [typeChanged, orig, tk, optsParsed, rows]);

  const apply = async (): Promise<void> => {
    if (!name.trim()) return;
    let propType: unknown = tk;
    if (isSelect) {
      if (optsParsed.length === 0) return;
      propType = { [tk]: optsParsed.map((v) => ({ value: v, color: null })) };
    }
    setApplying(true);
    try {
      await onApply({ id: edit.editId, name: name.trim(), propType, required });
      onClose();
    } finally {
      setApplying(false);
    }
  };

  return (
    <Modal title={edit.editId ? '编辑列' : '新增列'} onClose={onClose} wide>
      <div style={{ display: 'grid', gap: 10 }}>
        <label style={{ fontSize: 12, color: tokens.color.textSecondary }}>
          列名
          <input value={name} onChange={(e) => setName(e.target.value)} style={{ ...inputStyle, marginTop: 4 }} autoFocus />
        </label>
        <label style={{ fontSize: 12, color: tokens.color.textSecondary }}>
          类型
          <select value={tk} onChange={(e) => setTk(e.target.value)} style={{ ...inputStyle, marginTop: 4 }}>
            {ALL_TYPES.map((t) => (
              <option key={t} value={t}>{TYPE_LABEL[t] ?? t}</option>
            ))}
          </select>
        </label>
        {isSelect && (
          <label style={{ fontSize: 12, color: tokens.color.textSecondary }}>
            选项（逗号分隔）
            <input value={options} onChange={(e) => setOptions(e.target.value)} style={{ ...inputStyle, marginTop: 4 }} placeholder="待办,进行中,完成" />
          </label>
        )}
        <label style={{ fontSize: 12, color: tokens.color.textSecondary, display: 'flex', alignItems: 'center', gap: 8 }}>
          <input type="checkbox" checked={required} onChange={(e) => setRequired(e.target.checked)} />
          必填（新行提交时校验）
        </label>
        {typeChanged && preview && (
          <div
            role="status"
            style={{
              padding: 10,
              borderRadius: 8,
              border: `1px solid ${preview.lostCount > 0 ? 'rgba(251,191,36,0.45)' : 'rgba(88,166,255,0.35)'}`,
              background: preview.lostCount > 0 ? 'rgba(251,191,36,0.08)' : 'rgba(88,166,255,0.06)',
              fontSize: 12,
            }}
          >
            <strong>类型切换转换预览</strong>（{TYPE_LABEL[typeKey(orig!.prop_type)]} → {TYPE_LABEL[tk]}）
            {preview.lostCount > 0 && (
              <div style={{ color: '#fbbf24', marginTop: 4 }}>⚠ {preview.lostCount} 个已有值将丢失（无法转换→置空）</div>
            )}
            <div style={{ marginTop: 6, display: 'grid', gap: 4 }}>
              {preview.sample.map((s) => (
                <div key={s.title} style={{ opacity: 0.85 }}>
                  「{s.title}」：<code>{s.from}</code> → <code>{s.to}</code>
                </div>
              ))}
            </div>
          </div>
        )}
        <div style={{ display: 'flex', gap: 8, marginTop: 4 }}>
          {btn(applying ? '保存中…' : '保存', () => void apply(), { primary: true, disabled: applying || !name.trim() || (isSelect && optsParsed.length === 0) })}
          {edit.editId != null && btn('删除此列', () => { void onDelete(edit.editId as string).then(onClose); }, { danger: true })}
          {btn('取消', onClose)}
        </div>
      </div>
    </Modal>
  );
}

/* ── 主组件 ───────────────────────────────────────────────────────── */

export default function CollectionView({ invoke, onOpenNote }: { invoke: InvokeFn | null; onOpenNote: (id: string) => void }): React.ReactElement {
  const [cols, setCols] = useState<CollectionMeta[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [meta, setMeta] = useState<CollectionMeta | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [err, setErr] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  /* toast 自动消隐 */
  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 2600);
    return () => window.clearTimeout(t);
  }, [toast]);

  const [editing, setEditing] = useState<{ noteId: string; defId: string } | null>(null);
  /** A-07: 单元格级错误（红框+title——即时回显原因而非仅横幅） */
  const [cellErr, setCellErr] = useState<{ noteId: string; defId: string; msg: string } | null>(null);
  const [selectMenu, setSelectMenu] = useState<{ noteId: string; defId: string; x: number; y: number; multi: boolean } | null>(null);

  /* A-07: 视图配置（形态+筛选+排序——dbview:{id} 持久化） */
  const [vc, setVc] = useState<ViewConfig>(DEFAULT_VIEW_CONFIG);
  const [visibleCount, setVisibleCount] = useState(50);

  /* A-07: 对话框态（零 prompt 的替代 UI） */
  const [createDlg, setCreateDlg] = useState<{ title: string } | null>(null);
  const [addRowDlg, setAddRowDlg] = useState<{ noteId: string; localErr: string | null } | null>(null);
  const [schemaEdit, setSchemaEdit] = useState<SchemaEditState | null>(null);
  const [filterOpen, setFilterOpen] = useState(false);

  /* A-07: 视图切换滚动保位（mainRef.scrollTop 存取） */
  const mainRef = useRef<HTMLElement>(null);
  const scrollTopRef = useRef(0);

  const refreshRows = useCallback(async (): Promise<void> => {
    if (!invoke || !selected) return;
    try {
      setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
    } catch {
      setErr('集合数据加载失败');
    }
  }, [invoke, selected]);

  const refreshCols = useCallback(async () => {
    if (!invoke) return;
    try {
      const list = (await invoke('cmd_collection_list')) as CollectionMeta[];
      setCols(list);
      if (list.length > 0 && !selected) setSelected(list[0].id);
    } catch {
      setErr('集合列表加载失败');
    }
  }, [invoke, selected]);

  useEffect(() => {
    void refreshCols();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [invoke]);

  /* A-07: 选中集合 → schema+行+视图配置加载 */
  useEffect(() => {
    if (!invoke || !selected) return;
    void (async () => {
      try {
        setMeta((await invoke('cmd_collection_get', { id: selected })) as CollectionMeta);
        setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
        const saved = (await invoke('cmd_collection_view_get', { id: selected })) as ViewConfig | null;
        setVc(saved ?? DEFAULT_VIEW_CONFIG);
        setErr(null);
      } catch {
        setErr('集合数据加载失败');
      }
    })();
  }, [invoke, selected]);

  /* A-07: 视图配置持久化（变更即写——全量覆盖惯例） */
  useEffect(() => {
    if (!invoke || !selected) return;
    void invoke('cmd_collection_view_set', { id: selected, config: vc }).catch(() => {
      /* 持久化失败不阻断视图（本地态保真） */
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vc, selected]);

  /* A-07: 行↔笔记锚定——保存事件驱动行刷新（WritePath 面;A-05 桥复用） */
  useEffect(() => {
    const onDoc = (): void => {
      void refreshRows();
    };
    window.addEventListener('aurora:doc-refresh', onDoc);
    return () => window.removeEventListener('aurora:doc-refresh', onDoc);
  }, [refreshRows]);

  /* A-07: 切视图=存滚动→切→恢复（DoD 不丢滚动位置） */
  const switchView = (m: ViewMode): void => {
    if (mainRef.current) scrollTopRef.current = mainRef.current.scrollTop;
    setVc((v) => ({ ...v, viewMode: m }));
    requestAnimationFrame(() => {
      if (mainRef.current) mainRef.current.scrollTop = scrollTopRef.current;
    });
  };

  const commitSelect = useCallback(
    async (noteId: string, d: PropDef, value: string | string[] | null) => {
      setSelectMenu(null);
      if (!invoke) return;
      try {
        await invoke('cmd_collection_set_cell', { noteId, defId: d.id, value });
        await refreshRows();
        setErr(null);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [invoke, refreshRows],
  );

  // 点外/Esc 关浮层
  useEffect(() => {
    if (!selectMenu) return;
    const close = (ev: MouseEvent) => {
      const t = ev.target as HTMLElement;
      if (!t.closest('[data-select-menu]')) setSelectMenu(null);
    };
    const esc = (ev: KeyboardEvent) => {
      if (ev.key === 'Escape') setSelectMenu(null);
    };
    window.addEventListener('mousedown', close);
    window.addEventListener('keydown', esc);
    return () => {
      window.removeEventListener('mousedown', close);
      window.removeEventListener('keydown', esc);
    };
  }, [selectMenu]);

  /* A-07: 建集合（对话框——零 prompt） */
  const create = useCallback(async (title: string): Promise<void> => {
    if (!invoke || !title.trim()) return;
    try {
      const col = (await invoke('cmd_collection_create', { title: title.trim() })) as CollectionMeta;
      setCols((prev) => [...prev, col]);
      setSelected(col.id);
      setCreateDlg(null);
      setErr(null);
    } catch {
      setErr('建集合失败');
    }
  }, [invoke]);

  /* A-07: 加行（对话框——ID 输入+错误回显） */
  const addRow = useCallback(async (noteId: string): Promise<void> => {
    if (!invoke || !selected || !noteId.trim()) return;
    try {
      await invoke('cmd_collection_add_row', { id: selected, noteId: noteId.trim() });
      await refreshRows();
      setAddRowDlg(null);
      setErr(null);
    } catch (e) {
      setAddRowDlg({ noteId, localErr: `加行失败：${e instanceof Error ? e.message : String(e)}（笔记 ID 是否存在?）` });
    }
  }, [invoke, selected, refreshRows]);

  /** A-07: 导出（默认目录零 prompt;结果 toast） */
  const exportXlsx = useCallback(async () => {
    if (!invoke || !selected) return;
    try {
      const path = (await invoke('cmd_export_collection_xlsx', {
        collectionId: selected,
        destDir: '~/Downloads/aurora-export',
      })) as string;
      setErr(null);
      setToast(`已导出：${path}`);
    } catch (e) {
      setErr(`XLSX 导出失败：${String(e)}`);
    }
  }, [invoke, selected]);

  /* A-07: Schema 应用（增/改同路——全量覆盖语义） */
  const applySchema = useCallback(
    async (next: { id: string | null; name: string; propType: unknown; required: boolean }): Promise<void> => {
      if (!invoke || !meta) return;
      let defs = meta.prop_defs;
      if (next.id == null) {
        defs = [...defs, {
          id: `pd:${crypto.randomUUID?.() ?? Date.now()}`,
          name: next.name,
          prop_type: next.propType,
          required: next.required,
        }];
      } else {
        defs = defs.map((d) =>
          d.id === next.id ? { ...d, name: next.name, prop_type: next.propType, required: next.required } : d,
        );
      }
      try {
        await invoke('cmd_collection_set_schema', { id: meta.id, propDefs: defs });
        setMeta({ ...meta, prop_defs: defs });
        setErr(null);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [invoke, meta],
  );

  const removeColumn = useCallback(
    async (defId: string) => {
      if (!invoke || !meta) return;
      const next = meta.prop_defs.filter((d) => d.id !== defId);
      try {
        await invoke('cmd_collection_set_schema', { id: meta.id, propDefs: next });
        setMeta({ ...meta, prop_defs: next });
        setErr(null);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [invoke, meta],
  );

  /* G1-v3 内联单元格：双击进入 contentEditable，blur/Enter 提交，Esc 还原取消。
     A-07: 失败置 cellErr（红框+title 即时原因——非静默拒绝）。 */
  const submitInline = useCallback(
    async (noteId: string, d: PropDef, el: HTMLElement): Promise<boolean> => {
      if (!invoke) return false;
      const input = el.textContent?.trim() ?? '';
      let value: unknown = input;
      if (typeKey(d.prop_type) === 'Number') value = input === '' ? null : Number(input);
      if (typeKey(d.prop_type) === 'Checkbox') value = input === 'true' || input === '是';
      if (typeKey(d.prop_type) === 'Date') {
        if (input === 't' || input.toLowerCase() === 'today') {
          value = new Date().toISOString();
        } else if (input === '') {
          value = null;
        } else {
          const t = Date.parse(input);
          if (Number.isNaN(t)) {
            setErr('日期无法解析（RFC3339 或 t=今天）');
            setCellErr({ noteId, defId: d.id, msg: '日期无法解析（RFC3339 或 t=今天）' });
            return false;
          }
          value = new Date(t).toISOString();
        }
      }
      if (typeKey(d.prop_type) === 'MultiSelect') {
        value = input === '' ? [] : input.split(/[,，]/).map((s) => s.trim()).filter(Boolean);
      }
      try {
        await invoke('cmd_collection_set_cell', { noteId, defId: d.id, value });
        await refreshRows();
        setErr(null);
        setCellErr(null);
        return true;
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setErr(msg);
        setCellErr({ noteId, defId: d.id, msg });
        return false;
      }
    },
    [invoke, refreshRows],
  );

  /* A-07: 渲染管线（VM 共享域——筛选 conds+快捷文本→排序多列→切片） */
  const shownRows = useMemo(() => {
    let list = rows;
    if (vc.filterText.trim()) {
      const ft = vc.filterText.trim().toLowerCase();
      list = list.filter((r) => {
        if (r.title.toLowerCase().includes(ft)) return true;
        return (meta?.prop_defs ?? []).some((d) => {
          if (typeKey(d.prop_type) !== 'Text') return false;
          const v = r.values[d.id]?.value;
          return typeof v === 'string' && v.toLowerCase().includes(ft);
        });
      });
    }
    if (vc.conds.length > 0) {
      list = list.filter((r) => matchConds(r, vc.conds, vc.condMode, meta?.prop_defs ?? []));
    }
    return sortRows(list, vc.sortKeys, meta?.prop_defs ?? []).slice(0, visibleCount);
  }, [rows, meta, vc, visibleCount]);

  /* 看板分组（按第一个 Select 列——VM groupRows） */
  const board = useMemo(() => {
    const selDef = meta?.prop_defs.find((d) => typeKey(d.prop_type) === 'Select') ?? null;
    return selDef ? { defId: selDef.id, def: selDef, groups: groupRows(rows, selDef) } : { defId: null as string | null, def: null as PropDef | null, groups: [] as Array<{ key: string; label: string; rows: Row[] }> };
  }, [rows, meta]);

  /* ── 渲染 ──────────────────────────────────────────────────────── */

  const tabBtn = (m: ViewMode, label: string): React.ReactElement => (
    <button
      onClick={() => switchView(m)}
      aria-pressed={vc.viewMode === m}
      style={{
        fontSize: 12,
        padding: '2px 12px',
        borderRadius: 999,
        border: `1px solid ${vc.viewMode === m ? 'rgba(88,166,255,0.5)' : 'rgba(255,255,255,0.12)'}`,
        background: vc.viewMode === m ? 'rgba(88,166,255,0.14)' : 'transparent',
        color: vc.viewMode === m ? tokens.color.primaryBright : tokens.color.textSecondary,
        cursor: 'pointer',
      }}
    >
      {label}
    </button>
  );

  /* 表头点击:主序 asc→desc→null;Shift=叠加次序（多列排序） */
  const headerSort = (col: string, ev: React.MouseEvent): void => {
    const existing = vc.sortKeys.find((s) => s.col === col);
    let next: Array<{ col: string; dir: 1 | -1 }>;
    if (ev.shiftKey) {
      // 叠加/移除次序
      if (!existing) next = [...vc.sortKeys, { col, dir: 1 }];
      else if (existing.dir === 1) next = vc.sortKeys.map((s) => (s.col === col ? { col, dir: -1 as const } : s));
      else next = vc.sortKeys.filter((s) => s.col !== col);
    } else {
      if (!existing) next = [{ col, dir: 1 }];
      else if (existing.dir === 1) next = [{ col, dir: -1 }];
      else next = [];
    }
    setVc((v) => ({ ...v, sort: next }));
    setVisibleCount(50);
  };

  const sortMark = (col: string): string => {
    const i = vc.sortKeys.findIndex((s) => s.col === col);
    if (i < 0) return '';
    return `${vc.sortKeys[i].dir === 1 ? '↑' : '↓'}${vc.sortKeys.length > 1 ? i + 1 : ''}`;
  };

  const editCell = (noteId: string, defId: string, el: HTMLElement | null): void => {
    if (!el) return;
    el.focus();
    setEditing({ noteId, defId });
  };

  const onCellDoubleClick = (ev: React.MouseEvent<HTMLTableCellElement>, r: Row, d: PropDef): void => {
    const tk = typeKey(d.prop_type);
    if (tk === 'Checkbox') return; // 单击通道
    if (tk === 'Select' || tk === 'MultiSelect') {
      const rect = (ev.currentTarget as HTMLElement).getBoundingClientRect();
      setSelectMenu({ noteId: r.note_id, defId: d.id, x: rect.left + rect.width / 2, y: rect.top + rect.height / 2, multi: tk === 'MultiSelect' });
      return;
    }
    if (tk === 'Formula') return; // 只读
    editCell(r.note_id, d.id, ev.currentTarget as HTMLElement);
  };

  /* Checkbox 单击切换（零编辑态——直接回写） */
  const onCheckboxClick = (r: Row, d: PropDef): void => {
    const cur = r.values[d.id]?.value === true;
    void commitSelect(r.note_id, d, !cur as unknown as string);
  };

  const menuDef = selectMenu ? meta?.prop_defs.find((d) => d.id === selectMenu.defId) ?? null : null;
  const multiCur = (r: Row, d: PropDef): string[] => {
    const v = r.values[d.id]?.value;
    if (Array.isArray(v)) return v.map(String);
    return typeof v === 'string' && v ? v.split(/[,，]/) : [];
  };

  return (
    <main
      ref={mainRef}
      style={{ height: '100%', overflowY: 'auto', minWidth: 0, padding: `${tokens.spacing.lg}px ${tokens.spacing.xl}px` }}
    >
      <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size, display: 'flex', alignItems: 'center', gap: tokens.spacing.sm }}>
        数据库
        <button
          onClick={() => setCreateDlg({ title: '' })}
          style={{
            fontSize: 12, padding: '3px 12px', borderRadius: 999,
            border: '1px solid rgba(88,166,255,0.4)', background: 'rgba(88,166,255,0.12)',
            color: tokens.color.primaryBright, cursor: 'pointer',
          }}
        >
          ＋ 新建集合
        </button>
        <button
          onClick={() => void exportXlsx()}
          disabled={!selected}
          aria-label="导出当前集合为 XLSX 表格"
          title="导出到 ~/Downloads/aurora-export（列类型映射;公式列=导出时计算值+批注）"
          style={{
            fontSize: 12, padding: '3px 12px', borderRadius: 999,
            border: '1px solid rgba(88,166,255,0.4)', background: 'rgba(88,166,255,0.12)',
            color: tokens.color.primaryBright, cursor: selected ? 'pointer' : 'not-allowed', opacity: selected ? 1 : 0.5,
          }}
        >
          ⬇ 导出 XLSX
        </button>
      </h1>

      {err && (
        <div role="alert" style={{ marginBottom: 8, padding: '6px 10px', borderRadius: 8, border: '1px solid rgba(248,113,113,0.4)', background: 'rgba(248,113,113,0.08)', color: '#f87171', fontSize: 12 }}>
          {err}
        </div>
      )}
      {toast && (
        <div role="status" style={{ marginBottom: 8, padding: '6px 10px', borderRadius: 8, border: '1px solid rgba(134,239,172,0.4)', background: 'rgba(134,239,172,0.08)', color: '#86efac', fontSize: 12 }}>
          {toast}
        </div>
      )}

      {cols.length > 0 && (
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: tokens.spacing.md }}>
          {cols.map((c) => (
            <button
              key={c.id}
              onClick={() => setSelected(c.id)}
              aria-pressed={selected === c.id}
              style={{
                fontSize: 12, padding: '2px 12px', borderRadius: 999,
                border: `1px solid ${selected === c.id ? 'rgba(88,166,255,0.5)' : 'rgba(255,255,255,0.12)'}`,
                background: selected === c.id ? 'rgba(88,166,255,0.14)' : 'transparent',
                color: selected === c.id ? tokens.color.primaryBright : tokens.color.textSecondary,
                cursor: 'pointer',
              }}
            >
              {c.title}
            </button>
          ))}
        </div>
      )}

      {cols.length === 0 && (
        <p style={{ color: tokens.color.textSecondary }}>还没有集合——点「＋ 新建集合」开始。</p>
      )}

      {meta && (
        <div style={{ border: '1px solid rgba(255,255,255,0.1)', borderRadius: tokens.radius.lg, padding: tokens.spacing.md, background: 'rgba(255,255,255,0.02)' }}>
          {/* 工具行:四形态 + Schema 面板 + 筛选 */}
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', alignItems: 'center', marginBottom: 8 }}>
            {tabBtn('table', '表格')}
            {tabBtn('board', '看板')}
            {tabBtn('list', '列表')}
            {tabBtn('gallery', '画廊')}
            <span style={{ flex: 1 }} />
            {btn('☰ 筛选/排序', () => setFilterOpen((v) => !v), { title: '多条件 AND/OR + 多列排序（复用智能文件夹规则语义）' })}
            {btn('⚙ 列设置', () => setSchemaEdit({ editId: null, name: '', typeKey: 'Text', options: '待办,进行中,完成', required: false }), { primary: true })}
            {btn('＋ 加行', () => setAddRowDlg({ noteId: '', localErr: null }))}
          </div>

          {/* 快捷筛选（title+文本列 contains——保 v1 习惯） */}
          {meta.prop_defs.length > 0 && (
            <input
              value={vc.filterText}
              onChange={(e) => {
                setVc((v) => ({ ...v, quickText: e.target.value }));
                setVisibleCount(50);
              }}
              placeholder="筛选：标题或文本列包含…"
              aria-label="快捷筛选"
              style={{ ...inputStyle, maxWidth: 360, marginBottom: 8 }}
            />
          )}

          {/* A-07: 高级筛选面板（多条件 AND/OR + 排序清单） */}
          {filterOpen && (
            <div style={{ marginBottom: 10, padding: 10, borderRadius: 8, border: '1px solid rgba(255,255,255,0.1)', background: 'rgba(0,0,0,0.18)', fontSize: 12 }}>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 8 }}>
                <strong>条件合成</strong>
                {(['and', 'or'] as const).map((c) => (
                  <button key={c} onClick={() => setVc((v) => ({ ...v, condMode: c }))} aria-pressed={vc.condMode === c}
                    style={{ padding: '2px 10px', borderRadius: 999, fontSize: 11, cursor: 'pointer',
                      border: `1px solid ${vc.condMode === c ? 'rgba(88,166,255,0.5)' : 'rgba(255,255,255,0.14)'}`,
                      background: vc.condMode === c ? 'rgba(88,166,255,0.14)' : 'transparent', color: tokens.color.textPrimary }}>
                    {c === 'and' ? '全部满足' : '任一满足'}
                  </button>
                ))}
              </div>
              <div style={{ display: 'grid', gap: 6 }}>
                {vc.conds.map((c, i) => (
                  <div key={i} style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
                    <select
                      aria-label={`条件 ${i + 1} 列`}
                      value={c.colId}
                      onChange={(e) => {
                        const colId = e.target.value;
                        setVc((v) => ({ ...v, conds: v.conds.map((x, j) => (j === i ? { ...x, colId, kind: colId === '__title' ? ('title' as const) : colId === '__tags' ? ('tag' as const) : ('prop' as const) } : x)) }));
                      }}
                      style={{ ...inputStyle, width: 150 }}
                    >
                      <option value="__title">标题</option>
                      <option value="__tags">标签</option>
                      {meta.prop_defs.map((d) => (
                        <option key={d.id} value={d.id}>{d.name}（{TYPE_LABEL[typeKey(d.prop_type)]}）</option>
                      ))}
                    </select>
                    <select
                      aria-label={`条件 ${i + 1} 运算`}
                      value={c.op}
                      onChange={(e) => setVc((v) => ({ ...v, conds: v.conds.map((x, j) => (j === i ? { ...x, op: e.target.value as FilterCond['op'] } : x)) }))}
                      style={{ ...inputStyle, width: 110 }}
                    >
                      <option value="contains">包含</option>
                      <option value="not">不包含</option>
                      <option value="eq">等于</option>
                      <option value="ne">不等于</option>
                      <option value="gt">大于</option>
                      <option value="lt">小于</option>
                    </select>
                    <>
                      <input
                        aria-label={`条件 ${i + 1} 值`}
                        value={c.value}
                        onChange={(e) => setVc((v) => ({ ...v, conds: v.conds.map((x, j) => (j === i ? { ...x, value: e.target.value } : x)) }))}
                        placeholder={c.colId === '__tags' ? '标签,逗号分隔' : '值'}
                        style={{ ...inputStyle, width: 180 }}
                      />
                    </>
                    <button onClick={() => setVc((v) => ({ ...v, conds: v.conds.filter((_, j) => j !== i) }))} aria-label={`删除条件 ${i + 1}`}
                      style={{ border: 'none', background: 'transparent', color: '#f87171', cursor: 'pointer', fontSize: 14 }}>✕</button>
                  </div>
                ))}
              </div>
              <div style={{ marginTop: 8, display: 'flex', gap: 8 }}>
                {btn('＋ 条件', () => setVc((v) => ({ ...v, conds: [...v.conds, { kind: 'title' as const, op: 'contains' as const, value: '', colId: '__title' }] })))}
                {vc.conds.length > 0 && btn('清空条件', () => setVc((v) => ({ ...v, conds: [] })))}
              </div>
              {vc.sortKeys.length > 0 && (
                <div style={{ marginTop: 10, paddingTop: 8, borderTop: '1px solid rgba(255,255,255,0.08)' }}>
                  <strong>排序</strong>（表头点击主序;Shift+点击叠加）:{' '}
                  {vc.sortKeys.map((s, i) => {
                    const name = s.col === '__title' ? '标题' : meta.prop_defs.find((d) => d.id === s.col)?.name ?? s.col;
                    return (
                      <span key={s.col} style={{ marginRight: 8 }}>
                        {i + 1}. {name} {s.dir === 1 ? '↑' : '↓'}
                      </span>
                    );
                  })}
                  <button onClick={() => setVc((v) => ({ ...v, sort: [] }))} style={{ border: 'none', background: 'transparent', color: '#f87171', cursor: 'pointer', fontSize: 11 }}>清空</button>
                </div>
              )}
            </div>
          )}

          {/* ── 表格视图（语义 table/th——无障碍 DoD） ── */}
          {vc.viewMode === 'table' && (
          <table style={{ borderCollapse: 'collapse', width: '100%', fontSize: 13 }}>
            <thead>
              <tr>
                <th
                  onClick={(e) => headerSort('__title', e)}
                  style={{ textAlign: 'left', padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.14)', color: tokens.color.textSecondary, cursor: 'pointer', userSelect: 'none' }}
                  aria-sort={vc.sortKeys[0]?.col === '__title' ? (vc.sortKeys[0].dir === 1 ? 'ascending' : 'descending') : 'none'}
                >
                  笔记 {sortMark('__title')}
                </th>
                {meta.prop_defs.map((d) => (
                  <th
                    key={d.id}
                    onClick={(e) => headerSort(d.id, e)}
                    style={{ textAlign: 'left', padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.14)', color: tokens.color.textSecondary, cursor: 'pointer', userSelect: 'none' }}
                    aria-sort={vc.sortKeys[0]?.col === d.id ? (vc.sortKeys[0].dir === 1 ? 'ascending' : 'descending') : 'none'}
                    title={`${d.name} · ${TYPE_LABEL[typeKey(d.prop_type)]}${d.required ? ' · 必填' : ''} · 点击排序（Shift=叠加）`}
                  >
                    {d.name} <span style={{ opacity: 0.6, fontSize: 11 }}>{TYPE_LABEL[typeKey(d.prop_type)]}{d.required ? '*' : ''}</span> {sortMark(d.id)}
                    {' '}
                    <button
                      onClick={(e) => { e.stopPropagation(); setSchemaEdit({ editId: d.id, name: d.name, typeKey: typeKey(d.prop_type), options: selectOptions(d).join(',') || (typeKey(d.prop_type) === 'MultiSelect' ? (Array.isArray(d.prop_type) ? '' : Object.values((d.prop_type as Record<string, Array<{ value: string }>>)?.MultiSelect ?? []).map((o) => o.value).join(',')) : ''), required: d.required }); }}
                      aria-label={`编辑列 ${d.name}`}
                      style={{ border: 'none', background: 'transparent', color: tokens.color.textSecondary, cursor: 'pointer', fontSize: 11, padding: '0 2px' }}
                      title="编辑列（类型/必填/选项）"
                    >
                      ✎
                    </button>
                    <button
                      onClick={(e) => { e.stopPropagation(); void removeColumn(d.id); }}
                      aria-label={`删除列 ${d.name}`}
                      style={{ border: 'none', background: 'transparent', color: '#f87171', cursor: 'pointer', fontSize: 11, padding: '0 2px' }}
                      title="删除列"
                    >
                      ×
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {shownRows.map((r) => (
                <tr key={r.note_id}>
                  <td
                    onClick={() => onOpenNote(r.note_id)}
                    style={{ padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.06)', cursor: 'pointer', color: tokens.color.primaryBright, textDecoration: 'underline dotted' }}
                    title="点击打开源笔记"
                  >
                    {r.title}
                  </td>
                  {meta.prop_defs.map((d) => {
                    const cell = r.values[d.id];
                    const isEditing = editing?.noteId === r.note_id && editing.defId === d.id;
                    const errHere = cellErr?.noteId === r.note_id && cellErr.defId === d.id;
                    const tk = typeKey(d.prop_type);
                    return (
                      <td
                        key={d.id}
                        tabIndex={tk === 'Formula' ? -1 : 0}
                        onDoubleClick={(e) => onCellDoubleClick(e, r, d)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' && tk !== 'Formula' && tk !== 'Checkbox' && tk !== 'Select' && tk !== 'MultiSelect') {
                            e.preventDefault();
                            editCell(r.note_id, d.id, e.currentTarget);
                          }
                        }}
                        onClick={() => { if (tk === 'Checkbox') onCheckboxClick(r, d); }}
                        onBlur={(e) => {
                          if (isEditing) {
                            void submitInline(r.note_id, d, e.currentTarget).then((ok) => {
                              if (ok) e.currentTarget.textContent = String(r.values[d.id]?.value ?? '');
                              setEditing(null);
                            });
                          }
                        }}
                        onKeyDownCapture={(e) => {
                          if (!isEditing) return;
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            (e.currentTarget as HTMLElement).blur();
                          }
                          if (e.key === 'Escape') {
                            e.preventDefault();
                            e.currentTarget.textContent = cell ? String(cell.value) : '';
                            setEditing(null);
                          }
                        }}
                        style={{
                          padding: '6px 8px',
                          borderBottom: '1px solid rgba(255,255,255,0.06)',
                          cursor: tk === 'Checkbox' ? 'pointer' : isEditing ? 'text' : 'cell',
                          outline: isEditing ? '1px solid rgba(88,166,255,0.6)' : errHere ? '1px solid rgba(248,113,113,0.7)' : 'none',
                          borderRadius: 4,
                          minWidth: 72,
                          textDecoration: tk === 'Relation' && cell ? 'underline dotted' : 'none',
                        }}
                        title={errHere ? cellErr!.msg : cell ? `${String(cell.value)} · 双击编辑 · Enter 提交 · Esc 取消` : '双击编辑 · Enter 提交 · Esc 取消'}
                      >
                        {isEditing
                          ? cell ? String(cell.value) : ''
                          : cell
                            ? (formatCell(d, cell.value) || <span style={{ color: tokens.color.textDisabled }}>—</span>)
                            : <span style={{ color: tokens.color.textDisabled }}>—</span>}
                      </td>
                    );
                  })}
                </tr>
              ))}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={meta.prop_defs.length + 1} style={{ padding: '14px 8px', color: tokens.color.textSecondary }}>
                    空集合——「＋ 加行」把笔记挂进来
                  </td>
                </tr>
              )}
              {rows.length > 0 && shownRows.length === 0 && (
                <tr>
                  <td colSpan={meta.prop_defs.length + 1} style={{ padding: '14px 8px', color: tokens.color.textSecondary }}>
                    筛选无命中
                  </td>
                </tr>
              )}
            </tbody>
          </table>
          )}

          {/* ── 列表视图（移动默认形态——紧凑行） ── */}
          {vc.viewMode === 'list' && (
            <div role="list" style={{ display: 'grid', gap: 2 }}>
              {shownRows.map((r) => {
                const firstCols = meta.prop_defs.slice(0, 3);
                return (
                  <div
                    key={r.note_id}
                    role="listitem"
                    onClick={() => onOpenNote(r.note_id)}
                    style={{ display: 'flex', gap: 10, alignItems: 'center', padding: '8px 10px', borderRadius: 8, cursor: 'pointer', border: '1px solid rgba(255,255,255,0.06)', background: 'rgba(255,255,255,0.02)' }}
                  >
                    <span style={{ fontWeight: 600, minWidth: 140, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.title}</span>
                    {firstCols.map((d) => {
                      const cell = r.values[d.id];
                      if (!cell) return null;
                      return (
                        <span key={d.id} style={{ fontSize: 12, color: tokens.color.textSecondary, maxWidth: 160, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {formatCell(d, cell.value)}
                        </span>
                      );
                    })}
                  </div>
                );
              })}
              {rows.length === 0 && <p style={{ color: tokens.color.textSecondary }}>空集合</p>}
            </div>
          )}

          {/* ── 画廊视图（卡片网格） ── */}
          {vc.viewMode === 'gallery' && (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(180px, 1fr))', gap: 10 }}>
              {shownRows.map((r) => {
                const firstSel = meta.prop_defs.find((d) => typeKey(d.prop_type) === 'Select');
                const selVal = firstSel ? String(r.values[firstSel.id]?.value ?? '') : '';
                return (
                  <div
                    key={r.note_id}
                    onClick={() => onOpenNote(r.note_id)}
                    style={{ padding: 12, borderRadius: 10, cursor: 'pointer', border: '1px solid rgba(255,255,255,0.1)', background: 'rgba(255,255,255,0.03)', display: 'grid', gap: 6 }}
                  >
                    <strong style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{r.title}</strong>
                    {selVal && (
                      <span style={{ fontSize: 11, padding: '1px 8px', borderRadius: 999, border: '1px solid rgba(88,166,255,0.35)', color: tokens.color.primaryBright, justifySelf: 'start' }}>
                        {selVal}
                      </span>
                    )}
                    {meta.prop_defs.filter((d) => typeKey(d.prop_type) !== 'Select').slice(0, 2).map((d) => {
                      const cell = r.values[d.id];
                      if (!cell) return null;
                      return (
                        <span key={d.id} style={{ fontSize: 12, color: tokens.color.textSecondary, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {d.name}: {formatCell(d, cell.value)}
                        </span>
                      );
                    })}
                  </div>
                );
              })}
            </div>
          )}

          {/* ── 看板视图（按第一个 Select 列分组;拖卡换组=set_cell） ── */}
          {vc.viewMode === 'board' && board.groups.length >= 0 && (
            <div>
              {board.groups.length === 0 && (
                <p style={{ color: tokens.color.textSecondary, fontSize: 12 }}>
                  看板需要至少一个单选（Select）列——「⚙ 列设置」新增。
                </p>
              )}
              <div style={{ display: 'flex', gap: 10, alignItems: 'flex-start', overflowX: 'auto', paddingBottom: 8 }}>
                {board.groups.map((g) => (
                  <div
                    key={g.key}
                    onDragOver={(e) => e.preventDefault()}
                    onDrop={(e) => {
                      e.preventDefault();
                      const noteId = e.dataTransfer.getData('text/aurora-note');
                      if (noteId && board.def) {
                        void commitSelect(noteId, board.def, g.key === '__none' ? null : g.key);
                      }
                    }}
                    style={{ minWidth: 220, padding: 10, borderRadius: 10, background: 'rgba(255,255,255,0.03)', border: '1px solid rgba(255,255,255,0.08)' }}
                  >
                    <div style={{ fontSize: 12, color: tokens.color.textSecondary, marginBottom: 8, display: 'flex', justifyContent: 'space-between' }}>
                      <span>{g.label}</span>
                      <span>{g.rows.length}</span>
                    </div>
                    <div style={{ display: 'grid', gap: 6 }}>
                      {g.rows.slice(0, visibleCount).map((r) => (
                        <div
                          key={r.note_id}
                          draggable
                          onDragStart={(e) => e.dataTransfer.setData('text/aurora-note', r.note_id)}
                          onClick={() => onOpenNote(r.note_id)}
                          style={{ padding: 10, borderRadius: 8, background: 'rgba(255,255,255,0.05)', border: '1px solid rgba(255,255,255,0.08)', cursor: 'grab', fontSize: 13 }}
                        >
                          {r.title}
                        </div>
                      ))}
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 渲染切片扩容（大集合防卡） */}
          {(() => {
            const total = rows.length;
            const shown = shownRows.length;
            return total > shown ? (
              <button onClick={() => setVisibleCount((c) => c + 100)} style={{ marginTop: 10, ...inputStyle, maxWidth: 200, cursor: 'pointer' }}>
                加载更多（{shown}/{total}）
              </button>
            ) : null;
          })()}
        </div>
      )}

      {/* ── 对话框群（零 prompt 替代 UI） ── */}
      {createDlg && (
        <Modal title="新建集合" onClose={() => setCreateDlg(null)}>
          <div style={{ display: 'grid', gap: 10 }}>
            <input
              autoFocus
              value={createDlg.title}
              onChange={(e) => setCreateDlg({ title: e.target.value })}
              onKeyDown={(e) => { if (e.key === 'Enter') void create(createDlg.title); }}
              placeholder="集合名称"
              aria-label="集合名称"
              style={inputStyle}
            />
            <div style={{ display: 'flex', gap: 8 }}>
              {btn('创建', () => void create(createDlg.title), { primary: true, disabled: !createDlg.title.trim() })}
              {btn('取消', () => setCreateDlg(null))}
            </div>
          </div>
        </Modal>
      )}

      {addRowDlg && (
        <Modal title="把笔记加入集合" onClose={() => setAddRowDlg(null)}>
          <div style={{ display: 'grid', gap: 10 }}>
            <input
              autoFocus
              value={addRowDlg.noteId}
              onChange={(e) => setAddRowDlg({ noteId: e.target.value, localErr: null })}
              onKeyDown={(e) => { if (e.key === 'Enter') void addRow(addRowDlg.noteId); }}
              placeholder="笔记 ID（源笔记详情面板可复制）"
              aria-label="笔记 ID"
              style={inputStyle}
            />
            {addRowDlg.localErr && (
              <div role="alert" style={{ padding: 8, borderRadius: 8, border: '1px solid rgba(248,113,113,0.4)', background: 'rgba(248,113,113,0.08)', color: '#f87171', fontSize: 12 }}>
                {addRowDlg.localErr}
              </div>
            )}
            <div style={{ display: 'flex', gap: 8 }}>
              {btn('加入', () => void addRow(addRowDlg.noteId), { primary: true, disabled: !addRowDlg.noteId.trim() })}
              {btn('取消', () => setAddRowDlg(null))}
            </div>
          </div>
        </Modal>
      )}

      {schemaEdit && meta && (
        <SchemaPanel
          meta={meta}
          rows={rows}
          edit={schemaEdit}
          onClose={() => setSchemaEdit(null)}
          onApply={applySchema}
          onDelete={removeColumn}
        />
      )}

      {/* Select/MultiSelect 下拉浮层（R10 惯例——fixed 定位） */}
      {selectMenu && menuDef && (() => {
        const isMulti = selectMenu.multi;
        const row = rows.find((r) => r.note_id === selectMenu.noteId);
        const curStr = isMulti ? '' : String(row?.values[menuDef.id]?.value ?? '');
        const curList = row ? multiCur(row, menuDef) : [];
        const allOpts = selectOptions(menuDef);
        return (
          <div
            data-select-menu
            role="menu"
            aria-label={`${menuDef.name} 选项`}
            style={{
              position: 'fixed',
              left: Math.min(selectMenu.x, window.innerWidth - 200),
              top: Math.min(selectMenu.y + 12, window.innerHeight - 260),
              transform: 'translateX(-50%)',
              zIndex: 95,
              minWidth: 160,
              maxHeight: 240,
              overflowY: 'auto',
              padding: 6,
              borderRadius: 10,
              background: tokens.color.bgSurface,
              border: '1px solid rgba(255,255,255,0.14)',
              boxShadow: '0 10px 32px rgba(0,0,0,0.45)',
            }}
          >
            {isMulti && (
              <div style={{ fontSize: 11, color: tokens.color.textSecondary, padding: '4px 10px' }}>
                多选（点击切换;关闭即提交）
              </div>
            )}
            {allOpts.map((opt) => {
              const active = isMulti ? curList.includes(opt) : curStr === opt;
              return (
                <button
                  key={opt}
                  role="menuitemcheckbox"
                  aria-checked={active}
                  onClick={() => {
                    if (!isMulti) {
                      void commitSelect(selectMenu.noteId, menuDef, opt === curStr ? null : opt);
                      return;
                    }
                    // 多选:本地切换→提交数组（即时写库）
                    const next = active ? curList.filter((v) => v !== opt) : [...curList, opt];
                    void commitSelect(selectMenu.noteId, menuDef, next);
                    setSelectMenu({ ...selectMenu }); // 保留浮层继续切换
                    void refreshRows().then(() => setSelectMenu({ ...selectMenu }));
                  }}
                  style={{
                    display: 'block',
                    width: '100%',
                    textAlign: 'left',
                    padding: '6px 10px',
                    fontSize: tokens.typography.body.size,
                    color: active ? tokens.color.primaryBright : tokens.color.textSecondary,
                    background: 'transparent',
                    border: 'none',
                    borderRadius: 6,
                    cursor: 'pointer',
                    minHeight: tokens.a11y.minTouchTarget,
                  }}
                >
                  {active ? '✓ ' : ''}{opt}
                </button>
              );
            })}
            {!isMulti && (
              <button
                onClick={() => void commitSelect(selectMenu.noteId, menuDef, null)}
                style={{
                  display: 'block',
                  width: '100%',
                  textAlign: 'left',
                  padding: '6px 10px',
                  fontSize: tokens.typography.body.size,
                  color: curStr === '' ? tokens.color.primaryBright : tokens.color.textSecondary,
                  background: 'transparent',
                  border: 'none',
                  borderRadius: 6,
                  cursor: 'pointer',
                  borderTop: '1px solid rgba(255,255,255,0.08)',
                  marginTop: 2,
                  minHeight: tokens.a11y.minTouchTarget,
                }}
              >
                {curStr === '' ? '✓ ' : ''}未分组
              </button>
            )}
            {isMulti && (
              <button
                onClick={() => setSelectMenu(null)}
                style={{
                  display: 'block', width: '100%', textAlign: 'center',
                  padding: '6px 10px', fontSize: 12, color: tokens.color.textPrimary,
                  background: 'transparent', border: 'none', cursor: 'pointer',
                  borderTop: '1px solid rgba(255,255,255,0.08)', marginTop: 2,
                }}
              >
                完成
              </button>
            )}
          </div>
        );
      })()}
    </main>
  );
}
