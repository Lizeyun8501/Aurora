#!/usr/bin/env node
/**
 * O-03: 事件字典三方对拍——Rust `CoreEvent::DICT` ↔ TS `EVENT_DICT`。
 * 单一来源 = Rust event.rs 的 DICT；本脚本断言两侧**逐名称**一致
 * （含 channel 与 persistent 语义映射），任何一侧漂移即 exit 1（CI 阻断）。
 * 用法: node scripts/check-event-dict.mjs [--json]
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const rustSrc = readFileSync(join(root, 'crates/aurora-core/src/event_bus/event.rs'), 'utf8');
const tsSrc = readFileSync(join(root, 'shared/types/src/events.ts'), 'utf8');

// ── Rust DICT 解析: ("Name", EventChannel::X, bool), ──
const rustDict = [];
{
  const m = rustSrc.match(/pub const DICT:[^=]*=\s*&\[(.*?)\];/s);
  if (!m) fail('event.rs: 未找到 pub const DICT 定义');
  for (const line of m[1].split('\n')) {
    const t = line.match(/\("([A-Za-z0-9]+)",\s*EventChannel::(Low|Medium|High),\s*(true|false)\)/);
    if (t) rustDict.push({ name: t[1], channel: t[2].toLowerCase(), persistent: t[3] === 'true' });
  }
}

// ── TS EVENT_DICT 解析: { type: 'Name', channel: 'x', persistent: bool }, ──
const tsDict = [];
{
  const m = tsSrc.match(/export const EVENT_DICT:[^=]*=\s*\[(.*?)\];/s);
  if (!m) fail('events.ts: 未找到 export const EVENT_DICT 定义');
  for (const line of m[1].split('\n')) {
    const t = line.match(/type:\s*'([A-Za-z0-9]+)',\s*channel:\s*'(low|medium|high)',\s*persistent:\s*(true|false)/);
    if (t) tsDict.push({ name: t[1], channel: t[2], persistent: t[3] === 'true' });
  }
}

// ── 对拍 ──
const rustNames = rustDict.map((d) => d.name);
const tsNames = tsDict.map((d) => d.name);
const onlyRust = rustNames.filter((n) => !tsNames.includes(n));
const onlyTs = tsNames.filter((n) => !rustNames.includes(n));
const semanticDrift = rustDict
  .filter((r) => tsDict.some((t) => t.name === r.name))
  .filter((r) => {
    const t = tsDict.find((x) => x.name === r.name);
    return t.channel !== r.channel || t.persistent !== r.persistent;
  })
  .map((r) => `${r.name}: rust(${r.channel},${r.persistent}) vs ts(${tsDict.find((x) => x.name === r.name).channel},${tsDict.find((x) => x.name === r.name).persistent})`);

const json = process.argv.includes('--json');
const ok = onlyRust.length === 0 && onlyTs.length === 0 && semanticDrift.length === 0;

if (json) {
  console.log(JSON.stringify({ ok, rust: rustDict.length, ts: tsDict.length, onlyRust, onlyTs, semanticDrift }, null, 2));
} else {
  console.log(`Rust DICT: ${rustDict.length} 条 · TS EVENT_DICT: ${tsDict.length} 条`);
  if (onlyRust.length) console.log('❌ 仅 Rust 侧有: ' + onlyRust.join(', '));
  if (onlyTs.length) console.log('❌ 仅 TS 侧有: ' + onlyTs.join(', '));
  if (semanticDrift.length) console.log('❌ 语义漂移:\n  ' + semanticDrift.join('\n  '));
  console.log(ok ? '✅ 事件字典三方对拍一致' : '（对拍失败——以 crates/aurora-core/src/event_bus/event.rs DICT 为准修正 events.ts）');
}
process.exit(ok ? 0 : 1);

function fail(msg) {
  console.error('❌ ' + msg);
  process.exit(1);
}
