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
  prop_type: string;
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
  if (t && typeof t === 'object' && 'select' in (t as Record<string, unknown>)) return 'Select';
  if (t && typeof t === 'object' && 'multiSelect' in (t as Record<string, unknown>)) return 'MultiSelect';
  if (t && typeof t === 'object' && 'relation' in (t as Record<string, unknown>)) return 'Relation';
  if (t && typeof t === 'object' && 'formula' in (t as Record<string, unknown>)) return 'Formula';
  return 'Text';
}

export default function CollectionView({ invoke, onOpenNote }: { invoke: InvokeFn | null; onOpenNote: (id: string) => void }): React.ReactElement {
  const [cols, setCols] = useState<CollectionMeta[]>([]);
  const [selected, setSelected] = useState<string | null>(null);
  const [meta, setMeta] = useState<CollectionMeta | null>(null);
  const [rows, setRows] = useState<Row[]>([]);
  const [err, setErr] = useState<string | null>(null);

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

  const editCell = useCallback(
    async (noteId: string, def: PropDef, current: unknown) => {
      if (!invoke) return;
      const input = window.prompt(`编辑 ${def.name}（${TYPE_LABEL[typeKey(def.prop_type)] ?? ''}）：`, current == null ? '' : String(current));
      if (input === null) return;
      // v1 类型粗判：数字/布尔自动转，其余字符串
      let value: unknown = input;
      if (typeKey(def.prop_type) === 'Number') value = Number(input);
      if (typeKey(def.prop_type) === 'Checkbox') value = input === 'true' || input === '是';
      try {
        await invoke('cmd_collection_set_cell', { noteId, defId: def.id, value });
        setRows((await invoke('cmd_collection_rows', { id: selected })) as Row[]);
        setErr(null);
      } catch (e) {
        setErr(e instanceof Error ? e.message : String(e));
      }
    },
    [invoke, selected],
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
            <span style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
              {rows.length} 行 · {meta.prop_defs.length} 列（schema 编辑 v2）
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
          <table style={{ borderCollapse: 'collapse', width: '100%', fontSize: 13 }}>
            <thead>
              <tr>
                <th style={{ textAlign: 'left', padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.14)', color: tokens.color.textSecondary }}>笔记</th>
                {meta.prop_defs.map((d) => (
                  <th key={d.id} style={{ textAlign: 'left', padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.14)', color: tokens.color.textSecondary }}>
                    {d.name}
                    <span style={{ fontSize: 10, marginLeft: 4, opacity: 0.7 }}>[{TYPE_LABEL[typeKey(d.prop_type)] ?? '?'}]</span>
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
                    return (
                      <td
                        key={d.id}
                        onClick={() => void editCell(r.note_id, d, cell?.value)}
                        style={{ padding: '6px 8px', borderBottom: '1px solid rgba(255,255,255,0.06)', cursor: 'text' }}
                        title="点击编辑"
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
        </>
      )}
      {!meta && cols.length === 0 && !err && (
        <p style={{ color: tokens.color.textSecondary }}>暂无集合——「＋ 新建集合」创建第一个</p>
      )}
    </main>
  );
}
