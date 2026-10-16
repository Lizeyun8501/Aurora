# 插件审计面：结构化审计存储+查询（Alpha 2026-10-16 等待轮）

**性质**: 独立切片（Bravo 下载器在途，零领地冲突——audit.rs 新文件） ｜ **状态**: 110 单元+22 e2e 全绿，clippy 零告警

## 缺口勘定

wasm_runtime 18 处 `eprintln!("[audit-host] …")`——stderr 即弃：无结构化存储、不可查询、不可持久化。16/17 的 deny/share/revoke 审计链只存在于 stdout 日志。

## 落地

1. **`audit.rs`**（新模块）：`AuditEntry`（ts/plugin/event/result/reason/detail，serde Serialize=tauri DTO 直投影）+ `AuditRing`（定容 1024 环形缓冲，满丢最旧——**容量即安全边界**，防恶意插件刷爆宿主内存）+ `recent(limit, plugin)` 查询（新→旧截取反转）
2. **双写纪律**：产生面 eprintln **全保留**（stderr 可观测/CI 抓取不变）+ ring 直写——18/18 处全覆盖（8 单行自动+10 多行手动）
3. **Arc 共享回指**：`WasmRuntime.audit` 与 `WasmStoreCtx.audit` 同一 Arc——宿主回调产生面直写无需回指 runtime；查询经 `runtime.audit_recent`
4. **tauri**：`cmd_plugin_audit_logs(limit, plugin_id?)`（limit 上限 1024）
5. **UI**：PluginsView 行级"审计"按钮+审计链展示区（ts/事件/结果色标/reason/detail）

## 验证

- `audit` 单测 5/7→合并后 110 全绿（容量环绕/plugin 过滤/limit 截取/serde 投影）
- **集成 e2e**：真实 wasm 执行验证（零权限拒→denied 行入环；hello init→ok 行入环；ts 非空+serde 可序列化）——e2e 21→22
- 语义注记：未注册件走 NotFound 不产生审计行（与既有 eprintln 语义一致）

## 踩坑

1. **产生面 self 二态**：runtime 侧 `&self=WasmRuntime`（plugin_id:&str），宿主回调侧 `&self=WasmStoreCtx`（plugin_id:String）——audit_log 需双侧可用：签名 `impl AsRef<str>` 一体解决（&str/&String/String 通吃），clippy E0599/E0308/E0382 三连教训：AsRef 签名下调用点传值即 move——**仍持后用的 String 调用点必须 &**
2. **Python 批量正则对 rust 宏插值失效**：`{plugin_id}` 格式占位需映射回裸变量表达式，字面量/变量混合形态自动化性价比低——**8 自动+10 手动**是务实配比
3. hello 权限集：集成测试 register 需主链同款四权限（fs/log/storage/user-output）——hello init 实际调 log+write-doc（与"hello=零依赖演示"直觉相反）

## 边界（诚实）

- **内存态先行**：ring 不落盘——落盘随同步卡（shares 图谱同节奏）；重启即清（stderr 双写仍有 CI 留痕）
- 审计面暂无插件侧查询（宿主运维语义——插件查自身审计=新语义面，另议）
