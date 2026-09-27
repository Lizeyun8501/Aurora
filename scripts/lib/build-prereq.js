/**
 * 构建前置共享库（DK-05/DK-12 脚本基建 · dist 不入仓模式配套）
 *
 * 背景（D1 小卡裁决）：dist 产物不再入仓（断链根因：index.html 引用新 hash assets
 * 而 assets 被 .gitignore 挡住 → 拉取方 404 白屏）。脚本改为自包含：
 * 检测产物新鲜度（mtime 对比源文件）→ 陈旧/缺失时自动 build。
 *
 * 用法：const { ensureLabBuild } = require('../lib/build-prereq'); await ensureLabBuild(REPO);
 */
const { spawnSync } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

const newestMtime = (paths) => {
  let m = 0;
  for (const p of paths) {
    try { m = Math.max(m, fs.statSync(p).mtimeMs); } catch { /* 缺失视为 0 */ }
  }
  return m;
};

/** dist-lab 产物是否新鲜（比源文件旧则需 build）。 */
function labStale(repo) {
  const labHtml = path.join(repo, 'apps/desktop/dist-lab/editor-lab.html');
  const distHtml = path.join(repo, 'apps/desktop/dist/index.html');
  const productMtime = Math.min(
    fs.existsSync(labHtml) ? fs.statSync(labHtml).mtimeMs : 0,
    fs.existsSync(distHtml) ? fs.statSync(distHtml).mtimeMs : 0,
  );
  const srcMtime = newestMtime([
    path.join(repo, 'apps/desktop/editor-lab.html'),
    path.join(repo, 'shared/ui-components/src/editors/auroraEditor.ts'),
    path.join(repo, 'shared/ui-components/src/schema/auroraSchema.ts'),
  ]);
  return productMtime < srcMtime || productMtime === 0;
}

/**
 * 确保 dist-lab + dist 构建产物新鲜；陈旧/缺失则同步重建（阻塞）。
 * vite bin 直启（D2 加固模式）+ 产物探活。失败抛错退出。
 */
function ensureLabBuild(repo, { needDist = true } = {}) {
  if (!labStale(repo)) return false; // 产物新鲜，零成本跳过
  const cwd = path.join(repo, 'apps/desktop');
  const bin = path.join(repo, 'node_modules/.bin/vite');
  const viteBin = fs.existsSync(bin) ? bin : 'vite';
  const configs = needDist
    ? ['vite.config.editorlab.ts', 'vite.config.ts']
    : ['vite.config.editorlab.ts'];
  for (const cfg of configs) {
    console.log(`[build-prereq] 产物陈旧/缺失，重建: vite build --config ${cfg} ...`);
    const r = spawnSync(process.execPath, [viteBin, 'build', '--config', cfg], {
      cwd, stdio: ['ignore', 'pipe', 'inherit'],
    });
    if (r.status !== 0) {
      console.error(`[build-prereq] vite build 失败（${cfg}）exit=${r.status}`);
      process.exit(3);
    }
  }
  // 产物探活
  const mustExist = [
    path.join(repo, 'apps/desktop/dist-lab/editor-lab.html'),
    ...(needDist ? [path.join(repo, 'apps/desktop/dist/index.html')] : []),
  ];
  for (const f of mustExist) {
    if (!fs.existsSync(f)) {
      console.error(`[build-prereq] 产物探活失败: ${f} 不存在`);
      process.exit(3);
    }
  }
  console.log('[build-prereq] 产物就绪');
  return true;
}

module.exports = { ensureLabBuild, labStale };
