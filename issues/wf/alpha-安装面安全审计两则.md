# 安装面安全审计两则（Alpha 2026-10-16 08:47 轮）

## 审计 A：KV 落盘加密语义（plugins:blob:/installed: 明文）

- **事实**：KVStore 实现（SqliteStorage/storage_engine）无加密层——E2EE 在笔记写路径（LocalDekVault 密封后落库），KV 为通用键值
- **判定**：`plugins:blob:{id}`=wasm 分发物（市场公开语义，非机密）；`plugins:installed:{id}`=manifest 元数据（权限声明，非机密）——**明文落盘与公开语义一致，无风险**
- **边界确认**：KV 层若未来开放插件写权限（KV capability），`plugins:` 前缀须纳入宿主保留区——当前 L1 namespace 强制前缀 `{plugin_id}:` 与 `plugins:` 不相交，**无冲突面**；KV capability 派生时把"宿主保留前缀清单"写进设计稿（挂未来单）

## 审计 B：桌面 invoke 的 epoch ticker 挂载（死循环挂死风险）

- **事实**：WasmRuntime::new 自带 epoch ticker（DK-14d.d1：epoch_interruption(true)+后台线程，Drop 置停机位 join 不留孤儿）；deadline 默认 200 ticks=10s，DK-45 policy 命令运行时可调
- **判定**：桌面 PluginState 经 `with_default_registry()`→new——**ticker 随装**，插件死循环 10s 内被 epoch 打断，`spawn_blocking` 线程不会被永久占用——**无风险，闭环成立**（policy 调优面已接桌面命令层）

## 结论

两项均审计通过，无需代码变更。审计 A 的"宿主保留前缀"挂未来 KV capability 单（设计稿前置项）。
