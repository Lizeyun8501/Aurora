/**
 * SmartFolderView — DK-02 S3 UI 面：智能文件夹求值展示 + 规则编辑。
 *
 * 数据源：cmd_smartfolder_list_items（拉模式求值——打开视图时计算）+
 * cmd_smartfolder_get_rule / cmd_smartfolder_set_rule（title_contains v1 条件）。
 * 交互：列表点击打开笔记；规则输入回车/按钮保存后即时刷新。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

interface Props {
  invoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null;
  folderId: string;
  folderTitle: string;
  onOpenNote: (noteId: string) => void;
}

interface SmartItem {
  note_id: string;
  title: string;
  updated_at: string;
}

export default function SmartFolderView({ invoke, folderId, folderTitle, onOpenNote }: Props) {
  const [items, setItems] = useState<SmartItem[]>([]);
  const [draft, setDraft] = useState('');
  // DK-27 v2: tags 条件草稿（逗号分隔多选/排除——终态语义与 v1 rule 字段向后兼容）
  const [tagsInc, setTagsInc] = useState('');
  const [tagsExc, setTagsExc] = useState('');
  const [err, setErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  const refresh = useCallback(async () => {
    if (!invoke) return;
    try {
      const li = (await invoke('cmd_smartfolder_list_items', {
        folder_id: folderId,
      })) as SmartItem[];
      setItems(li);
      setErr(null);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [invoke, folderId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // 初始规则回读（get_rule——无规则/读取失败回退空）
  useEffect(() => {
    if (!invoke) return;
    invoke('cmd_smartfolder_get_rule', { folder_id: folderId })
      .then((r) => {
        const rule = r as {
          title_contains?: string | null;
          tags_include?: string[];
          tags_exclude?: string[];
        };
        setDraft(rule.title_contains ?? '');
        setTagsInc((rule.tags_include ?? []).join(', '));
        setTagsExc((rule.tags_exclude ?? []).join(', '));
      })
      .catch(() => {
        setDraft('');
        setTagsInc('');
        setTagsExc('');
      });
  }, [invoke, folderId]);

  const saveRule = useCallback(async () => {
    if (!invoke) return;
    setSaving(true);
    try {
      const parse = (raw: string): string[] =>
        raw
          .split(/[,，]/)
          .map((t) => t.trim())
          .filter((t) => t !== '');
      await invoke('cmd_smartfolder_set_rule', {
        folder_id: folderId,
        title_contains: draft.trim() === '' ? null : draft.trim(),
        tags_include: parse(tagsInc),
        tags_exclude: parse(tagsExc),
      });
      setErr(null);
      await refresh();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }, [invoke, folderId, draft, tagsInc, tagsExc, refresh]);

  return (
    <main style={{ flex: 1, padding: tokens.spacing.lg, overflowY: 'auto' }}>
      <h1 style={{ margin: `0 0 ${tokens.spacing.xs}px`, fontSize: tokens.typography.title.size }}>
        🔮 {folderTitle}
      </h1>
      <div style={{ color: tokens.color.textSecondary, marginBottom: tokens.spacing.md }}>
        智能文件夹 · 按规则动态求值（拉模式） · 共 {items.length} 篇
      </div>

      {/* 规则编辑（v1：title_contains 子串） */}
      <div style={{ display: 'flex', gap: tokens.spacing.sm, marginBottom: tokens.spacing.md }}>
        <input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void saveRule();
          }}
          placeholder="标题包含…（留空 = 全部笔记）"
          aria-label="智能文件夹规则：标题包含"
          style={{
            flex: 1,
            background: tokens.color.bgSurface,
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
            color: tokens.color.textPrimary,
            fontSize: tokens.typography.body.size,
            outline: 'none',
          }}
        />
      </div>
      {/* DK-27 v2: tags 条件（逗号分隔多选/排除） */}
      <div style={{ display: 'flex', gap: tokens.spacing.sm, marginBottom: tokens.spacing.md }}>
        <input
          value={tagsInc}
          onChange={(e) => setTagsInc(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void saveRule();
          }}
          placeholder="包含标签…（逗号分隔，AND 语义；留空 = 不限）"
          aria-label="智能文件夹规则：包含标签"
          style={{
            flex: 1,
            background: tokens.color.bgSurface,
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
            color: tokens.color.textPrimary,
            fontSize: tokens.typography.body.size,
            outline: 'none',
          }}
        />
        <input
          value={tagsExc}
          onChange={(e) => setTagsExc(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void saveRule();
          }}
          placeholder="排除标签…（逗号分隔，任一含即排除）"
          aria-label="智能文件夹规则：排除标签"
          style={{
            flex: 1,
            background: tokens.color.bgSurface,
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
            color: tokens.color.textPrimary,
            fontSize: tokens.typography.body.size,
            outline: 'none',
          }}
        />
        <button
          onClick={() => void saveRule()}
          disabled={saving}
          style={{
            padding: `${tokens.spacing.sm}px ${tokens.spacing.lg}px`,
            background: tokens.color.primary,
            color: '#fff',
            border: 'none',
            borderRadius: tokens.radius.md,
            cursor: saving ? 'wait' : 'pointer',
            fontSize: tokens.typography.body.size,
          }}
        >
          {saving ? '保存中…' : '保存规则'}
        </button>
      </div>

      {err && (
        <div style={{ color: tokens.color.danger, marginBottom: tokens.spacing.md }}>⚠ {err}</div>
      )}

      {items.length === 0 ? (
        <div
          style={{
            padding: tokens.spacing.lg,
            color: tokens.color.textDisabled,
            textAlign: 'center',
          }}
        >
          无匹配笔记 — 调整规则或创建标题含关键字的笔记
        </div>
      ) : (
        <div>
          {items.map((it) => (
            <div
              key={it.note_id}
              onClick={() => onOpenNote(it.note_id)}
              style={{
                padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
                borderBottom: '1px solid rgba(255,255,255,0.06)',
                cursor: 'pointer',
              }}
            >
              <div style={{ fontWeight: 500, fontSize: tokens.typography.body.size }}>
                📄 {it.title || '未命名'}
              </div>
              <div
                style={{
                  fontSize: tokens.typography.caption.size,
                  color: tokens.color.textSecondary,
                }}
              >
                {it.updated_at || ''}
              </div>
            </div>
          ))}
        </div>
      )}
    </main>
  );
}
