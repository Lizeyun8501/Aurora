# Alpha 验收回执 — Bravo DK-31 wasmtime 36 升级 + TLS 链补洞

> c6cfbf1 · 2026-10-05 · 三件套全过，裁定 **通过** ✅

## 一、三件套

### ① 代码实锤

| 项 | 实锤 |
|---|---|
| wasmtime 27→36.0.17 | ✅ Cargo.toml `wasmtime = "36"` + lock 36.0.17；触点盘点成立（aurora-plugin/wasm_runtime.rs + aurora-core/wasm.rs，仅 Engine 句柄 3 处、零 Module/Linker/实例化——直接升级路线合理，workspace TEST=0 全绿覆盖） |
| TLS 四项达标 | ✅ lock 实测：rustls **0.23.45** / rustls-webpki **0.103.15** / h2 **0.4.16** / reqwest **0.12.28**（+0.13.4 节点同 rustls 0.23 链）；reqwest 0.11→0.12 根修三 crate（ai/sync/observability）+prometheus 0.14 联动 |
| 旧链清零 | ✅ lock **零残留**：无 rustls 0.21/h2 0.3/webpki 0.101 节点（rg 全扫零命中）——依赖树收敛唯一达标链 |
| audit 复扫 25→0 | ✅ 下方本地复核 |

### ② 本地复跑（Alpha 独立）

- `cargo-audit audit`（musl 二进制，962 crate 依赖扫描）：**0 vulnerabilities + 21 条警告（全部 unmaintained 类，无漏洞）**——"25→0 远超 ≤7 线"复核成立；
- `cargo fmt --all --check`：0；`cargo clippy --workspace --all-targets`：**0 警告 0 错误**。

### ③ 时间线

c6cfbf1 → **CI 五绿**（Test/Clippy/MSRV/Rustfmt/desktop-check 全 completed success）——闭合。

## 二、裁定与队列

**通过** ✅——**DK-30 audit 基线 25 条漏洞 → DK-31 升级后 0 条，安全闭环**。经验收记档：audit 工具 musl 直调需显式 `audit` 子命令（裸调打印 help）。

**流水线队列**：DK-31 闭环后**当前零在途**。挂起池：notesnap 多用户场景（待真实需求）。周一 11:00 bench cron 首跑待核验（15:53 已过 11:00——**cron 首跑核验转下一动作**）。

— Alpha 2026-10-05
