/**
 * G3：FSRS 闪卡复习浮层 — palette「开始复习」入口挂载。
 *
 * 链路：cmd_due_review_cards（到期卡=R 阈值/due≤now）→ 卡片逐张渲染
 * （笔记级卡：title+首行摘要）→ 四键评分 Again/Hard/Good/Easy
 * → cmd_review_card（core FsrsScheduler 状态推进）→ 下一张。
 *
 * v1 边界（诚实边界）：笔记级整卡（块级卡待 Distill 管线 v2）；
 * 建卡首评固定 Good（评分 UX 在复习面板）；评分后本地移除该卡
 * （下次到期由 due 驱动回队——与移动端同源语义）。
 */
import { useCallback, useEffect, useState } from 'react';
import tokens from '../design/tokens';
import type { InvokeFn } from './DesktopShell';

export interface ReviewCard {
  card_id: string;
  note_id: string;
  due_at: string;
  retrievability: number;
  reps: number;
  lapses: number;
  /** cmd_add_review_card 返回面（due 队列不含——建卡反馈用） */
  title?: string;
  summary?: string;
}

interface Props {
  invoke: InvokeFn;
  onClose: () => void;
}

const RATINGS: Array<{ label: string; value: number; color: string }> = [
  { label: '忘记', value: 1, color: '#e5484d' },
  { label: '困难', value: 2, color: '#d29922' },
  { label: '良好', value: 3, color: '#3fb950' },
  { label: '轻松', value: 4, color: '#58a6ff' },
];

export default function ReviewPanel({ invoke, onClose }: Props) {
  const [cards, setCards] = useState<ReviewCard[] | null>(null);
  const [idx, setIdx] = useState(0);
  const [done, setDone] = useState(0);
  const [err, setErr] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const out = (await invoke('cmd_due_review_cards')) as ReviewCard[];
      setCards(out ?? []);
    } catch {
      setCards([]); // mock/降级空态
    }
  }, [invoke]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const grade = useCallback(
    async (rating: number) => {
      if (!cards || idx >= cards.length) return;
      const card = cards[idx];
      try {
        await invoke('cmd_review_card', { cardId: card.card_id, rating });
        setDone((d) => d + 1);
        setIdx((i) => i + 1);
      } catch (e) {
        setErr(String(e));
      }
    },
    [cards, idx, invoke],
  );

  const card = cards && idx < cards.length ? cards[idx] : null;
  const loading = cards === null;

  return (
    <div
      role="dialog"
      aria-modal
      aria-label="FSRS 复习"
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(0,0,0,0.45)',
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'center',
        zIndex: 2100,
        paddingTop: 120,
      }}
    >
      <div
        onClick={(e) => e.stopPropagation()}
        style={{
          width: 480,
          background: tokens.color.bgElevated,
          border: '1px solid rgba(255,255,255,0.10)',
          borderRadius: tokens.radius.md,
          padding: tokens.spacing.md,
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <strong style={{ fontSize: tokens.typography.title.size }}>📇 闪卡复习</strong>
          <span style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
            {loading ? '加载中…' : `第 ${idx + 1} / ${cards!.length} 张 · 已评 ${done}`}
          </span>
        </div>

        {loading && (
          <p style={{ color: tokens.color.textSecondary }}>队列加载中…</p>
        )}

        {!loading && !card && (
          <div style={{ marginTop: tokens.spacing.md }}>
            <p style={{ color: tokens.color.textSecondary }}>
              {done > 0 ? `本轮 ${done} 张已评完——下次到期见 🎉` : '今日无到期卡片'}
            </p>
            <div style={{ textAlign: 'right', marginTop: tokens.spacing.sm }}>
              <button
                onClick={onClose}
                style={{
                  background: tokens.color.bgSurface,
                  border: '1px solid rgba(255,255,255,0.10)',
                  borderRadius: tokens.radius.sm,
                  color: tokens.color.textPrimary,
                  padding: '4px 14px',
                  cursor: 'pointer',
                }}
              >
                关闭
              </button>
            </div>
          </div>
        )}

        {card && (
          <div style={{ marginTop: tokens.spacing.md }}>
            <div
              style={{
                background: tokens.color.bgSurface,
                border: '1px solid rgba(255,255,255,0.08)',
                borderRadius: tokens.radius.md,
                padding: tokens.spacing.md,
                minHeight: 96,
              }}
            >
              {/* R11/R14：生成卡辨识——card_id 尾 ':b'（块）/':l'（列表）为
                  Distill 生成卡；title「笔记名 · heading/术语」拆显：笔记名弱化 */}
              {(() => {
                const isBlock = /:b[0-9a-f]{8}$/.test(card.card_id ?? '');
                const isList = /:l[0-9a-f]{8}$/.test(card.card_id ?? '');
                const t = card.title ?? '未命名卡';
                const sep = t.indexOf(' · ');
                return (
                  <div style={{ fontWeight: 600, marginBottom: 6 }}>
                    {isBlock && (
                      <span style={{ color: '#c9a9ff', fontSize: tokens.typography.caption.size, marginRight: 6 }}>
                        ▸ 块
                      </span>
                    )}
                    {isList && (
                      <span style={{ color: '#8fd3c7', fontSize: tokens.typography.caption.size, marginRight: 6 }}>
                        ▸ 项
                      </span>
                    )}
                    {(isBlock || isList) && sep > 0 ? (
                      <>
                        <span style={{ color: tokens.color.textSecondary, fontWeight: 400 }}>{t.slice(0, sep)} · </span>
                        {t.slice(sep + 3)}
                      </>
                    ) : (
                      t
                    )}
                  </div>
                );
              })()}
              <div style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size }}>
                {card.summary || (/:b[0-9a-f]{8}$|:l[0-9a-f]{8}$/.test(card.card_id ?? '') ? '（无正文——概念卡）' : '（无摘要——笔记首行即卡面，v1 笔记级整卡）')}
              </div>
              <div style={{ color: tokens.color.textSecondary, fontSize: tokens.typography.caption.size, marginTop: 8 }}>
                可提取性 {(card.retrievability * 100).toFixed(0)}% · 复习 {card.reps} 次
                {card.lapses > 0 ? ` · 遗忘 ${card.lapses} 次` : ''}
              </div>
            </div>
            {err && (
              <div role="alert" style={{ color: '#f88', fontSize: 13, marginTop: 6 }}>
                {err}
              </div>
            )}
            <div style={{ display: 'flex', gap: tokens.spacing.sm, marginTop: tokens.spacing.md }}>
              {RATINGS.map((r) => (
                <button
                  key={r.value}
                  onClick={() => void grade(r.value)}
                  style={{
                    flex: 1,
                    background: `${r.color}22`,
                    border: `1px solid ${r.color}55`,
                    borderRadius: tokens.radius.sm,
                    color: r.color,
                    padding: '8px 0',
                    cursor: 'pointer',
                    fontWeight: 600,
                  }}
                  title={`FSRS Rating ${r.value}`}
                >
                  {r.label}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
