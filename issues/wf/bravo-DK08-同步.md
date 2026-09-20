# WF-Bravo 任务书：DK-08 同步 — WebDAV 增量适配器（自包含版）

> 派发对象：Bravo 工作流 agent（**零上下文可执行**）
> 生成：2026-09-19 18:38 · 基线 commit: d62bfbc（main）· 仓库: https://github.com/Lizeyun8501/Aurora
> 预计 3–5 人日 · 你是四条并行工作流之一（Alpha 主线 / Charlie AI / Delta 画布），与其他 WF 零文件交集

---

## 1. 你的使命

实现 Aurora 笔记应用的 **WebDAV 同步适配器**，让「增量同步真实发生」（DK-08 卡 DoD 第 1 条）。这是 50 人日卡的第一切片，交付后继续二、三切片（见 §7）。

## 2. 环境速查

| 项 | 值 |
|---|---|
| 仓库路径 | `/home/z/my-project/repos/Aurora`（本机已有；新环境 `git clone https://github.com/Lizeyun8501/Aurora.git`） |
| 工作分支 | `wf/bravo-sync`（从 main 切出；**首次需自建**：`git checkout -b wf/bravo-sync origin/main`） |
| Rust 工具链 | `cargo +1.91.0`（必须带版本号）；环境变量 `RUSTUP_HOME=/home/z/.rustup CARGO_HOME=/home/z/.cargo PATH=/home/z/.cargo/bin:$PATH` |
| 验证命令 | `cargo +1.91.0 test -p aurora-sync` / `cargo +1.91.0 clippy -p aurora-sync --all-targets` |
| CI | push 触发 GitHub Actions（~10min 全量）；badge: repo README 顶部 |
| push 认证 | `git -c core.sshCommand="python3 /home/z/.ssh/git_ssh_wrapper.py" push origin wf/bravo-sync`（本机已配；新环境需向用户索取凭据） |
| git commit 规范 | `feat(DK-08): ...` / `test(DK-08): ...` / `fix(DK-08): ...` 小步提交 |

## 3. 领地（可修改）与禁改

**可修改**：`crates/aurora-sync/**` 全部独占 + `issues/` 下你的工作笔记。

**禁改**（越界集成时被打回）：
- `crates/aurora-core/**`——SyncTarget trait 已冻结（DK-00 契约）。**接口不够用不要改它**，在 `issues/wf/bravo-request-<主题>.md` 写需求给 Alpha，每周一冻结窗口合入
- `apps/**`、`crates/aurora-security/**`、`crates/aurora-ai/**`、`crates/aurora-bootstrap/**`

## 4. 现状盘点（Alpha 已核实，直接用）

### 4.1 SyncTarget trait（`crates/aurora-core/src/traits/sync_target.rs`）

```rust
#[async_trait]
pub trait SyncTarget: Send + Sync {
    async fn connect(&mut self, e: &Endpoint) -> Result<Connection, Error>;
    async fn sync(&self, c: &Connection, d: &DocSet) -> Result<SyncReport, Error>;

    // ★ DK-00 增量三原语（V26 契约冻结）：
    async fn send_update(&self, c: &Connection, u: &UpdatePayload) -> Result<(), Error>;
    async fn recv_update(&self, c: &Connection, doc_id: &str) -> Result<Vec<u8>, Error>;
    async fn sync_version(&self, c: &Connection, doc_id: &str) -> Result<Option<u64>, Error>;
    // 注意：sync_version 无默认实现（编译期强制实现）；对端不支持时 Ok(None) 是合法显式声明
    fn watch(&self, cb: Box<dyn Fn(SyncEvent) + Send + Sync>);
    async fn disconnect(&self, c: &Connection) -> Result<(), Error>;
}
```

> ⚠️ 事实纠正：issue 清单写「三适配器无 recv_update 覆写，默认实现返回空 Vec/None」**已过时**——DK-00 后 sync_version 已无默认实现。现状是：**external/ 下的适配器没有一个是「真网络增量同步」**，你的 WebDAV 是第一个真实现。

### 4.2 参照样板（照着写）

- **`crates/aurora-sync/src/external/webhook.rs`**（666 行）——external/ 内最完整的适配器，含 `mod tests`（393 行起），用 **mockito + tempfile** 测试模式。你的 WebDAV 照此结构
- **`crates/aurora-sync/src/router.rs`**——四路选路 + 健康探测（`state()` 已实现 sync_version + 3s 超时，V23-I0）；NoopTarget 测试桩
- **`crates/aurora-sync/src/external/mod.rs`**（507 行）——external 模块组织
- workspace 依赖已有：`reqwest`、`mockito`、`tempfile`、`tokio`——**不要新增重依赖**

### 4.3 数据模型

- 文档以 **Loro OpLog 快照 bytes** 同步（`recv_update` 返回 `Vec<u8>`）
- KV 键：`note:{id}`（NoteRecord JSON，可能带 `encryption: "aes256gcm"` 密级字段）；同步层工作在 Loro oplog 层**不受本机 at-rest 封装影响**（isomorphic_write_path.rs 已验证）
- 版本标记：`sync_version` 返回 `Option<u64>`（Lamport/版本向量，自定协议但必须是真增量判定依据）

## 5. 第一切片任务（3–5 人日）：WebDAV 真增量同步

### 5.1 新建 `crates/aurora-sync/src/external/webdav.rs`

