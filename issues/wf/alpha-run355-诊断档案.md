# CI Run 37564438794 (5b181cd) 诊断记录 — 2026-10-07 11:41 触发

## 终态
- **failure**，run 时间 02:57:09Z→03:09:17Z（12min）
- Clippy / Rustfmt / desktop-check / MSRV 全绿
- Test (stable) job 内：Build ✅ + 主 Test step ✅（**swap+重试自愈对常规 Test 生效，354 常规红闭环**）
- 唯一红 step：**Test (iroh-transport feature)** —— 重试后仍红（两轮同位失败）

## Annotation 首秀（check-run 112608837179，6 条）
- `iroh-failed-list>>` **空** = 没有任何测试 FAILED（没跑到测试阶段）
- `iroh-oom>>` **空** = dmesg 无 OOM/killed process（**非 OOM kill**）
- `iroh-raw-tail>>` = `--verbose` 下 cargo 的 `Caused by: rustc ... (exit status: 1)` 命令行 dump + `warning: build failed, waiting for other jobs to finish...` = **编译期失败**
- rustc 命令 `--extern` 含 tantivy/wasmtime/sled/sha3/sqlparser（aurora-sync 的 dev-dep 拉入 core/note 面编译）
- 乱码 `` 为 tail 输出分隔符
- 注：`rg|head|tr` 管道 exit code 是 tr 的 0 → `|| echo none-captured` 永不触发，空=真无匹配

## 根因链（git 时间线，本地 repo 确认）
- fd1464d：CI 全绿基准（chacha20 升级，**不含 DK-40a V2**）
- **9ce1d07：dk40a(V2) 混合存储代码 —— fd1464d 后唯一一笔 Rust 代码**
- 56e8ff6（代码面=9ce1d07）：iroh step **首红** → 84ec381、944e1d6（空提交）、04dbeaf、5b181cd 连红 5 笔
- **误判**：944e1d6 空提交定性"fd1464d 同代码面绿→环境 flake"不成立——fd1464d 与 56e8ff6 代码面差 9ce1d07！空提交只证明 56e8ff6→944e1d6 无代码差异
- 真嫌疑：**9ce1d07 DK-40a V2 代码在 `--features iroh-transport` + `RUSTFLAGS=-D warnings`（ci.yml 全局）下编译不过，warning 升 error**
- 本地"224 全绿"大概率未带 iroh-transport feature 或未设 -D warnings

## CI 条件（ci.yml）
- 全局 `RUSTFLAGS: "-D warnings"`（L11）
- iroh step：`cargo test -p aurora-sync --features iroh-transport --verbose`，失败重跑一次（同 target dir）
- 限流：匿名 GitHub API 已耗尽（reset 1791346735 ≈ 12:12），job logs 匿名 403

## 本地复现（进行中）
- 脚本 `/home/z/my-project/scripts/iroh_repro.sh`，日志 `/home/z/my-project/tmp_research/iroh_check.log`
- 命令：`RUSTFLAGS="-D warnings" cargo check -p aurora-sync --features iroh-transport --all-targets`
- 结果：待查
