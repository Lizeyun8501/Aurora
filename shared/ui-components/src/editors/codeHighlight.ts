/**
 * DK-2.7 切片一：code_block 语法着色——正则级 tokenizer + 纯函数装饰面。
 *
 * 设计裁定：
 * - **自实现正则级**（不引 highlight.js/prism——依赖纪律：~1MB 换覆盖
 *   不成比例；v1 覆盖 注释/字符串/数字/关键字 四类——诚实边界：非完整
 *   语法树，嵌套/转义边角容忍误判——着色是呈现增强非语义承诺）；
 * - 语言集：rust/ts|js/python/json/常见回退（未识别语言=纯文本零装饰）；
 * - 输出 token 区间（[start, end, kind]）——消费方（ProseMirror 插件）
 *   换算 doc 绝对偏移建 Decoration；**本函数零 DOM 依赖**（可独立单测）。
 */

export type TokenKind = 'comment' | 'string' | 'number' | 'keyword';
export type Token = { start: number; end: number; kind: TokenKind };

const KEYWORDS: Record<string, string[]> = {
  rust: [
    'fn', 'let', 'mut', 'const', 'struct', 'enum', 'impl', 'trait', 'pub',
    'use', 'mod', 'match', 'if', 'else', 'for', 'while', 'loop', 'return',
    'async', 'await', 'self', 'Self', 'true', 'false', 'unsafe', 'where',
  ],
  ts: [
    'const', 'let', 'var', 'function', 'return', 'if', 'else', 'for',
    'while', 'class', 'extends', 'interface', 'type', 'import', 'export',
    'from', 'async', 'await', 'new', 'this', 'true', 'false', 'null',
    'undefined', 'switch', 'case', 'default', 'try', 'catch', 'throw',
  ],
  python: [
    'def', 'class', 'return', 'if', 'elif', 'else', 'for', 'while', 'import',
    'from', 'as', 'with', 'try', 'except', 'raise', 'lambda', 'pass', 'in',
    'not', 'and', 'or', 'True', 'False', 'None', 'self',
  ],
  json: ['true', 'false', 'null'],
};

const ALIASES: Record<string, string> = {
  javascript: 'ts', jsx: 'ts', typescript: 'ts', tsx: 'ts',
  py: 'python', rs: 'rust',
};

/** 语言归一（未知语言 → null——零装饰纯文本）。 */
export function normLang(lang: string): string | null {
  const l = lang.toLowerCase();
  if (l in KEYWORDS) return l;
  if (l in ALIASES) return ALIASES[l];
  return null;
}

/** 正则级 tokenize：单次扫描——注释/字符串优先吞噬，数字/关键字回退。 */
export function tokenizeCode(lang: string, text: string): Token[] {
  const norm = normLang(lang);
  if (!norm) return [];
  const kws = new Set(KEYWORDS[norm]);
  // 行注释：python=#，rust/ts/js/json=//（块注释 /* */ 非 python）
  const lineCmt = norm === 'python' ? '#' : '//';
  const out: Token[] = [];
  let i = 0;
  const push = (start: number, end: number, kind: TokenKind): void => {
    if (end > start) out.push({ start, end, kind });
  };
  while (i < text.length) {
    const rest = text.slice(i);
    // 行注释（// 或 #）
    if (rest.startsWith(lineCmt)) {
      const nl = text.indexOf('\n', i);
      const end = nl === -1 ? text.length : nl;
      push(i, end, 'comment');
      i = end;
      continue;
    }
    // 块注释（/* */——非 python）
    if (norm !== 'python' && rest.startsWith('/*')) {
      const end = text.indexOf('*/', i + 2);
      const stop = end === -1 ? text.length : end + 2;
      push(i, stop, 'comment');
      i = stop;
      continue;
    }
    // 字符串（' " `——含转义吞噬）
    const ch = text[i];
    if (ch === '"' || ch === "'" || ch === '`') {
      let j = i + 1;
      while (j < text.length) {
        if (text[j] === '\\') { j += 2; continue; }
        if (text[j] === ch) { j += 1; break; }
        j += 1;
      }
      push(i, Math.min(j, text.length), 'string');
      i = Math.min(j, text.length);
      continue;
    }
    // 数字
    if (/[0-9]/.test(ch) && !/[A-Za-z_]/.test(text[i - 1] ?? '')) {
      let j = i;
      while (j < text.length && /[0-9a-fA-FxXoObB._]/.test(text[j])) j += 1;
      push(i, j, 'number');
      i = j;
      continue;
    }
    // 标识符 → 关键字判定
    if (/[A-Za-z_]/.test(ch)) {
      let j = i;
      while (j < text.length && /[A-Za-z0-9_]/.test(text[j])) j += 1;
      const word = text.slice(i, j);
      if (kws.has(word)) push(i, j, 'keyword');
      i = j;
      continue;
    }
    i += 1;
  }
  return out;
}
