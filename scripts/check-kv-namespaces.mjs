#!/usr/bin/env node
/**
 * O-02/ADR-003: KV 命名空间静态门禁——扫全仓 `kv_store.set(` 直写调用点,
 * 与 crates/aurora-core/src/kv_namespace_registry.rs 的 REGISTRY 对账:
 *   1. 前缀未登记 → 阻断（exit 1）
 *   2. 命中 A 类前缀且调用方不在 EXEMPT_CALLERS（write_path/框架层）→ 阻断
 *   3. 命中含债登记（debt: Some）的 B 类 → 提示 OPEN 债存量（不阻断,债修复后应归零）
 * 字面量前缀识别: kv_store.set("note:...") / format!("dbview:{id}") 两种形态。
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, dirname, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const registrySrc = readFileSync(join(root, 'crates/aurora-core/src/kv_namespace_registry.rs'), 'utf8');

// ── 解析 REGISTRY（与 Rust 单测同源对账的 JS 侧镜像）──
const classMap = { A: 'A', B: 'B' };
const entries = [];
for (const m of registrySrc.matchAll(/Namespace \{ prefix: "([a-z_]+)", class: KvClass::(A|B), purpose: "[^"]*", user_content: (true|false), sync: (true|false), sealed: (true|false), debt: (None|Some)/g)) {
  entries.push({ prefix: m[1], class: classMap[m[2]], userContent: m[3] === 'true', debt: m[6] === 'Some' });
}
const exempt = [];
const exemptBlock = registrySrc.match(/EXEMPT_CALLERS[^=]*=\s*&\[(.*?)\];/s);
for (const m of exemptBlock[1].matchAll(/"([^"]+)"/g)) exempt.push(m[1]);
if (entries.length === 0) fail('REGISTRY 解析失败（格式漂移——同步本脚本的正则）');

// ── 扫描 Rust 源码的 kv_store.set 调用点 ──
const SCAN_ROOTS = ['crates', 'apps'];
const findings = [];
const KEY_LITERAL = /kv_store\s*\.\s*set\s*\(\s*&?\s*format!\s*\(\s*"([a-z_]+)[:{]|kv_store\s*\.\s*set\s*\(\s*&?\s*"([a-z_]+)[:{]/g;

for (const scanRoot of SCAN_ROOTS) {
  walk(join(root, scanRoot));
}
function walk(dir) {
  for (const name of readdirSync(dir)) {
    if (name === 'target' || name === 'node_modules' || name.startsWith('.')) continue;
    const full = join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) { walk(full); continue; }
    if (!name.endsWith('.rs')) continue;
    const rel = relative(root, full).replaceAll('\\', '/');
    // 测试文件白名单（测试夹具可造任意前缀——数据非产品直写面）
    if (rel.includes('/tests/') || rel.endsWith('_test.rs')) continue;
    let src = readFileSync(full, 'utf8');
    // 剥离内联测试 mod（#[cfg(test)] mod tests { ... } ——粗粒度括号配平,夹具非产品直写面）
    for (;;) {
      const at = src.indexOf('#[cfg(test)]');
      if (at < 0) break;
      const brace = src.indexOf('{', at);
      if (brace < 0) break;
      let depth = 0, i = brace;
      for (; i < src.length; i++) {
        if (src[i] === '{') depth++;
        else if (src[i] === '}') { depth--; if (depth === 0) break; }
      }
      src = src.slice(0, at) + src.slice(i + 1);
    }
    // 只锚定 set 写入链（kv_store.set / kv.set 别名,常量引用形态留给 REGISTRY 同步）;
    // 读/删（get/delete/scan_prefix）不在写入面管辖——A 类读不受限
    const SET_LITERAL = /(?:kv_store|kv|core\.kv_store)\s*\.\s*set\s*\(\s*&?\s*(?:format!\s*\(\s*)?"([a-z_]+)[:{]/g;
    for (const m of src.matchAll(SET_LITERAL)) {
      const prefix = m[1];
      if (!prefix) continue;
      const entry = entries.find((e) => e.prefix === prefix);
      if (!entry) {
        findings.push({ rel, prefix, verdict: 'UNREGISTERED', msg: `前缀未登记: ${prefix}:` });
      } else if (entry.class === 'A' && !exempt.some((ex) => rel.startsWith(ex))) {
        findings.push({ rel, prefix, verdict: 'A_CLASS_DIRECT_WRITE', msg: `A 类前缀直写（必须走 write_path）: ${prefix}: —— 调用方不在 EXEMPT_CALLERS` });
      } else if (entry.debt) {
        findings.push({ rel, prefix, verdict: 'DEBT_OK', msg: `OPEN 债存量登记中: ${prefix}:（seal 改造另卡）` });
      }
    }
  }
}

const blockers = findings.filter((f) => f.verdict !== 'DEBT_OK');
const debts = findings.filter((f) => f.verdict === 'DEBT_OK');
for (const f of blockers) console.log(`❌ ${f.rel}: ${f.msg}`);
for (const f of debts) console.log(`⚠️  ${f.rel}: ${f.msg}`);
console.log(`KV 直写扫描: ${findings.length} 处（阻断 ${blockers.length} · 债存量 ${debts.length}）· REGISTRY 登记 ${entries.length} 前缀`);
if (blockers.length > 0) {
  console.log('（未登记/A 类直写即阻断——登记于 crates/aurora-core/src/kv_namespace_registry.rs）');
  process.exit(1);
}
console.log('✅ KV 命名空间门禁通过');

function fail(msg) { console.error('❌ ' + msg); process.exit(1); }
