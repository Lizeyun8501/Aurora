/** DK-2.7 切片一：tokenizeCode 纯函数单测——CI 面兜底（vitest）。 */
import { describe, it, expect } from 'vitest';
import { tokenizeCode } from '../editors/codeHighlight';

describe('tokenizeCode（正则级 tokenizer）', () => {
  it('rust：注释/字符串/数字/关键字四类识别', () => {
    const src = '// 注释\nlet x = 42;\nfn main() { println("hi"); }';
    const tokens = tokenizeCode('rust', src);
    const kinds = (k: string) => tokens.filter((t) => t.kind === k);
    expect(kinds('comment')).toHaveLength(1);
    expect(kinds('keyword').map((t) => src.slice(t.start, t.end))).toEqual(
      expect.arrayContaining(['let', 'fn']),
    );
    expect(kinds('number')).toHaveLength(1);
    expect(kinds('string')).toHaveLength(1);
    // 区间不越界
    for (const t of tokens) {
      expect(t.start).toBeLessThanOrEqual(t.end);
      expect(t.end).toBeLessThanOrEqual(src.length);
    }
  });

  it('ts：字符串转义吞噬+关键字大小写敏感', () => {
    const src = 'const s = "a\\"b"; Return;';
    const tokens = tokenizeCode('ts', src);
    const str = tokens.find((t) => t.kind === 'string');
    expect(str && src.slice(str.start, str.end)).toBe('"a\\"b"');
    // Return 非关键字（大小写敏感——return 才是）
    const kws = tokens.filter((t) => t.kind === 'keyword').map((t) => src.slice(t.start, t.end));
    expect(kws).toEqual(['const']);
  });

  it('未识别语言=纯文本零装饰+越界安全', () => {
    expect(tokenizeCode('plaintext', 'let x = 1')).toHaveLength(0);
    expect(tokenizeCode('rust', '')).toHaveLength(0);
    // 未闭合字符串=吞到文末（不 panic）
    const unclosed = tokenizeCode('ts', 'const s = "未闭合');
    expect(unclosed.some((t) => t.kind === 'string')).toBe(true);
  });
});
