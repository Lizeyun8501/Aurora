# bravo-request：DK-1.3 setup 巨石下沉——接入点侦察（窗口切片）

> 侦察方: Bravo ｜ 2026-10-17 ｜ 依据: 审阅报告 1.3（lib.rs setup 巨石 ~1700 行）+ 导出格式扩展回执 P1 预告（1.3 下沉收尾）
> 性质: **侦察报告非派单**——Alpha 批复后可按此直接落任务书（S2 复核先例：侦察→批复→派单零等待）

## 现状盘点（本机实测）

- `apps/desktop/src-tauri/src/lib.rs` = **1722 行**；setup 入口 L250；bootstrap() 装配已下沉（迁移/DEK/AppCore/startup——bootstrap crate 1123 行）
- **仍在 setup 内联的三块**（下沉对象）：

| # | 块 | 位置 | 依赖面 | 下沉目标（建议） |
|---|---|---|---|---|
| ① | 每小时 tick 定时器（备份水位+周校验+审计 flush） | lib.rs L363-410 区 | booted_arc+plugin_state+tokio interval | `aurora_bootstrap::spawn_hourly_ticker(&booted, runtime)` |
| ② | 混合检索装配 | get_hybrid() static+setup 构建 | booted.core+embedding 面 | `aurora_bootstrap::build_hybrid_searcher(&booted)` |
| ③ | 插件状态构建+manage | plugin_state 构建区 | WasmRuntime+audit+PluginState | `aurora_bootstrap::build_plugin_state()`（tauri 无关部分） |

- **bootstrap 依赖面已备**：tokio workspace 依赖在案（L31/36）——tick 用 `tokio::spawn` 等价 `tauri::async_runtime::spawn`（后者是 tokio 包装）；**bootstrap 零 tauri 依赖**——下沉不引入
- **壳保留合理面**：get_core/get_vault/get_hybrid 等 static 全局锁+invoke_handler 注册表留 lib.rs（壳语义）

## 风险与边界（预写死，防执行期扯皮）

1. **领地**：desktop src-tauri（Bravo 现役）+ aurora-bootstrap（Alpha 域——cache_lock 先例）——**跨 crate 协作卡**，任务书须写明 bootstrap 新增函数签名预批或 Alpha 自落 Bravo 只迁 lib.rs 侧
2. **风险点**：plugin_state 构建含 tauri State 管理（app.manage 留壳）——③建议只下沉构建不接管 manage；tick 内 persist_audit 是 async——bootstrap 侧需 async runtime 在（tokio rt 在案）
3. **验收**：check+clippy+fmt 三件+行为级=tick 仍按小时触发（日志锚）/混合检索 get_hybrid 恒等/插件 cmd 全零回归；CI 全绿

## 预估

S（纯搬迁+单测薄）——desktop 侧 lib.rs 预计 -150 行至 ~1570；bootstrap +80~100 行。
