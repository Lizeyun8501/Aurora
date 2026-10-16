# Bravo 交付验收：DK-模块编译缓存（8c138d8）+CI 修复四连

**验收人**: Alpha 2026-10-16 13:39 ｜ **结论**: **通过（无保留）**——127 单元+22 e2e 全绿 clippy 干净（独立复跑实证）

## 主卡：模块编译缓存——逐项核查（派单 DoD 对照）

| 项 | 判定 | 证据 |
|---|---|---|
| spike 先行 | ✅ | `Component::serialize`（safe）+`deserialize`（**unsafe**，前提=同版本+同 Engine config，不匹配可能 UB 而非 Err）——可行性成立不降级 bench |
| **unsafe 纪律** | ✅ **亮点** | 双头防线：MAGIC/tag_len 边界/engine_tag/sha256(wasm) 全验过才进 deserialize——**失败路径绝不触碰 unsafe**（UB 而非 Err 的正确应对） |
| fail-open | ✅ | KV 读 Err/头坏/漂移/serialize Err/set Err 全部降级全量编译；写失败 `let _ =` 不阻断；"缓存是优化不是正确性依赖"注记原文 |
| 形态裁决 A | ✅ 合理 | 键省 checksum+值内嵌自包含头——invoke 路径 listing.checksum 不可达，B 方案需扩散 checksum 进 invoke 签名链（越 installer 领地）；checksum 参数保留双保险口 |
| 懒回写修复 | ✅ | fail-open 编译后 stash+serialize 回写——损坏缓存自愈 |
| 容量护栏 64 | ✅ | trim_modcache 按值头 ts 删最旧（解析失败 ts=0 最旧优先——确定性 LRU 近似） |
| runtime 接线 | ✅ | `cached_components` 槽（stash/take 幂等）+get_or_instantiate 慢路径槽优先——**fail-open 主链不变**；prime 在 cmd_plugin_invoke async 面先行（spawn_blocking 前读 KV 合规） |
| 停手线 | ✅ | installer/audit/fetcher **零触碰**（git diff 5176113..8c138d8 实证） |
| 测试 | ✅ | 5 例确定性断言（首写/损坏 fail-open 修复/字节漂移 miss/护栏 64 LRU/版本漂移检出）——127/22 全绿独立复跑 |
| 注记诚实度 | ✅ | API 行号/UB 前提/残余风险（engine_tag 碰撞）如实 |

## 残余风险与建议（不阻塞）

1. **engine_tag=aurora-plugin 版本串**——wasmtime 升级需手动随动 bump（纪律依赖，无 CI 强制）。建议：tag 嵌入 wasmtime 版本常量（若 crate 导出）或 CI 加"lock 中 wasmtime 变更则提醒"断言——S 级搭车项
2. engine_tag 碰撞面（同 tag 异 config）：本 workspace Engine 单构造点（with_default_registry）——**当前碰撞面=零**；多 Engine config 出现时须复核

## CI 修复四连（1f2def3/f9c6bf4/a94e59a/b3ad87d）——质量确认

修 Alpha 审计落盘批（494e4a7/5176113）desktop 面编译错（本机无 GTK 检不出——CI 兜底连续兑现）：
- ✅ 1f2def3：audit test unused import——**领地纪律例外已披露**（一行机械修，Clippy -D warnings 挡合并）
- ✅ f9c6bf4：restore_audit `&*` 解引用（&Arc<dyn>→&dyn 签名匹配——我签名不一致引入）
- ✅ a94e59a：debug! 宏导入
- ✅ b3ad87d：manage 挂 clone 保原值供 tick 捕获（E0382——我"定义前移"方案与 clone 方案叠加=冗余但无害，Arc 浅拷贝语义安全）
- **教训吸收**：desktop 面编译错误连续三批由 Bravo CI 兜底——Alpha 后续 desktop 触碰项，push 前**必须过一遍 use 导入清单+签名对账+Clone derive 核查**（三连坑同源）

## 关账

插件系统最后一张在途卡验收通过——**插件系统按用户指令正式冻结**（挂起池已固化：S1 前缀闸/KV capability S2-S3/输出确认面/CallerCtx/审计 flush 定量/engine_tag 加固）。下一主线=审阅报告 P0（双向链接/hot_update 统一注册表/导出面），待用户拍板。
