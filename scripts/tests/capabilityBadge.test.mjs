// DK-15c：capabilityBadge 逻辑单测（node:test——纯 JS 投影便于零基建运行；
// TS 源为单源，本文件为镜像断言——TS 侧变更需同步）
import { test } from 'node:test';
import assert from 'node:assert/strict';

// 投影自 apps/desktop/src/components/capabilityBadge.ts（badgeForGranted 逻辑）
const BADGE_MAP = {
  fs: { label: '文件系统', danger: true },
  network: { label: '网络', danger: true },
  native: { label: '原生宿主', danger: true },
  storage: { label: '文档存储', danger: false },
  log: { label: '日志', danger: false },
  'user-output': { label: '用户输出', danger: false },
  clipboard: { label: '剪贴板', danger: true },
  notifications: { label: '系统通知', danger: false },
  dialog: { label: '模态对话框', danger: false },
};
function badgesForGranted(granted) {
  const seen = new Set();
  const out = [];
  for (const g of granted) {
    if (seen.has(g)) continue;
    seen.add(g);
    const m = BADGE_MAP[g];
    out.push(m ? { key: g, label: m.label, danger: m.danger } : { key: g, label: 'unknown', danger: true });
  }
  return out;
}

test('granted 逐枚举映射：危险能力警示语义', () => {
  const b = badgesForGranted(['fs', 'storage', 'log']);
  assert.deepEqual(b.map((x) => [x.key, x.danger]), [['fs', true], ['storage', false], ['log', false]]);
});
test('重复 granted 去重', () => {
  assert.equal(badgesForGranted(['fs', 'fs', 'storage']).length, 2);
});
test('fail-closed：未知变体显示 unknown（不静默丢弃）', () => {
  const b = badgesForGranted(['bogus-perm']);
  assert.deepEqual([b[0].label, b[0].danger], ['unknown', true]);
});
test('空 granted=空序列', () => {
  assert.deepEqual(badgesForGranted([]), []);
});
test('BADGE_MAP 与 Rust 权威清单对齐（含 15b 预置 user-output）', () => {
  for (const k of ['fs', 'network', 'native', 'storage', 'log', 'clipboard', 'notifications', 'dialog', 'user-output']) {
    assert.ok(k in BADGE_MAP, `映射应含 ${k}`);
  }
  assert.equal(BADGE_MAP.fs.danger, true);
  assert.equal(BADGE_MAP.native.danger, true);
  assert.equal(BADGE_MAP.network.danger, true);
  assert.equal(BADGE_MAP.storage.danger, false);
});
