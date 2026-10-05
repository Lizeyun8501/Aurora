# Bravo · DK-31 交付报告 — wasmtime 36 升级落地 + TLS 链补洞

**Base**: 3a001b0（DK-28 验收后）｜**执行者**: Bravo｜2026-10-05

---

## 一、wasmtime 27→36 升级落地（任务 1：落地出口，非评估报告）

**触点盘点（数据决策前提）**：全库仅 `wasm_runtime.rs` 3 处纯 `wasmtime::Engine` 句柄持有（字段/default/getter）——**零 Module/Linker/Component/实例化调用**，API 断裂面=0 → **直接升级路线**（<3 人日改判线远未触发）。

- Cargo.toml `wasmtime = "27"` → `"36"`，Cargo.lock **36.0.17**（cranelift 0.114→0.123 全链联动）
- aurora-plugin 编译零错、全量回归通过——**升级落地完成**

## 二、TLS 链补洞（任务 2，超预期根修）

| 包 | 升级后 | 达标线 | 手段 |
|---|---|---|---|
| rustls | **0.23.45**（唯一实例） | ≥0.23.45 ✅ | `cargo update --precise` |
| rustls-webpki | **0.103.15**（唯一实例） | ≥0.103.13 ✅ | reqwest 0.12 根修联动 |
| h2 | **0.4.16**（唯一实例） | ≥0.4.16 ✅ | 同上 |
| protobuf | **3.7.2** | ≥3.7.2 ✅ | prometheus 0.13→**0.14** 联动拉升 |

**根修关键**：旧实例链（rustls 0.21/webpki 0.101/h2 0.3）全部由 reqwest 0.11 拖入——**reqwest 0.11→0.12**（aurora-ai/aurora-observability/aurora-sync 三 crate）+ prometheus 0.14 升级后，依赖树收敛为唯一达标实例（cargo tree 验证零多版本残留）。三 crate API 零断裂（reqwest 0.12 兼容面）。

## 三、DoD 3：cargo audit 复扫（cargo-audit 0.22.2 musl 二进制）

| 项 | 首扫 | 复扫 |
|---|---|---|
| Vulnerabilities（真漏洞） | **25** | **0**（wasmtime 18 条清零 + TLS 链清零——**远超 ≤7 达标线**） |
| 警告（unmaintained/unsound，非漏洞） | — | 21（atomic-polyfill/bincode/lru unsound 等——独立面，不涉安全漏洞） |

1290 条 advisory 库扫描、962 crate 依赖复核。

## 四、验证与领地

- 全量四门（wasmtime 36 + reqwest 0.12 + prometheus 0.14 全链回归后）：**TEST=0 CLIPPY=0 FMT=0 DESK=0** ✅
- 插件 parity：Engine 句柄路径不变（36 API 兼容），全量测试含插件面零回退
- 领地：Cargo.toml/workspace 依赖声明、aurora-plugin（零代码改动——纯版本升级）、aurora-ai/observability/sync Cargo.toml（版本行）——最小补丁
- audit 工具：cargo-audit 0.22.2 musl 预编译二进制（本机 cargo install 卡 rustc 1.91 依赖链——走 GitHub release 快路径，教训沉淀：安装优先官方预编译产物）

## 五、挂账

1. 21 条 unmaintained/unsound 警告（bincode 1.3/atomic-polyfill/lru 等）——非漏洞不涉安全，独立卡评估迁移（bincode 2.x 破坏性大，YAGNI）
2. wasmtime 36 启动/执行性能对照 bench——引擎句柄路径无执行差异面，免 bench（数据决策：触点盘点为零执行调用）
