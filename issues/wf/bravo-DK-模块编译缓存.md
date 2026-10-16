# bravo-DK-模块编译缓存 注记（Bravo 2026-10-16）

**commit**: 见 git log（feat(DK-模块编译缓存)）｜ **派单**: c231cf4

## Spike 结论（wasmtime 36.0.17 实证）

- `Component::serialize(&self) -> Result<Vec<u8>>`（**safe**）——src/runtime/component/component.rs:523
- `pub unsafe fn deserialize(engine: &Engine, bytes) -> Result<Component>`——:208
- **可行性成立，本卡不降级 bench 报告**

## API 与 unsafe 边界

- `deserialize` 的 unsafe 前提=bytes 必须来自**同 wasmtime 版本+同 Engine 配置**（doc 同 Module::deserialize——不匹配可能 **UB 而非 Err**）
- **双头防线**（失败路径绝不触碰 deserialize）：
  1. `engine_tag`：编译期 `aurora-plugin/<CARGO_PKG_VERSION>`（wasmtime 升级必随动——tag 漂移即 miss）
  2. `sha256(wasm) 32B`：KV 字节损坏 → 校验拒绝
- **残余风险**（如实注记）：engine_tag 碰撞（同 tag 异 config）——本进程 Engine 配置硬编码（component_model+epoch 不变），单进程桌面无跨配置恢复场景

## 校验形态裁决（卡面二选一 → A）

**A 键省 checksum+值内嵌头**：invoke 路径只有 wasm_bytes，listing.checksum 数据源不可达；B 需把 checksum 扩散进 invoke 签名链（越 installer 领地）。值头自包含：`AUMC | tag_len | engine_tag | sha256(wasm) | ts(LRU 依据) | serialized`。checksum 参数保留为双保险口（数据源可达时对账，invoke 路径传 ""）。

## fail-open 语义

命中失败（损坏/漂移/Err）→ 全量编译+stash+serialize 回写修复缓存；写失败不阻断。**缓存是优化不是正确性依赖**；仅 load_component 编译失败上抛（与无缓存行为一致）。

## 接线（lazy 语义保持）

- `cache.rs`（新，Bravo 领地）：`load_component_cached(runtime, kv, plugin_id, wasm_bytes, checksum)`+`trim_modcache`（64 上限按头 ts 删最旧）
- `wasm_runtime.rs`：`cached_components` 槽+`stash_cached_component`/`take_cached_component`（取走即清幂等）+`component_bytes` getter；`get_or_instantiate` 慢路径槽优先（无槽原链 fail-open 不变）
- prime 调用点=**desktop cmd_plugin_invoke async 面**（spawn_blocking 前先行——invoke 本体 sync，KV 读需 async 面；命中/编译产物经槽交付同步链）
- installer/audit/fetcher **零触碰**；首启行为=编译（与现状等价不劣化），二启起=deserialize

## 验证矩阵

| 门 | 结果 |
|---|---|
| lib 单元 | 127 ok（122+5 cache） |
| e2e | 22 ok（全 guest 重建 SOP） |
| clippy -p aurora-plugin（lib） | 0 |
| clippy workspace excl desktop（CI 同款） | 0 |
| fmt --check | 0 |

## 诚实边界

- **audit.rs L262 test 目标 unused import**（Alpha 5176113 引入，`--all-targets` 判据报）——**Alpha 领地未动**；CI workspace 命令（无 --all-targets）不卡，Alpha 侧顺手修
- 容量护栏 LRU=头内 ts（写时刻秒级）——同秒并列时删除序退化按键序（现实影响可忽略：护栏只防膨胀不保精确 LRU）
- 缓存命中路径无耗时断言（卡面纪律）——收益证明=形态正确性断言（digest 绑定/修复回写）
- 真实 MB 级插件 deserialize 收益未 bench（无真实 wasm 产物；spike 已证 API 可行）
- desktop 面编译本地无 GTK——CI desktop-check 兜底

