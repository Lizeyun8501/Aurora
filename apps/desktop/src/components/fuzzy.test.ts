/** DK-3.2 切片一：fuzzyMatch 单测——子序列/连续/词首/归一化四维。
 * 语义：**字符级**子序列（中文标题=字匹配——拼音化 v2 单列）。 */
import { describe, expect, it } from 'vitest';
import { fuzzyMatch, fuzzySort } from './fuzzy';

describe('DK-3.2：fuzzyMatch', () => {
  it('子序列命中（nb → node build）', () => {
    expect(fuzzyMatch('nb', 'node build')).not.toBeNull();
  });

  it('中文标题：字匹配（笔记 → 新建笔记）', () => {
    const m = fuzzyMatch('笔记', '新建笔记');
    expect(m).not.toBeNull();
    expect(m!.indices).toEqual([2, 3]);
  });

  it('非子序列拒绝（xz → 新建笔记）', () => {
    expect(fuzzyMatch('xz', '新建笔记')).toBeNull();
  });

  it('连续命中 > 离散命中', () => {
    const a = fuzzyMatch('jk', 'javascript kit');
    const b = fuzzyMatch('jk', 'javascript koala');
    expect(a && b).toBeTruthy();
    expect(a!.score).toBeGreaterThan(b!.score);
  });

  it('词首/串首命中（js → javascript）', () => {
    const m = fuzzyMatch('js', 'javascript');
    expect(m).not.toBeNull();
    expect(m!.indices).toEqual([0, 4]);
  });

  it('空 pattern 恒通过（score 0）', () => {
    expect(fuzzyMatch('', '任意')?.score).toBe(0);
  });

  it('fuzzySort：降序且过滤不命中', () => {
    const items = [
      { title: '从剪贴板捕获笔记' },
      { title: '新建笔记' },
      { title: 'AI 液化审查' },
    ];
    const out = fuzzySort(items, '笔记', (i) => i.title);
    expect(out.length).toBe(2);
    expect(out[0].title).toBe('新建笔记'); // 短标题归一化高分在前
  });
});

// ── C-06: recency 加权语义（palette 置顶——不挤掉精确匹配） ──
describe('C-06: recency 加权', () => {
  it('精确匹配分数 > 模糊匹配 + 0.5 recency 加成（置顶不挤精确）', () => {
    const exact = fuzzyMatch('新建笔记', '新建笔记')!;
    const vague = fuzzyMatch('建', '新建笔记')!;
    // 精确完整命中 vs 单字模糊: 分差远超 0.5 加成上限
    expect(exact.score).toBeGreaterThan(vague.score + 0.5);
  });

  it('同分层: recency 加成 tie-break 生效（0.5 可翻转同分项）', () => {
    const a = fuzzyMatch('任务', '任务归档')!;
    const b = fuzzyMatch('任务', '任务清理')!;
    // 同型命中分近——boost 0.5 使带 recency 项胜出
    const boosted = a.score + 0.5;
    expect(boosted).toBeGreaterThan(b.score);
  });
});
