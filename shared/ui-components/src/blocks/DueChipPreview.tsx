/**
 * C-02: NL 日期 chip 群预览组件 — @aurora/ui-components 共享层（双端真单源）。
 *
 * 解析权威=Rust core `due_parse_multi`（桌面 cmd_preview_due_multi /
 * 移动 AndroidBridge.previewDueMulti 桥——同一 core,前端零复刻）。
 * 本组件=纯展示+交互原语（无平台依赖,无 invoke——数据由端侧传入）。
 *
 * 设计:
 * - chips: 解析结果群（语义=开始/截止/未分类——core 词面判定不猜测）
 * - onRemove(i): 一键移除误识别（父层记 removed 索引集——不再写回 due）
 * - onCorrect(i): 点 chip 主体开日期选择器校正（父层供 DatePicker）
 * - 无解析=零渲染（DoD: 无占位空态）
 * - 无障碍: aria-label 含完整日期+语义;Delete/Backspace 键盘删除
 *   （chip 群 roving focus,Esc 释放焦点回输入框）
 */
import { useRef, useState, type KeyboardEvent } from 'react';

export interface DueChip {
  due_ts: number;
  start: number;
  end: number;
  matched_text: string;
  human: string;
  semantic: string; // '开始' | '截止' | '未分类'
}

export interface DueChipPreviewProps {
  chips: DueChip[];
  /** 已移除索引集（父层持久——移除后不写回 due 字段） */
  removed: ReadonlySet<number>;
  onRemove: (i: number) => void;
  onCorrect: (i: number) => void;
}

const CHIP_BG: Record<string, string> = {
  开始: 'rgba(86,182,94,0.14)',
  截止: 'rgba(88,166,255,0.14)',
  未分类: 'rgba(150,150,160,0.12)',
};
const CHIP_FG: Record<string, string> = {
  开始: '#6fcf7a',
  截止: '#8ab8ff',
  未分类: '#a8a8b0',
};

export default function DueChipPreview({ chips, removed, onRemove, onCorrect }: DueChipPreviewProps) {
  const [focusIdx, setFocusIdx] = useState<number | null>(null);
  const boxRef = useRef<HTMLDivElement>(null);
  if (chips.length === 0) return null; // DoD: 无解析零渲染（无空态文案）

  const onKeyDown = (e: KeyboardEvent, i: number) => {
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      onRemove(i);
      // 焦点顺移到下一存活 chip（无则释放）
      const next = chips.findIndex((_, j) => j > i && !removed.has(j));
      setFocusIdx(next >= 0 ? next : null);
    } else if (e.key === 'Escape') {
      setFocusIdx(null);
      (boxRef.current?.parentElement?.querySelector('textarea, input') as HTMLElement | null)?.focus();
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      const dir = e.key === 'ArrowRight' ? 1 : -1;
      for (let j = i + dir; j >= 0 && j < chips.length; j += dir) {
        if (!removed.has(j)) { setFocusIdx(j); break; }
      }
    }
  };

  return (
    <div
      ref={boxRef}
      role="group"
      aria-label="识别到的时间"
      data-testid="due-chip-group"
      style={{ display: 'flex', flexWrap: 'wrap', gap: 6, marginTop: 6 }}
    >
      {chips.map((c, i) =>
        removed.has(i) ? null : (
          <span
            key={i}
            role="button"
            tabIndex={focusIdx === null ? i === chips.findIndex((_, j) => !removed.has(j)) ? 0 : -1 : focusIdx === i ? 0 : -1}
            aria-label={`识别时间：${c.human}，${c.semantic}，原文「${c.matched_text}」。按 Delete 移除。`}
            data-testid={`due-chip-${i}`}
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 5,
              padding: '3px 8px 3px 10px',
              borderRadius: 6,
              background: CHIP_BG[c.semantic] ?? CHIP_BG['未分类'],
              border: `1px solid ${CHIP_FG[c.semantic] ?? CHIP_FG['未分类']}55`,
              color: CHIP_FG[c.semantic] ?? CHIP_FG['未分类'],
              fontSize: 12,
              cursor: 'pointer',
            }}
            onClick={() => onCorrect(i)}
            onKeyDown={(e) => onKeyDown(e, i)}
            onFocus={() => setFocusIdx(i)}
          >
            <span aria-hidden>📅</span>
            <b style={{ fontWeight: 600 }}>{c.human}</b>
            <span style={{ opacity: 0.75 }}>· {c.semantic}</span>
            <button
              aria-label={`移除 ${c.human}`}
              data-testid={`due-chip-remove-${i}`}
              style={{
                border: 'none',
                background: 'transparent',
                color: 'inherit',
                cursor: 'pointer',
                padding: '0 2px',
                fontSize: 13,
                lineHeight: 1,
              }}
              onClick={(e) => { e.stopPropagation(); onRemove(i); }}
            >
              ×
            </button>
          </span>
        ),
      )}
    </div>
  );
}
