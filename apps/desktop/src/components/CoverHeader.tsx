/**
 * R4-1：Cover+图标 — 页面门面（Notion 式 cover 带 + emoji 图标）。
 *
 * v1 生成式零持久化：note_id hash → 稳定双色渐变 cover + 固定池 emoji——
 * 同一笔记每次渲染一致（无 schema 变更/无持久化——字段落库 v2）。
 * 图标点击轮换（会话级）。诚实边界：刷新即回默认轮换位——v2 落
 * note.icon/cover 字段后显式记忆。
 */
import { useState } from 'react';
import tokens from '../design/tokens';

/** 固定 emoji 池（知识/协作向 12 枚——hash 稳定选位） */
const ICONS = ['📝', '📚', '🧠', '🔬', '🚀', '🌱', '⚙️', '🧭', '💡', '🗂️', '🔭', '🧩'] as const;

/** 渐变对池（暗色系 8 组——与主题 tokens 协调） */
const GRADS: Array<[string, string]> = [
  ['#1e3a5f', '#2d5a88'],
  ['#3b2f63', '#5c4a8f'],
  ['#1f4a3d', '#2f6b57'],
  ['#4a2f2f', '#6b4a3a'],
  ['#2f3b4a', '#4a5c6e'],
  ['#4a3b1f', '#6e5a2f'],
  ['#3d1f4a', '#5c2f6e'],
  ['#1f4a4a', '#2f6b6b'],
];

/** FNV-1a 哈希（稳定——同 id 同结果） */
function hash32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export default function CoverHeader({ noteId }: { noteId: string }): React.ReactElement {
  const [spin, setSpin] = useState(0);
  const h = hash32(noteId) + spin * 7919;
  const grad = GRADS[h % GRADS.length];
  const icon = ICONS[Math.floor(h / GRADS.length) % ICONS.length];
  return (
    <div data-testid="cover-header" style={{ marginBottom: tokens.spacing.sm }}>
      <div
        style={{
          height: 72,
          borderRadius: tokens.radius.md,
          background: `linear-gradient(135deg, ${grad[0]}, ${grad[1]})`,
          position: 'relative',
        }}
      >
        <button
          onClick={() => setSpin((v) => v + 1)}
          aria-label="轮换图标"
          title="点击轮换图标（会话级 v1——持久化 v2）"
          style={{
            position: 'absolute',
            left: 14,
            bottom: -14,
            width: 44,
            height: 44,
            borderRadius: tokens.radius.md,
            background: tokens.color.bgElevated,
            border: '1px solid rgba(255,255,255,0.12)',
            fontSize: 22,
            cursor: 'pointer',
          }}
        >
          {icon}
        </button>
      </div>
      <div style={{ height: 20 }} />
    </div>
  );
}
