# Aurora Note

本地优先的个人知识管理工具：Markdown 编辑 × CRDT 协同 × GTD 任务 × 端到端加密，单机与 P2P 同步皆为一等公民。

> **V26 迭代进行中** — M0（止血与收敛）已完成，M1（核心基石）推进至 80%。

## 架构总览

Rust Workspace（10 crate / 66K 行）+ 4 端应用（desktop/mobile/web/extension）：

```
┌─ apps ─────────────────────────────────────────────┐
│  desktop (Tauri)   mobile (WebView+React+Vite)     │
│  web               extension                       │
└────────────┬───────────────────────────────────────┘
             │ uniffi FFI / Tauri IPC / WASM
┌────────────▼───────────────────────────────────────┐
│  aurora-mobile-ffi   mobile JNI 边界               │
│  aurora-bootstrap    装配层（DI 组合根, 7 大端口） │
│  aurora-core         应用核心（事件总线/投影/写路径）│
│  ├─ write_path       唯一写入入口（ADR-003）       │
│  ├─ event_bus        分层事件（High/Medium/Low）   │
│  ├─ projections      搜索/任务/双链/时间机器投影   │
│  └─ l1_infrastructure SQLite/原子事务/Loro/Tantivy  │
│  aurora-security     AES-256-GCM + Argon2id + ML-KEM │
│  aurora-sync         iroh P2P 同步                 │
│  aurora-ai           Agent 上下文/GTD 提取         │
│  aurora-plugin / aurora-observability / entity     │
│  migration            版本化 Schema 迁移           │
└────────────┬───────────────────────────────────────┘
             │
      ┌──────▼──────┐
      │ aurora.db   │ SQLite（KV/事件流/blocks/组织表）
      │ tantivy/    │ 搜索索引
      └─────────────┘
```

**核心设计契约**（详见 `docs/adr/`）：

- **WritePath 唯一写入入口**（ADR-003）：load → apply → 原子保存 → 派生 → 发事件，桌面与移动端调用同一实现
- **权威源三阶段**（ADR-004）：notes 元数据为权威指针，blocks/搜索索引为派生（失败不阻断主流程，启动时自动重建），Loro 快照双写观察期后接管正文权威
- **分层事件总线**：High（实时 UI）/ Medium（持久化重放驱动投影）/ Low（后台），投影水位线对比事件流决定增量追赶或全量重建

## 快速开始

```bash
# 全量检查（CI 同款门禁）
cargo +1.91 fmt --all -- --check
cargo +1.91 clippy --workspace --exclude aurora-desktop -- -D warnings
cargo +1.91 test --workspace --exclude aurora-desktop

# 桌面端
cd apps/desktop && npm install && npm run tauri dev

# 移动端（WebView bundle）
cd apps/mobile && npm install && npm run build
# 产物 dist/index.html 注入 Android assets/

# Android FFI（NDK aarch64）
cargo +1.91 build -p aurora-mobile-ffi --release --target aarch64-linux-android
```

**工具链**：Rust 1.91（MSRV，见 ci.yml MSRV job）/ Node 20+ / Android NDK。

## CI 门禁

`.github/workflows/ci.yml` 五 job 全绿为合并门槛：

| Job | 内容 |
|---|---|
| Rustfmt | `cargo fmt --all -- --check` |
| Clippy | `-D warnings`（stable 工具链最新 lint） |
| Test | `--workspace --exclude aurora-desktop`（1100+ 测试） |
| MSRV | Rust 1.91 编译验证 |
| desktop-check | Tauri 桌面端编译（含系统库安装） |

## 文档索引

| 文档 | 内容 |
|---|---|
| `docs/adr/ADR-001` | 前端框架选型 |
| `docs/adr/ADR-002` | 契约治理（事件字典/错误码单一来源） |
| `docs/adr/ADR-003` | WritePath 唯一写入入口（双端同源） |
| `docs/adr/ADR-004` | 权威源三阶段与投影自愈 |
| `docs/DK-05M-V-验证报告.md` | WebView 输入时序验证（GO 判定 + 万字 60fps） |
| `issues/v26_issue清单.md` | V26 迭代任务卡（DK/RV 编号体系） |
| `AGENTS.md` | 工作区协作规范 |

## 迭代进度

| 阶段 | 迭代 | 状态 |
|---|---|---|
| M0 止血收敛 | I0 CI 五绿 / I1 契约冻结 / I2 写入路径统一 | ✅ |
| M1 核心基石 | I3 存储迁移（V5/原子事务/权威源）/ I4 编辑器（Go 门+第一二轮） | 🔄 80% |
| M2 任务安全 | DK-06 GTD / DK-07 安全 | ⏳ |
| M3+ | 协同/发布/性能 | ⏳ |

## 安全

- 笔记正文端到端加密（AES-256-GCM，DEK 本地保险库），P2P 同步仅传密文
- ML-KEM-768 后量子密钥封装（安全通道）
- 无遥测、无云端账号依赖
