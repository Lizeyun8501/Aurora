# hot_update 统一注册表（DK-审阅 P0-2，Alpha 2026-10-16）

**性质**: 架构债清偿（审阅报告 §1.4.1——hot_update 内存域与 installer KV 域割裂）｜ **验证**: 130 单元（+3）+clippy 干净

## 缺口

HotUpdateManager 自持内存 `installed` 表（纯会话态）——桌面安装件（`plugins:installed:`/`plugins:blob:` KV 权威域）不在表内→`preload` 报 NotFound→**升级流对桌面安装件不可用**。

## 统一语义

**权威源=installer KV 域；内存表退化为会话缓存**：

1. **`preload_from_registry(kv, id, new_version, new_blob)`**——内存表无此件→查 KV installed/blob 域自动装载（manifest.version 为当前版本）→走既有 preload 状态机；两域均无→NotFound（语义不变）
2. **`confirm_switch_persist(kv, id, canary)`**——复用 confirm_switch 状态机；**仅 Completed 落库**（RolledBack KV 不动=回滚语义）：manifest.version 更新+blob 覆盖（batch_set 单事务）；内存会话缓存同步
3. **环 0 呼应**：confirm_switch_persist=reinstall=true 的**正式升级通道**（不走六环——升级字节信任调用方；verify 接线归市场下载器 S2——v1 边界注记）

## v1 边界（诚实）

- **落库失败=内存已切换而 KV 滞后**（最终一致，下次启动按 KV 旧版本恢复——重装 preload 可再来）；不做两阶段提交
- **switch 后 runtime 组件重载**未编排（components 表仍旧字节）——归命令层卡（接 UI 时：confirm 后调 uninstall+install_component+register 组合，或新增 reload_component helper）
- 无审计行（HotUpdateManager 无 runtime 引用——审计接入随命令层卡）

## 单测（registry_tests 3 例）

- registry 装载成功（from_version=1.0.0→Pending+会话缓存生效）
- 两域均无→NotFound（"not in registry"）
- persist 落库断言（KV version=1.1.0+blob=new-blob+内存同步）
