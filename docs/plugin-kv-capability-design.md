# 插件 KV capability 设计稿（v0.1 草案）

**作者**: Alpha 2026-10-16 ｜ **状态**: 设计稿（未实施——待拍板后排期） ｜ **来源**: 安装面审计 A 挂起项 + 功能完善主线

## 1. 背景

- 现状：插件数据面只有 **doc 键集**（`docs` 共享 KV，L1 namespace `{id}:` 前缀隔离，16 卡语义）——通用 KV（`KVStore` trait 全量键域）对插件**未开放**
- 宿主特权键与插件 doc 键共域（SqliteStorage 单一 KV）：宿主保留前缀（`plugins:`/`settings:` 等）靠"插件 id 禁 `:`+前缀形不相交"间接保证（审计 A 已核无冲突面）——**但无显式保留清单与强制闸**
- 本稿目标：给插件**受控的 KV 读写能力**，同时把宿主保留前缀从"事实安全"升格为"显式强制"

## 2. 宿主保留前缀清单（枚举现有键域，实施前须 re-scan 复核）

| 前缀 | 归属 | 卡 |
|---|---|---|
| `plugins:installed:` / `plugins:blob:` | 安装面 | 2ca3d8c |
| `plugins:audit:snapshot` | 审计面 | 494e4a7 |
| `plugins:modcache:` | 编译缓存 | DK-模块编译缓存（8c138d8） |
| `plugins:policy:` | 沙箱策略 | DK-45 |
| `settings:` | 宿主设置（wifi_only 等） | sync_gate |
| `attachblob:` / `attach:` | 附件 | DK-09 |
| `trash:` | 回收站 | DK-02 |
| `sync:` / 水位类键 | 同步引擎 | DK-08/44 |
| `backup:` | 备份水位 | DK-03 |
| `links:` | 双向链接索引（出链/反链倒排） | DK-审阅P0-1 S1 |

**纪律**：新宿主键前缀落地必须同步登记本清单（演进为代码常量 `HOST_RESERVED_PREFIXES`，单一事实源）。另：`plugins:modcache:` 行"Bravo 在途"已过时（8c138d8 验收冻结）——登记时顺手修正为卡号。

> **闸状态注记（2026-10-17，S1 落地）**：`HOST_RESERVED_PREFIXES` 已落为代码常量（`wasm_runtime.rs`）——**"间接保证"→"强制闸落地"**。登记清单=`plugins:`/`settings:`/`links:`（键域 re-scan 复核：attachblob:/trash:/sync:/backup: 等宿主面键当前无插件 KV 通路——16 卡 doc 面+write_path 面，闸随通路扩容同步登记）。语义：**v1 拦写不拦读**（read/query 命中保留前缀=插件可见性策略，另批）；先于 namespace 检查——`{id}:` 键形相交（id=`plugins` 边界形）也拒。单测 5 例（命中拒+审计行/边界形/doc 键不误伤/纯函数命中判定/read v1 放行口径）。

## 3. 能力模型：**统一复用 L1 namespace（推荐案）**

**不引入新概念**——插件 KV 能力 = 自己 L1 namespace `{id}:` 下的键域：

- `kv_roots` 不新增 manifest 字段：插件天然获得 `{id}:` 前缀下 KV 读写（与 doc 键集同域）
- 跨插件共享：复用 DK-17 shares 语义（`share` 指定 doc_id 即可作用到 KV 键——**v2 再议**，v1 不开）
- 宿主保留前缀强制闸：`storage_guard` 扩展——目标键命中 `HOST_RESERVED_PREFIXES` 任一 → `denied`（reason=reserved-prefix）+ 审计行；**即使键形与 `{id}:` 相交也拒**（如插件 id 恰为 `plugins`——16.1 禁 `:` 不禁此形，硬编码清单兜底）

**否决案（记录理由）**：独立 `kv_roots` 白名单字段——能力面翻倍、manifest 复杂化、与 L1 语义重叠，收益仅"插件可用非自命名前缀"——无真实需求支撑。

## 4. 宿主函数面

- `kv_get(key) / kv_set(key, value) / kv_delete(key) / kv_scan(prefix)`——走既有 HostFunction 枚举扩展（四点同步惯例：枚举+分派+guard+审计）
- 审计节流：读类首调记行（read-doc 同款）；**写类逐条**（write-doc 同款——写是信任敏感面）

## 5. 配额（SandboxLimits 扩展）

- `kv_max_keys: u32`（缺省 1024——namespace 内键数上限，scan 计数 enforce）
- `kv_max_value_bytes: usize`（缺省 256KiB——单值上限）
- 沿 DK-45 policy 覆盖链：宿主 publisher 维度策略 > manifest 声明 > 缺省

## 6. 审计与可观测

- 所有 denied：reason（reserved-prefix / quota-keys / quota-size / namespace）+ 审计行（结构化 ring，17 惯例）
- guest 面探针断言入 e2e（拒绝语义全走 guest 面探针——17 的教训：host-invoked 特权面无 guard 语义）

## 7. 实施切片（拍板后排期）

| 切片 | 内容 | 规模 |
|---|---|---|
| S1 | `HOST_RESERVED_PREFIXES` 常量+storage_guard 保留前缀强制+审计行（**纯收紧，无新能力**——可独立先落） | S |
| S2 | kv_get/set/delete 宿主函数+配额+e2e 探针 | M |
| S3 | kv_scan+doc 键集与 KV 键域关系梳理（是否合一） | S-M |

## 8. 开放问题（待用户/后续拍板）

1. doc 键集（`docs`）与新 KV 面是否合并为单一键域？（合并=概念最少；分开=doc 语义保留同步/检索特殊性）
2. KV 键是否纳入同步（iroh 传输）？——不同步则插件数据单机（v1 建议不同步）
3. 配额缺省值是否需要用户可配置 UI？（v1 建议仅策略层，UI 随策略卡）
