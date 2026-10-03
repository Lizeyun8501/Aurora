/**
 * TodayDailyNote — DK-19 UI 面：今日笔记入口 + 三态模式切换 + 模板编辑。
 *
 * 数据源：cmd_daily_note_open（Auto 创建/Manual 定位/Off 拒绝）+
 * cmd_daily_note_get/set_mode（Auto 默认）+ get/set_template（{{date}}/{{weekday}} 渲染）。
 * 交互：打开按钮跳转笔记；模式就近切换（今日域功能就近原则）；模板折叠编辑。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';

interface Props {
  invoke: ((cmd: string, args?: Record<string, unknown>) => Promise<unknown>) | null;
  onOpenNote: (noteId: string) => void;
}

type DailyMode = 'auto' | 'manual' | 'off';

function todayIso(): string {
  const d = new Date();
  const mm = String(d.getMonth() + 1).padStart(2, '0');
  const dd = String(d.getDate()).padStart(2, '0');
  return `${d.getFullYear()}-${mm}-${dd}`;
}

export default function TodayDailyNote({ invoke, onOpenNote }: Props) {
  const [mode, setMode] = useState<DailyMode>('auto');
  const [openErr, setOpenErr] = useState<string | null>(null);
  const [tplOpen, setTplOpen] = useState(false);
  const [tpl, setTpl] = useState('');
  const [tplSaved, setTplSaved] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    if (!invoke) return;
    invoke('cmd_daily_note_get_mode')
      .then((m) => setMode((m as string) as DailyMode))
      .catch(() => {});
    invoke('cmd_daily_note_get_template')
      .then((t) => setTpl((t as string) ?? ''))
      .catch(() => {});
  }, [invoke]);

  const openToday = useCallback(async () => {
    if (!invoke) return;
    setOpenErr(null);
    try {
      const r = (await invoke('cmd_daily_note_open', { date: todayIso() })) as {
        note_id: string | null;
        created: boolean;
      };
      if (r.note_id) {
        onOpenNote(r.note_id);
      } else {
        setOpenErr('每日笔记已关闭（Off 模式）——切换为 Auto 或 Manual 后可用');
      }
    } catch (e) {
      setOpenErr(e instanceof Error ? e.message : String(e));
    }
  }, [invoke, onOpenNote]);

  const changeMode = useCallback(
    async (m: DailyMode) => {
      if (!invoke) return;
      try {
        await invoke('cmd_daily_note_set_mode', { mode: m });
        setMode(m);
      } catch (e) {
        setOpenErr(e instanceof Error ? e.message : String(e));
      }
    },
    [invoke],
  );

  const saveTpl = useCallback(async () => {
    if (!invoke) return;
    try {
      await invoke('cmd_daily_note_set_template', { template: tpl });
      setTplSaved(true);
      setTimeout(() => setTplSaved(false), 1500);
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    }
  }, [invoke, tpl]);

  return (
    <div
      style={{
        background: tokens.color.bgSurface,
        border: '1px solid rgba(255,255,255,0.08)',
        borderRadius: tokens.radius.lg,
        padding: tokens.spacing.md,
        display: 'flex',
        flexDirection: 'column',
        gap: tokens.spacing.sm,
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: tokens.spacing.sm }}>
        <button
          onClick={() => void openToday()}
          style={{
            background: tokens.color.primary,
            color: '#fff',
            border: 'none',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.sm}px ${tokens.spacing.md}px`,
            fontSize: tokens.typography.body.size,
            cursor: 'pointer',
          }}
        >
          📝 打开今日笔记
        </button>
        <select
          value={mode}
          onChange={(e) => void changeMode(e.target.value as DailyMode)}
          aria-label="每日笔记模式"
          style={{
            background: tokens.color.bgElevated,
            color: tokens.color.textPrimary,
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
            fontSize: tokens.typography.caption.size,
          }}
        >
          <option value="auto">Auto · 自动创建</option>
          <option value="manual">Manual · 仅定位</option>
          <option value="off">Off · 关闭</option>
        </select>
        <button
          onClick={() => setTplOpen((v) => !v)}
          style={{
            background: 'transparent',
            color: tokens.color.textSecondary,
            border: '1px solid rgba(255,255,255,0.12)',
            borderRadius: tokens.radius.md,
            padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
            fontSize: tokens.typography.caption.size,
            cursor: 'pointer',
          }}
        >
          {tplOpen ? '收起模板' : '模板'}
        </button>
      </div>
      {openErr && (
        <div style={{ fontSize: tokens.typography.caption.size, color: tokens.color.warning }}>
          {openErr}
        </div>
      )}
      {tplOpen && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: tokens.spacing.xs }}>
          <textarea
            value={tpl}
            onChange={(e) => setTpl(e.target.value)}
            rows={3}
            placeholder={'今日待办：\n- {{date}}（{{weekday}}）'}
            style={{
              background: tokens.color.bgElevated,
              color: tokens.color.textPrimary,
              border: '1px solid rgba(255,255,255,0.12)',
              borderRadius: tokens.radius.md,
              padding: tokens.spacing.sm,
              fontSize: tokens.typography.caption.size,
              fontFamily: 'inherit',
              resize: 'vertical',
            }}
          />
          <button
            onClick={() => void saveTpl()}
            style={{
              alignSelf: 'flex-start',
              background: 'transparent',
              color: tokens.color.primaryBright,
              border: '1px solid rgba(55,220,242,0.4)',
              borderRadius: tokens.radius.md,
              padding: `${tokens.spacing.xs}px ${tokens.spacing.sm}px`,
              fontSize: tokens.typography.caption.size,
              cursor: 'pointer',
            }}
          >
            {tplSaved ? '已保存 ✓' : '保存模板（{{date}} / {{weekday}} 可用）'}
          </button>
          {err && (
            <div style={{ fontSize: tokens.typography.caption.size, color: tokens.color.danger }}>
              {err}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
