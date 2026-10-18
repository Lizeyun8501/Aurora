# bravo-1.3-setup下沉 交付注记（2026-10-18）

依据: 7ded281 侦察卡 + alpha-approval-1.3-setup下沉.md 批复（按卡直落无需二次侦察）

## 交付

**bootstrap 新增三函数**（aurora-plugin 依赖加入——零 tauri 引入红线遵守）：
1. `build_hybrid_searcher(core, model, dim) -> (Arc<VectorIndex>, Arc<dyn EmbedProvider>, Arc<HybridSearcher>)`——类型全 core/ai 自有面（批复②授权前提成立）；dim=u32 对齐 vector_index_for 签名
2. `restore_plugin_runtime(runtime, kv) -> (usize, usize)`——restore_installed+restore_audit 组合（审计 fail-open 语义保持）
3. `spawn_hourly_ticker(booted, runtime)`——tick 全逻辑（备份水位/周校验/审计 flush）tokio::spawn 逐行等价搬迁

**desktop 侧调用点替换**（lib.rs 1723→1684 行，-39）：
- 混合检索段：构建迁出，**回填 spawn 留壳**（依赖 desktop static getter+unwrap_note_bytes 私有面——批复"只沉独立子段"）
- 插件段：PluginState::new()+app.manage **留壳**（desktop 私有类型），restore 组合面迁出+block_on+日志留壳
- tick 段：全迁（一行调用点替代 33 行内联）
- static 全局锁+invoke_handler 注册表**留壳**（批复③）

## 行为级证据（代码走读）

1. **tick 路径**：搬迁 diff 逐行对照——interval 3600s/Delay/首跳跳过/maybe_backup_on_boot/verify_backups_if_due/persist_audit 六段全等价，日志锚文案原样（"hourly tick backup created"/"weekly verify FAILED"/"audit snapshot flushed"）——运行时行为可由启动日志 grep 验证
2. **get_hybrid 恒等**：构建参数同源（"nomic-embed-text",768 字面量未动）+VEC/EMBED/HYBRID 三 static 挂载面不变+getter 未动——检索 cmd 零调用点变更
3. **插件 cmd 零回归**：plugin_commands.rs 零触碰；restore_installed/restore_audit 调用参数同源（&runtime+&kv_store）

## 顺手收口（帮 Alpha 2.2 领地例外披露）

todo_commands.rs:327 E0716（toggle 临时值语句尾释放——clippy --all-targets 面）——最小修：toggled owned String 先落再借用。

## 验证矩阵

本机: check 两面 0 / clippy --all-targets 两面 -D warnings 0 / fmt 实跑 0 / tsc 0
本机受限（CI 兜底）: bootstrap --lib test 链 iroh 全家桶+desktop lib test 链 webkit——本机空间/链接环境不可行（磁盘两度 os error 28 实证）——Test 面走 CI

## CI 终态（2026-10-18 补记）

fa8f6ad **8/8 全绿**（Clippy/Rustfmt/MSRV/desktop-check/Test stable/三 guest build）——Test stable 真验兜底兑现（bootstrap 链接面 CI 通过=本机空间受限面无损）。审阅 1.3 setup 巨石下沉**收官**。