1. `WebDavTarget` 实现 `SyncTarget`——HTTP 用 reqwest（Basic Auth：Endpoint 里带 url/username/password）
2. 路径布局（约定）：`{base}/aurora/{doc_id}.oplog` + `{base}/aurora/index.json`（doc_id → {version, etag} 清单）
3. **`sync_version`**：GET index.json（或 HEAD 文件取 ETag）→ 该 doc 的远端版本；404 → `Ok(None)`
4. **`recv_update`**：比对本地/远端版本 → 仅远端较新时 GET 拉取 oplog bytes；版本相同 → 返回空 Vec（表示无更新——**增量语义的证据**）
5. **`send_update`**：PUT 上传 oplog + 更新 index.json
6. external/mod.rs 注册模块

### 5.2 测试（`webdav.rs` 内 `mod tests`，mockito 起 WebDAV stub）

- `dk08_webdav_recv_incremental_only`：远端 index.json 标记 v2、本地 v1 → recv_update 拉到 bytes；本地 v2 → 返回空 Vec。**断言 HTTP 请求次数**（版本相同路径不得 GET oplog 文件本体——这是「非全量回退」证据）
- `dk08_webdav_send_then_recv_roundtrip`：send PUT 后 sync_version 返回新版本，recv 拉回 bytes 与发送一致
- `dk08_webdav_auth_failure_fails_closed`：401 → Err（不静默返回空数据）
- `dk08_webdav_index_missing_treated_as_empty`：index.json 404 → 全部 doc 版本 None（首次同步语义）

### 5.3 集成进 router（可选加分）

`router.rs` 四路选路中把 WebDAV 挂为 WebDAV 路由的 target——若 Endpoint/路由配置结构不支持，写 request 文档给 Alpha，不硬改。

## 6. 验收标准（全绿才算切片完成）

- [x] `cargo +1.91.0 test -p aurora-sync` 全绿（S1-S4 默认集 201 通过；S5 带 `--features iroh-transport` 211 通过）
- [x] `cargo +1.91.0 clippy -p aurora-sync --all-targets` **零 warning**（默认集；iroh-transport 集编译干净）
- [x] `cargo +1.91.0 fmt --all` 干净
- [x] 测试证明**增量语义**（请求次数断言），非全量回退
- [x] `wf/bravo-sync` 分支已推远端，commit 信息符合规范（验收后分支已清理）

### Alpha 验收记录（2026-09-20 18:5x · 合入 6558187）

**DK-08 全切片（S1-S5）验收通过，卡关闭。**

- S1 WebDAV 真增量 / S2 CRDT 冲突 / S3 分片续传 / S4 Wi-Fi 门：默认 feature 集 201 测试全绿 + clippy 清（早上会话）
- S5 多节点多 NAT 仿真（iroh TestNetwork 内存网络）：双节点双向 / 4 节点环多跳 / 5 节点星型 CRDT 收敛断言全绿——**需 `--features iroh-transport`**（见下）
- 裁决遗留已闭环：`Endpoint.auth` 方案 A + `SyncProtocol::WebDav` 排期 Alpha 周一冻结窗口；mobile-ffi `NetworkStateProvider`（Android ConnectivityManager）Alpha 下轮
- ⚠️ **CI 盲区（转 Alpha 待办）**：S5 模块被 `#[cfg(feature = "iroh-transport")]` 门控，CI 默认集**不覆盖** S5 测试与生产传输代码。处置：CI 增补 `cargo test -p aurora-sync --features iroh-transport` job（Alpha 排期）
- 两份 request 裁决已写回对应文档（ffba558）

## 7. 后续切片队列（第一切片验收后按序领）

1. **断点续传/大文件分片**（DK-08 任务 5）
2. **冲突处理**：CRDT 自动合并（loro merge）+ 真冲突 `.conflict-{ts}.md` 副本（conflict.rs 已有骨架）
3. **移动端仅 Wi-Fi 同步**（可配置）——需 Alpha 接口（写 request 文档）
4. **多节点多 NAT 仿真测试**（3–10 节点，iroh_transport.rs 配合）

## 8. 已知坑（Alpha 踩过，直接绕行）

1. **CI -D warnings**：`Copy` 类型（如 `[u8; N]`）别调 `.clone()`（clippy::clone_on_copy）——测试代码也过门槛
2. **提交前必须跑 clippy --all-targets**，不是只 test+fmt（run 110 教训）
3. **磁盘治理**：全量编译膨胀 20G；用 `CARGO_INCREMENTAL=0` 环境变量；磁盘 >90% 时 `rm -rf target/debug/{deps,incremental}`（比 cargo clean 快）
4. **aurora-core 全量编译 ~10min / aurora-mobile-ffi ~40min**——你只碰 aurora-sync（依赖轻），首次编译后增量很快
5. python 脚本改 Rust 文件注意：Write 工具落盘可能 root 属主 → 后续 python 无权改；用 Edit 工具或先 chown
6. **别信对话记忆，信编译器和测试**——每个断言改动后立即重跑

## 9. 工作流规约（与其他 WF 并行的关键）

1. 每日 `git fetch origin && git rebase origin/main`（main 由 Alpha 集成推进，你 rebase 即可拿到其他 WF 成果）
2. 只推 `wf/bravo-sync`——**永远不直接推 main**（branch protection 已开，你也推不动）
3. 每日末推分支一次（集成窗口：Alpha 次日按序 merge）
4. 迁移文件编号向 Alpha 领取（你本切片应该不需要）
5. 遇到阻塞 >2h：在 issues/wf/ 写 `bravo-blocker-<主题>.md` 说明，切下一个子任务
