/**
 * R17：FSRS 统计面板 — ReviewQueue.review_stats 聚合面（任务书 T4）。
 *
 * 链路：cmd_review_stats（today/again_rate/due_7d 全量口径）→
 * 四指标卡（总卡数/今日到期/今日复习/Again 率）+ due_7d 迷你条形
 * （纯 CSS div 高度映射，index 0=今日）。
 *
 * 诚实边界：invoke 失败静默 0 化（非阻断）；today 口径 Utc 日界
 * （与 core 聚合同偏差）；again_rate 今日零复习显「—」。
 */
import { useEffect, useState } from 'react';
import tokens from '../design/tokens';
import type { InvokeFn } from './DesktopShell';

interface ReviewStatsDto {
  total_cards: number;
  due_now: number;
  today_reviews: number;
  today_again_rate: number | null;
  due_7d: number[];
}

interface ReviewDayStatDto {
  date: string;
  reviews: number;
  again_rate: number | null;
}

const HOT = '#c9a9ff';
// R18：again_rate 超过该阈值的柱体染警示色（克制小面积标记）。
const WARN = '#F58323';
const WARN_RATE = 0.3;

function StatCard({
  label,
  value,
  hot,
}: {
  label: string;
  value: string;
  hot?: boolean;
}): React.ReactElement {
  return (
    <div
      style={{
        flex: 1,
        background: tokens.color.bgSurface,
        border: `1px solid ${hot ? 'rgba(201,169,255,0.35)' : 'rgba(255,255,255,0.08)'}`,
        borderRadius: tokens.radius.md,
        padding: tokens.spacing.sm,
        textAlign: 'center',
      }}
    >
      <div style={{ fontSize: 20, fontWeight: 700, color: hot ? HOT : tokens.color.textPrimary }}>
        {value}
      </div>
      <div style={{ fontSize: tokens.typography.caption.size, color: tokens.color.textSecondary }}>
        {label}
      </div>
    </div>
  );
}

