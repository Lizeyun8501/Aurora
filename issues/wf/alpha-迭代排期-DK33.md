# 迭代排期 DK-33+（2026-10-05 盘点版）

> 盘点基线：DK-22~32 全闭环、安全审计 0 漏洞、性能资产三指标（search P50 1.04ms / tags 21ms / startup 153ms）、流水线零在途。

## 一、缺口盘点（repo 实锤）

| # | 缺口 | 位置 | 性质 |
|---|---|---|---|
| 1 | WASM 插件运行时未闭环（Module 加载/TypedFunc 调用两个 TODO） | aurora-core/wasm.rs:48,83 | 产品能力缺口 |
| 2 | WebSocket 同步传输未接入（fails loudly，T3） | aurora-core/p2p.rs:307 + router.rs×3 unimplemented | 核心能力缺口（最大） |
| 3 | 增量压缩 zstd 未接 | aurora-sync/incremental.rs:215 | 小缺口 |
| 4 | cloud api key 扁平存储（未入 secret store） | system_settings.rs:720 | 安全债务 |
| 5 | SmartFolderView mock 无样本（冒烟 SKIP） | DK-29 r4 留档 | 测试缺口 |
| 6 | perf 竞品对照列 TODO（Obsidian/思源） | docs/evidence/perf-baseline.md | 数据资产缺口 |

## 二、候选池评估

| 卡 | 价值 | 成本 | 风险 | 判定 |
|---|---|---|---|---|
| A1 WASM 插件闭环（DK-33） | 高：产品扩展性核心；**DK-31 wasmtime 36 刚验证兼容——趁热窗口** | 中 | 低 | **Bravo 主力** |
| B2 竞品对照实测（DK-34） | 中高：数据文化延伸+perf 资产表补全 | 中（Electron headless 可自主测） | 低（对照口径需冻结） | **Alpha 并行** |
| B1 SmartFolder mock 冒烟（DK-35） | 小：测试面补齐 | 小 | 低 | Alpha 空闲小卡 |
| A3 zstd 增量压缩（DK-36） | 小中：与 A2 同域，可搭车 | 小 | 低 | 小卡池 |
| C1 secret store 迁移（DK-37） | 中：密码学纵深（DK-30 延伸） | 小中 | 低（vault DEK 体系已有） | 小卡池 |
| A2 WebSocket 传输层（DK-38 预告） | 高：多设备同步根基 | 大 | 中（握手/冲突面） | **先出设计评估卡，下迭代主力** |

## 三、本迭代执行令

- **DK-33（Bravo 主力）**：WASM 插件运行时闭环——manifest.entry 读取 WASM 字节码→Module 编译→TypedFunc 导出调用链；DoD=插件加载+调用 e2e 测试绿+四门+复用 wasmtime 36 Engine 实例（不新增配置面）
- **DK-34（Alpha 并行）**：竞品对照实测——Obsidian/思源 Electron headless（Xvfb）冷启动 10k 库同口径计时+检索计时，perf-baseline.md 竞品列补齐；DoD=对照表落档+口径备注完整
- **小卡池**（Alpha 机动）：DK-35 mock 冒烟 → DK-36 zstd → DK-37 secret store
- **DK-38 预告**：WebSocket 传输层设计评估卡（先评估后立项——握手协议/冲突合并/断线重连三面）

## 四、裁决点（需拍板）

1. A2 同步传输是否本迭代启动设计评估（默认：DK-38 先评估卡）
2. 移动端 Android 打包链路（mobile-ffi）是否入池（默认：不入——等同步面成熟）

— Alpha 2026-10-05
