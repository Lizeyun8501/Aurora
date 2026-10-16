# DK-模块编译缓存派工单（M，性能优化）

**派给**: Bravo ｜ **派单**: Alpha 2026-10-16 10:30 ｜ **前置**: 无（市场系冻结，本卡纯性能）

## 背景与目标

**性能瓶颈勘定**：启动恢复（restore_installed）是 lazy 的（只登记 manifest 不实例化 ✓），但**启动后首次 invoke** 仍要 wasmtime `Component::new` 全量编译 wasm 组件——每次启动每插件一次（hello 1.2KB 尚可，真实插件 MB 级编译数百 ms-秒级）。

**目标**：component 序列化缓存——第二次启动起跳过编译。

## 设计要点（Alpha 预审通过）

1. **新模块 `cache.rs`**（你领地）：
   ```rust
   pub const MODCACHE_PREFIX: &str = "plugins:modcache:";
   // 键形态：plugins:modcache:{id}:{checksum 前 8 位}——checksum 绑定防陈旧
   pub async fn load_component_cached(
       runtime: &WasmRuntime, kv: &dyn KVStore,
       plugin_id: &str, wasm_bytes: &[u8], checksum: &str,
   ) -> Result<(), crate::Error>;
   ```
   - cache hit（键存在）→ `Component::deserialize`（wasmtime 36 API 名自查：`Component::deserialize(engine, ...)`——注意 unsafe 边界与 Engine 一致性要求）→ 失败 fail-open fallback 全量编译（wasmtime 版本升级/Engine 不匹配自然 miss，**不阻断**）
   - miss → 编译 → serialize 回写 KV
2. **接线**：
   - **invoke 路径**（wasm_runtime.rs 的 load_component 调用点）：先查缓存——**注意 invoke 路径拿不到 checksum**（manifest 无此字段）——方案：缓存键省略 checksum，改**值内嵌校验**（bincode 头含 wasm_bytes 的 sha256——load 时对当前字节验证，不匹配=miss）——你裁决两种形态取一，注记说明理由
   - **install 路径**（installer.rs 是 Alpha 领地**勿动**）：缓存回写放 invoke/加载路径统一处理（懒回写），installer 不碰
3. **容量护栏**：`plugins:modcache:` 前缀键数上限（建议 64，超限删最旧——按键名时间戳或 KV scan 排序）——防插件装卸循环膨胀
4. **单测**（确定性断言，勿用耗时断言）：
   - 首次加载→缓存键写入
   - 二次加载（新 runtime 实例+同 KV）→走 deserialize 路径（断言：注入损坏缓存→fail-open 编译成功+缓存回写修复）
   - 字节变更（checksum/sha256 头不匹配）→miss 重编译
   - 容量护栏触发删最旧

## 边界（停手线）

- installer.rs / audit.rs / fetcher.rs——**Alpha 领地勿动**（并行推进中）
- wasmtime 版本迁移策略/预热（启动即编译全部）——不做（lazy 语义保持）
- 移动端 FFI 面——不做

## DoD

1. 单测 ≥4 例全绿+既有 122 单元/22 e2e 零回归（全 guest 重建 SOP）
2. clippy/fmt 零告警；wasmtime unsafe 边界注记
3. 注记 issues/wf/bravo-DK-模块编译缓存.md（API 名/校验形态裁决理由/fail-open 语义）
4. push main（惯例直推）

## 已知坑

- **本地无 GTK**：desktop 面编译 CI 验证；你只动 aurora-plugin crate 即可本地全验
- **wasmtime 36 API**：`Component::serialize/deserialize` 是否存在先 cargo doc 确认——若 component 模型不支持 serialize，fallback 方案=**Engine 级 module 缓存不可行则本卡降级为"编译耗时 bench 报告"**（先 spike 15min 验证可行性再动工，注记结论）
- Rust 1.85 语法限制（无 is_multiple_of）
