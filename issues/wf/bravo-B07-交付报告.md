# B-07 交付报告：插件 per-plugin namespace（C-ns）+ 容量 guard 审计行

> 执行: Bravo ｜ 工单: TA 2026-11-15 13:01 ｜ 核心发现: **C-ns 主体 DK-16 L1 已落**——本卡=缺口收口+台账关账

## 首节 · 开工令假设与代码实况对账

| # | 工单断言 | 实况核实（锚点） | 处置 |
|---|---|---|---|
| 1 | 「docs 全 runtime 共享,无 per-plugin 隔离」 | **已过时**——DK-16 L1（10-17 落）storage_guard 第二环已强制 `{plugin_id}:` 前缀（wasm_runtime.rs:1510+）;越界=CapabilityDenied+审计（namespace-denied）;query 键集过滤本插件前缀（:1428）;share/revoke 显式授权面 | 复核+台账关账（零改动——既有语义即工单目标） |
| 2 | 前缀形 `plugin:{plugin_id}:` | 现状 `{plugin_id}:`——**不采用工单形**：share 授权 split-once(':') 依赖 `{from}:` 形态（:1095）;改形=破坏 share 面+违反零回归 DoD | 披露（形态裁决记录在案） |
| 3 | 「既有无前缀数据一次性迁移（新增→双写→切读）,保留只读回退一版」 | docs store=**内存 HashMap**（无持久化——生命周期=进程内;seed() 仅 host 测试面）→「一次性迁移/双写」无持久存量可迁 | **落只读回退一版**（真缺口）: read-doc `{id}:x` 未命中→回退读 `x`（只读;写面仍强制前缀=不产生新 legacy 键;query 不列 legacy 键=隔离不变）;逐命中审计 reason=legacy-fallback（归零即下版可移除） |
| 4 | 「host-invoked write-doc 容量 guard 审计行缺位」 | 属实——T9 注记：guard 在 DefaultHostFunctions::call（registry 层）仅 stderr（无 plugin/审计上下文） | **修**：runtime 层 call_host_function 带上下文先行拦截（audit_log 结构化审计环+scope=host-invoked 标注）;错误码与 guest 面同形（"write_doc: payload exceeds 1 MiB"）;registry 层 guard 保留=纵深防御 |
| 5 | 台账 C-①（token-None 占位语义） | 全路径核验：init/handle-event token-None→denied+审计（:949/:986）;guest 面 table miss→granted=false→CapabilityDenied——**无 None 假装通过路径** | 台账关账（核验式收口） |

## 交付（两修+两测+双关账）
- **legacy 无前缀只读回退一版**（read_doc :1389+——回退命中审计+写面不豁免）
- **host-invoked 容量 guard 审计行**（call_host_function :1150+——与 guest 面对齐）
- 测试：`test_b07_legacy_fallback_read_only`（回退命中+审计+写拒三断言）/`test_b07_host_invoked_size_guard_audited`（audit 环 size-exceeded+scope=host-invoked+错误码）
- 台账：C-①+C-ns 双关账（锚点级）

## DoD 对照
- ✅ 插件 A 无法读 B 的 doc（E2E 断言）——**在案四枚**：e2e_dataplane_namespace_isolation/e2e_namespace_read_denied/e2e_namespace_write_denied/e2e_query_isolation（guest_e2e.rs:413/620/652/679）+本卡 lib 测写拒断言
- ✅ 容量超限有审计行+明确错误码——本卡修（host 面）+guest 面在案（e2e_dataplane_size_guard:376）
- ✅ 既有插件数据零丢失——内存 store 无持久存量;host seed/legacy 键经只读回退可达;写面零破坏
- ✅ CI plugin lib 140+e2e 22 零回归——实测 **lib 142/142（+2 新）+e2e 22/22 全绿**

## 停手边界遵守
不做 per-plugin 配额/速率限制（v2）✓ / WIT 接口零改动（本卡纯宿主侧——三 guest 不涉）✓