function ReviewStatsPanel({ invoke }: { invoke: InvokeFn | null }): React.ReactElement {
  // α3 挂载接线：InfoPanelTabs 的 invoke 为可空——组件内 !invoke 短路已兜。
  const [stats, setStats] = useState<ReviewStatsDto | null>(null);
  // R18：留存率曲线（最近 7 天按日序列；null=未加载）。
  const [series, setSeries] = useState<ReviewDayStatDto[] | null>(null);
  useEffect(() => {
    if (!invoke) return; // invoke 可空（InfoPanelTabs 直传）——短路
    let alive = true;
    invoke('cmd_review_stats')
      .then((r) => {
        if (alive) setStats(r as ReviewStatsDto);
      })
      .catch(() => {
        if (alive) {
          setStats({
            total_cards: 0,
            due_now: 0,
            today_reviews: 0,
            today_again_rate: null,
            due_7d: [0, 0, 0, 0, 0, 0, 0],
          });
        }
      });
    return () => {
      alive = false;
    };
  }, [invoke]);
  // R17-α3 挂载接线（Alpha）：统计重拉——与 ReviewEntryCard 同款
  // aurora:review-dirty 事件桥（评分/建卡后统计与曲线均不 stale）。
  // R18：曲线段（最近 7 天按日评分量+Again 率）同桥并入。
  useEffect(() => {
    if (!invoke) return;
    const loadAll = (): void => {
      invoke('cmd_review_stats')
        .then((r) => setStats(r as ReviewStatsDto))
        .catch(() => undefined); // 静默——面板非关键路径
      invoke('cmd_review_daily_series', { days: 7 })
        .then((r) => setSeries(r as ReviewDayStatDto[]))
        .catch(() => undefined);
    };
    loadAll();
    window.addEventListener('aurora:review-dirty', loadAll);
    return () => window.removeEventListener('aurora:review-dirty', loadAll);
  }, [invoke]);
  const s = stats;
  const again = s?.today_again_rate;
  const maxDue = s ? Math.max(1, ...s.due_7d) : 1;
  // R18：曲线段——最近 7 天评分量柱状（柱高=复习量，柱体紫；again 率
  // 超 WARN_RATE 染警示）。maxRev 归一，0 复习日显 2% 底线（同 due_7d 模式）。
  const maxRev = series && series.length > 0 ? Math.max(1, ...series.map((d) => d.reviews)) : 1;
  const dayLabel = (date: string): string => {
    // "2026-10-26" → "10-26"（省年份；失败回退原串）
    const m = /^\d{4}-(\d{2}-\d{2})$/.exec(date);
    return m ? m[1] : date;
  };
  return (
    <div style={{ marginTop: tokens.spacing.md }}>
      <div style={{ display: 'flex', gap: tokens.spacing.sm }}>
        <StatCard label="总卡数" value={s ? String(s.total_cards) : '…'} />
        <StatCard label="今日到期" value={s ? String(s.due_now) : '…'} hot={(s?.due_now ?? 0) > 0} />
        <StatCard label="今日复习" value={s ? String(s.today_reviews) : '…'} />
        <StatCard label="Again 率" value={again == null ? '—' : `${Math.round(again * 100)}%`} />
      </div>
      <div
        style={{
          display: 'flex',
          alignItems: 'stretch',
          gap: 4,
          height: 56,
          marginTop: tokens.spacing.sm,
          padding: tokens.spacing.sm,
          background: tokens.color.bgSurface,
          border: '1px solid rgba(255,255,255,0.08)',
          borderRadius: tokens.radius.md,
        }}
      >
        {['今', '明', '+2', '+3', '+4', '+5', '+6'].map((d, i) => {
          const v = s?.due_7d[i] ?? 0;
          return (
            <div
              key={d}
              style={{
                flex: 1,
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: 2,
                justifyContent: 'flex-end',
              }}
            >
              <div
                title={`+${i} 天 ${v} 张`}
                style={{
                  width: '70%',
                  height: `${Math.max(v > 0 ? 8 : 2, (v / maxDue) * 100)}%`,
                  background: i === 0 ? HOT : 'rgba(201,169,255,0.35)',
                  borderRadius: 2,
                }}
              />
              <div style={{ fontSize: 10, color: tokens.color.textSecondary }}>{d}</div>
            </div>
          );
        })}
      </div>
      {/* R18：留存率曲线段——最近 7 天按日评分量（again 率>30% 染警示） */}
      <div
        style={{
          display: 'flex',
          alignItems: 'stretch',
          gap: 4,
          height: 56,
          marginTop: tokens.spacing.sm,
          padding: tokens.spacing.sm,
          background: tokens.color.bgSurface,
          border: '1px solid rgba(255,255,255,0.08)',
          borderRadius: tokens.radius.md,
        }}
      >
        {(series ?? Array.from({ length: 7 }, () => null)).map((d, i) => {
          const v = d?.reviews ?? 0;
          const warn = (d?.again_rate ?? 0) > WARN_RATE;
          return (
            <div
              key={d?.date ?? `slot-${i}`}
              style={{
                flex: 1,
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                gap: 2,
                justifyContent: 'flex-end',
              }}
            >
              <div
                title={
                  d
                    ? `${d.date}：${d.reviews} 次${d.again_rate == null ? '' : `，Again ${Math.round(d.again_rate * 100)}%`}`
                    : '加载中'
                }
                style={{
                  width: '70%',
                  height: `${Math.max(v > 0 ? 8 : 2, (v / maxRev) * 100)}%`,
                  background: v === 0 ? 'rgba(201,169,255,0.18)' : warn ? WARN : HOT,
                  borderRadius: 2,
                }}
              />
              <div style={{ fontSize: 10, color: tokens.color.textSecondary }}>
                {d ? dayLabel(d.date) : '·'}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default ReviewStatsPanel;
