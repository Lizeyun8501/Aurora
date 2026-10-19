/**
 * DK-3.2 切片一：命令面板模糊匹配 — fzf 风格 subsequence 评分。
 *
 * 规则（纯函数，O(n·m) 上界、短串可忽略）：
 * - 全部字符按序命中（子序列）才候选——否则 null
 * - 连续命中段加分（连贯性）
 * - 词首/串首命中加分（边界敏感性）
 * - 分母归一化：得分/目标长度（短标题同命中更高分）
 */
export interface FuzzyResult {
  score: number;
  /** 命中下标序列（高亮用——v1 未消费保留） */
  indices: number[];
}

const SCORE_BASE = 8;
const SCORE_CONSECUTIVE = 5;
const SCORE_WORD_START = 6;
const SCORE_STRING_START = 10;

export function fuzzyMatch(pattern: string, target: string): FuzzyResult | null {
  if (!pattern) return { score: 0, indices: [] };
  const p = pattern.toLowerCase();
  const t = target.toLowerCase();
  const indices: number[] = [];
  let ti = 0;
  let score = 0;
  let prevHit = -2;
  for (let pi = 0; pi < p.length; pi++) {
    const ch = p[pi];
    if (ch === ' ') {
      // 空格=词边界——重置连续但不要求命中
      prevHit = -2;
      continue;
    }
    let found = -1;
    while (ti < t.length) {
      if (t[ti] === ch) { found = ti; ti++; break; }
      ti++;
    }
    if (found < 0) return null;
    score += SCORE_BASE;
    if (found === prevHit + 1) score += SCORE_CONSECUTIVE;
    const wordStart = found === 0 || /[\s\-_/（(【[]/.test(t[found - 1]);
    if (wordStart) score += SCORE_WORD_START;
    if (found === 0) score += SCORE_STRING_START;
    prevHit = found;
    indices.push(found);
  }
  // 归一化：目标越短同命中越高分
  return { score: score / (t.length + 1), indices };
}

/** 列表按 pattern 模糊过滤+降序（无 pattern 原样返回） */
export function fuzzySort<T>(
  items: T[],
  pattern: string,
  textOf: (item: T) => string,
): T[] {
  if (!pattern.trim()) return items;
  return items
    .map((item) => ({ item, m: fuzzyMatch(pattern.trim(), textOf(item)) }))
    .filter((x): x is { item: T; m: FuzzyResult } => x.m !== null)
    .sort((a, b) => b.m.score - a.m.score)
    .map((x) => x.item);
}
