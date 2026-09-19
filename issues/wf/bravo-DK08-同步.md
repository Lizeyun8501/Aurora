# WF-Bravo 任务书：DK-08 同步（aurora-sync 领地）

> 派发对象：Bravo 工作流 agent（独立上下文可执行）
> 生成：2026-09-19 · Repo: Lizeyun8501/Aurora · 基线 commit: 58a2a3c（main, CI passing）

## 你的领地（可修改）

- `crates/aurora-sync/**` —— **全部独占**
- `issues/` 下 DK-08 相关笔记（可选）

## 禁改（改动会被 Alpha 集成时打回）

- `crates/aurora-core/**`（traits 接口如需变更 → 提 issue 给 Alpha，周一冻结窗口合入）
- `apps/**`、`crates/aurora-security/**`、`crates/aurora-ai/**`

## 背景（DK-08 卡，50 人日，issue 清单 §[DK-08]）

- 当前 `state()` 已修为 `sync_version` + 3s 超时真实探测（V23-I0）
- **硬伤：三个适配器无 `recv_update` 覆写，默认实现返回空 Vec/None——增量同步从未真实发生**
- 已有结构：`router.rs`（四路选路）、`external/`（webhook/cloud_drive 等）、`iroh_transport.rs`、`conflict.rs`（CRDT 合并 + `.conflict-{ts}.md`）

## 第一切片（3–5 人日）：WebDAV 增量同步真实发生

1. 在 `external/` 新增 `webdav.rs` 适配器（HTTP PUT/GET/PROPFIND，复用 workspace 依赖的 reqwest）
2. 覆写 `recv_update`：PROPFIND 拉远端清单 → 比对 `sync_version` → 增量拉取变更笔记
3. 覆写 `sync_version`：返回远端最新版本标记（非 None）
4. 集成测试：**双实例增量同步**（本地 mockito 起 WebDAV stub）——变更一篇笔记 → recv_update 只拉增量（断言非全量回退）

## 验收标准

- `cargo test -p aurora-sync` 全绿；`cargo clippy -p aurora-sync --all-targets` 零警告
- 测试证明：**增量**发生（记录拉取条数/字节），非全量回退
- DoD 对应：「增量同步真实发生（非全量回退）」

## 工作流规约（全 WF 通用）

1. 分支：`wf/bravo-sync` 从 main 切出，**每日 rebase main**
2. 提交：小步快跑，conventional commits（`feat(DK-08): ...`）
3. **本地验证铁律**：提交前 `cargo clippy --all-targets`（CI -D warnings，Copy 类型别 .clone()）+ `CARGO_INCREMENTAL=0`（磁盘治理）
4. 集成：每日末推 `wf/bravo-sync`，Alpha 次日按序 merge → main
5. 迁移文件如需新增：向 Alpha 领编号，禁自选 seq
6. 遇接口不够用：**不要**改 aurora-core——在 issues/wf/ 写 `bravo-request-<主题>.md` 提给 Alpha
