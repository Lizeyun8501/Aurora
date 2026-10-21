/**
 * G1-S3：CollectionView — 数据库视图（表格 v1）。
 *
 * 数据：cmd_collection_list（集合列表）→ 选中 → cmd_collection_rows
 * （行+属性值+title）+ cmd_collection_get（schema 列头）。
 * 交互 v1：建集合 / 列头类型徽章只读展示 / 单元格点击 prompt 编辑
 * （类型校验在 core PropertyEngine——非法值拒绝回显）/ 加行 prompt
 * 笔记 ID / 行点击跳源笔记。
 *
 * v1 诚实边界：列头增删改（schema 编辑）无 UI——cmd_collection_set_schema
 * 命令面已就绪等 v2 表格化编辑器；内联单元格编辑/下拉选择器 v2；
 * 视图形态（看板/列表/筛选排序）v2。
 */
import React, { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

type InvokeFn = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

interface PropDef {
  id: string;
  name: string;
  // R-β2：serde 外部标签——单元变体为字符串，Select/MultiSelect 为对象
  prop_type: unknown;
  required: boolean;
}
interface CollectionMeta {
  id: string;
  title: string;
  prop_defs: PropDef[];
}
interface Row {
  note_id: string;
  title: string;
  values: Record<string, { value: unknown }>;
}

const TYPE_LABEL: Record<string, string> = {
  Text: '文本',
  Number: '数字',
  Date: '日期',
  Checkbox: '勾选',
  Select: '单选',
  MultiSelect: '多选',
  Relation: '关联',
  Formula: '公式',
};

function typeKey(t: unknown): string {
  if (typeof t === 'string') return t;
  // R-β2（self-caught）：serde 外部标签变体键为大写原名（{"Select":[...]}）——
  // 旧版 'select' in t 小写检查是死代码
  if (t && typeof t === 'object') {
    const o = t as Record<string, unknown>;
    if ('Select' in o) return 'Select';
    if ('MultiSelect' in o) return 'MultiSelect';
    if ('Relation' in o) return 'Relation';
    if ('Formula' in o) return 'Formula';
  }
  return 'Text';
}

export default function CollectionView({ invoke, onOpenNote }: { invoke: InvokeFn | null; onOpenNote: (id: string) => void }): React.ReactElement {
  const [cols, setCols] = useState<CollectionMeta[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [meta, setMeta] = useState<CollectionMeta | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [err, setErr] = useState<string | null>(null);
  // G1-v3：内联单元格编辑位 + 视图形态（表格 | 看板）
  const [editing, setEditing] = useState<{ noteId: string; defId: string } | null>(null);
  const [viewMode, setViewMode] = useState<'table' | 'board'>('table');

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

  useEffect(() => {
    if (!invoke || !selected) return;
    void (async () => {
      try {
        setMeta((await invoke('cmd_collection_get', { id: selected })) as CollectionMeta);
        setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
        setErr(null);
      } catch {
        setErr('集合数据加载失败');
      }
    })();
  }, [invoke, selected]);

  const create = useCallback(async () => {
    if (!invoke) return;
    const title = window.prompt('集合名称：');
    if (!title?.trim()) return;
    try {
      const col = (await invoke('cmd_collection_create', { title: title.trim() })) as CollectionMeta;
      setCols((prev) => [...prev, col]);
      setSelected(col.id);
    } catch {
      setErr('建集合失败');
    }
  }, [invoke]);

  const addRow = useCallback(async () => {
    if (!invoke || !selected) return;
    const noteId = window.prompt('加入的笔记 ID：');
    if (!noteId?.trim()) return;
    try {
      await invoke('cmd_collection_add_row', { id: selected, noteId: noteId.trim() });
      setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
    } catch {
      setErr('加行失败（笔记不存在?）');
    }
  }, [invoke, selected]);

  /* R5-1：schema 编辑——加列（名称+类型 prompt 两段）+删列（列头 ×）。
     set_schema=全量覆盖语义（对齐 cmd_set_note_tags 惯例）。 */
  const addColumn = useCallback(async () => {
    if (!invoke || !meta) return;
    const name = window.prompt('列名：');
    if (!name?.trim()) return;
    const types = ['文本', '数字', '日期', '勾选', '单选', '多选'];
    const ti = window.prompt(`类型（1-6）：\n${types.map((t, i) => `${i + 1}. ${t}`).join('\n')}`);
    if (ti == null) return;
    const idx = Number(ti) - 1;
    // R-β1（Bravo 复核）：不得用 !idx——会误杀合法 idx=0（文本列）
    if (Number.isNaN(idx) || idx < 0 || idx >= 6) return;
    const typeNames = ['Text', 'Number', 'Date', 'Checkbox', 'Select', 'MultiSelect'];
    // R-β2：serde 外部标签——单元变体传字符串，参数变体传 {Select:[{value,color}]}
    let prop_type: unknown = typeNames[idx];
    if (idx >= 4) {
      const raw = window.prompt('选项（逗号分隔，如：待办,进行中,完成）：', '待办,进行中,完成');
      if (raw == null) return;
      const opts = raw
        .split(/[,，]/)
        .map((s) => s.trim())
        .filter(Boolean)
        .map((v) => ({ value: v, color: null }));
      if (opts.length === 0) return;
      prop_type = idx === 4 ? { Select: opts } : { MultiSelect: opts };
    }
    const def = {
      id: `pd:${crypto.randomUUID?.() ?? Date.now()}`,
      name: name.trim(),
      prop_type,
      required: false,
    };
    const next = [...meta.prop_defs, def];
    try {
      await invoke('cmd_collection_set_schema', { id: meta.id, propDefs: next });
      setMeta({ ...meta, prop_defs: next });
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [invoke, meta]);

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

  /* G1-v3 内联单元格：双击进入 contentEditable，blur/Enter 提交，
     Esc 还原取消——contentEditable 期间 React 不 patch 用户脏改，
     还原/失败路径均手动回写 textContent 防 DOM 残留。 */
  const submitInline = useCallback(
    async (noteId: string, d: PropDef, el: HTMLElement): Promise<boolean> => {
      if (!invoke) return false;
      const input = el.textContent?.trim() ?? '';
      let value: unknown = input;
      if (typeKey(d.prop_type) === 'Number') value = input === '' ? null : Number(input);
      if (typeKey(d.prop_type) === 'Checkbox') value = input === 'true' || input === '是';
      if (typeKey(d.prop_type) === 'MultiSelect') {
        value = input === '' ? [] : input.split(/[,，]/).map((s) => s.trim()).filter(Boolean);
      }
      try {
        await invoke('cmd_collection_set_cell', { noteId, defId: d.id, value });
        setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
        setErr(null);
        return true;
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
        return false;
      }
    },
    [invoke, selected],
  );

  const selectOptions = useCallback((d: PropDef): string[] => {
    if (d.prop_type && typeof d.prop_type === 'object' && 'Select' in (d.prop_type as Record<string, unknown>)) {
      const opts = (d.prop_type as { Select: Array<{ value: string }> }).Select ?? [];
      return opts.map((o) => o.value);
    }
    return [];
  }, []);

  /* G1-v3 看板视图：按第一个单选列分组；拖卡换组=set_cell（Select 值）。
     「未分组」列对应 null 值（PropertyEngine 对 Select null 放行）。 */
  const [boardDrag, setBoardDrag] = useState<string | null>(null);
  const [boardOver, setBoardOver] = useState<string | null>(null);

  const dropBoard = useCallback(
    async (groupKey: string) => {
      setBoardOver(null);
      const id = boardDrag;
      setBoardDrag(null);
      if (!invoke || !id || !meta) return;
      const selDef = meta.prop_defs.find((d) => typeKey(d.prop_type) === 'Select');
      if (!selDef) return;
      const value = groupKey === '__none' ? null : groupKey;
      try {
        await invoke('cmd_collection_set_cell', { noteId: id, defId: selDef.id, value });
        setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
        setErr(null);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [boardDrag, invoke, meta, selected],
  );

  return (
    <main style={{ flex: 1, padding: `${tokens.spacing.lg}px ${tokens.spacing.xl}px`, overflowY: 'auto', minWidth: 0 }}>
      <h1 style={{ margin: `0 0 ${tokens.spacing.md}px`, fontSize: tokens.typography.title.size, display: 'flex', alignItems: 'center', gap: tokens.spacing.sm }}>
        数据库
        <button
          onClick={() => void create()}
          style={{
            fontSize: 12,
            padding: '3px 12px',
            borderRadius: 999,
            border: '1px solid rgba(88,166,255,0.4)',
            background: 'rgba(88,166,255,0.12)',
            color: tokens.color.primaryBright,
            cursor: 'pointer',
          }}
        >
          ＋ 新建集合
        </button>
      </h1>
      {cols.length > 0 && (
        <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: tokens.spacing.md }}>
          {cols.map((c) => (
            <button
              key={c.id}
              onClick={() => setSelected(c.id)}
              aria-pressed={selected === c.id}
              style={{
                fontSize: 12,
                padding: '2px 12px',
                borderRadius: 999,
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
      {err && <p style={{ color: '#f88', fontSize: 13 }}>{err}</p>}
      {!invoke && <p style={{ color: tokens.color.textSecondary }}>browser-mock 模式——数据库命令仅 tauri 模式可用</p>}
      {meta && (
        <>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
            <span style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
              {rows.length} 行 · {meta.prop_defs.length} 列
              <button
                onClick={() => void addColumn()}
                style={{
                  fontSize: 11,
                  padding: '1px 10px',
                  borderRadius: 999,
                  border: '1px solid rgba(163,113,247,0.4)',
                  background: 'rgba(163,113,247,0.14)',
                  color: '#c9a9ff',
                  cursor: 'pointer',
                }}
                title="加列（名称+类型）"
              >
                ＋ 加列
              </button>
              <span style={{ display: 'inline-flex', borderRadius: 999, border: '1px solid rgba(255,255,255,0.14)', overflow: 'hidden' }}>
                {(['table', 'board'] as const).map((m) => (
                  <button
                    key={m}
                    onClick={() => setViewMode(m)}
                    style={{
                      fontSize: 11,
                      padding: '1px 10px',
                      border: 'none',
                      background: viewMode === m ? 'rgba(88,166,255,0.2)' : 'transparent',
                      color: viewMode === m ? tokens.color.primaryBright : tokens.color.textSecondary,
                      cursor: 'pointer',
                    }}
                  >
                    {m === 'table' ? '表格' : '看板'}
                  </button>
                ))}
              </span>
            </span>
            <button
              onClick={() => void addRow()}
              style={{
                fontSize: 12,
                padding: '2px 10px',
                borderRadius: 6,
                border: '1px solid rgba(255,255,255,0.14)',
                background: 'transparent',
                color: tokens.color.textSecondary,
                cursor: 'pointer',
              }}
            >
              ＋ 加行
            </button>
          </div>
          {viewMode === 'table' && (
          <table style={{ borderCollapse: 'collapse', width: '100%', fontSize: 13 }}>
            <thead>
              <tr>
                <th style={{ textAlign: 'left', padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.14)', color: tokens.color.textSecondary }}>笔记</th>
                {meta.prop_defs.map((d) => (
                  <th key={d.id} style={{ textAlign: 'left', padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.14)', color: tokens.color.textSecondary }}>
                    {d.name}
                    <span style={{ fontSize: 10, marginLeft: 4, opacity: 0.7 }}>[{TYPE_LABEL[typeKey(d.prop_type)] ?? '?'}]</span>
                    <button
                      onClick={() => void removeColumn(d.id)}
                      aria-label={`删除列 ${d.name}`}
                      style={{
                        marginLeft: 4,
                        fontSize: 10,
                        border: 'none',
                        background: 'transparent',
                        color: tokens.color.textDisabled,
                        cursor: 'pointer',
                        padding: '0 2px',
                      }}
                      title="删除该列（schema 全量覆盖）"
                    >
                      ×
                    </button>
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {rows.map((r) => (
                <tr key={r.note_id}>
                  <td
                    onClick={() => onOpenNote(r.note_id)}
                    style={{ padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.06)', cursor: 'pointer', color: tokens.color.primaryBright }}
                    title="打开源笔记"
                  >
                    {r.title}
                  </td>
                  {meta.prop_defs.map((d) => {
                    const cell = r.values[d.id];
                    const isEditing = editing?.noteId === r.note_id && editing.defId === d.id;
                    const restoreText = cell ? String(cell.value) : '';
                    return (
                      <td
                        key={d.id}
                        contentEditable={isEditing}
                        suppressContentEditableWarning
                        onDoubleClick={(e) => {
                          if (!invoke) return;
                          setEditing({ noteId: r.note_id, defId: d.id });
                          setTimeout(() => (e.currentTarget as HTMLElement).focus(), 0);
                        }}
                        onBlur={(e) => {
                          if (!isEditing) return;
                          const el = e.currentTarget as HTMLElement;
                          setEditing(null);
                          void submitInline(r.note_id, d, el).then((ok) => {
                            if (!ok) el.textContent = restoreText; // 失败还原防 DOM 残留
                          });
                        }}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            (e.currentTarget as HTMLElement).blur();
                          }
                          if (e.key === 'Escape') {
                            e.preventDefault();
                            e.currentTarget.textContent = restoreText;
                            setEditing(null);
                          }
                        }}
                        style={{
                          padding: '6px 8px',
                          borderBottom: '1px solid rgba(255,255,255,0.06)',
                          cursor: isEditing ? 'text' : 'cell',
                          outline: isEditing ? '1px solid rgba(88,166,255,0.6)' : 'none',
                          borderRadius: 4,
                          minWidth: 72,
                        }}
                        title="双击编辑 · Enter 提交 · Esc 取消"
                      >
                        {cell ? String(cell.value) : <span style={{ color: tokens.color.textDisabled }}>—</span>}
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
            </tbody>
          </table>
          )}
          {viewMode === 'board' && (() => {
            const selDef = meta.prop_defs.find((d) => typeKey(d.prop_type) === 'Select');
            if (!selDef) {
              return (
                <p style={{ color: tokens.color.textSecondary }}>
                  看板视图需要一个「单选」列——「＋ 加列」选类型 5（单选）后按选项分组
                </p>
              );
            }
            const groups: Array<{ key: string; label: string }> = [
              ...selectOptions(selDef).map((v) => ({ key: v, label: v })),
              { key: '__none', label: '未分组' },
            ];
            return (
              <div style={{ display: 'flex', gap: tokens.spacing.sm, alignItems: 'flex-start', overflowX: 'auto', paddingBottom: 8 }}>
                {groups.map((g) => {
                  const cards = rows.filter((r) => {
                    const v = r.values[selDef.id]?.value;
                    return g.key === '__none' ? v == null || v === '' : v === g.key;
                  });
                  return (
                    <div
                      key={g.key}
                      onDragOver={(e) => {
                        e.preventDefault();
                        setBoardOver(g.key);
                      }}
                      onDrop={() => void dropBoard(g.key)}
                      style={{
                        flex: '0 0 200px',
                        background: boardOver === g.key ? 'rgba(88,166,255,0.08)' : tokens.color.bgSurface,
                        border: `1px solid ${boardOver === g.key ? 'rgba(88,166,255,0.45)' : 'rgba(255,255,255,0.07)'}`,
                        borderRadius: tokens.radius.md,
                        padding: tokens.spacing.sm,
                        minHeight: 160,
                      }}
                    >
                      <p style={{ margin: `0 0 ${tokens.spacing.xs}px`, fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
                        {g.label} · {cards.length}
                      </p>
                      {cards.map((c) => (
                        <div
                          key={c.note_id}
                          draggable
                          onDragStart={() => setBoardDrag(c.note_id)}
                          onDragEnd={() => setBoardDrag(null)}
                          onClick={() => onOpenNote(c.note_id)}
                          style={{
                            padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
                            marginBottom: 6,
                            background: 'rgba(255,255,255,0.05)',
                            border: '1px solid rgba(255,255,255,0.09)',
                            borderRadius: tokens.radius.sm,
                            fontSize: 13,
                            cursor: 'grab',
                            overflow: 'hidden',
                            textOverflow: 'ellipsis',
                            whiteSpace: 'nowrap',
                          }}
                          title="拖到其他分组换状态 · 点击打开笔记"
                        >
                          {c.title}
                        </div>
                      ))}
                    </div>
                  );
                })}
              </div>
            );
          })()}
        </>
      )}
      {!meta && cols.length === 0 && !err && (
        <p style={{ color: tokens.color.textSecondary }}>暂无集合——「＋ 新建集合」创建第一个</p>
      )}
    </main>
  );
}
