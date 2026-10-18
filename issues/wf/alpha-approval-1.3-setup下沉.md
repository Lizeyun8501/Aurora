# alpha-approval：DK-1.3 setup 下沉——侦察批复+派单

> 批复方: Alpha ｜ 2026-10-18 ｜ 对象: 7ded281 侦察卡（窗口切片：侦察→批复→派单零等待兑现）
> **批复状态：通过——按本卡直接落实现，无需二次侦察**

## 批复要点

1. **三块下沉方案同意**（tick 定时器/混合检索装配/插件状态构建）——bootstrap 零 tauri 依赖引入红线正确；tokio::spawn 等价 tauri::async_runtime::spawn 判断正确。
2. **签名预批（跨 crate 授权）**：bootstrap 新增函数为**新增面**——预批授权 Bravo 直接落，签名以 bootstrap 现有类型面为准（BootedApp 为 bootstrap 自有类型可直接引用）；**涉 desktop 私有类型（如 PluginState）时只下沉可独立子段，剩余留壳**——以编译通过+零 tauri 引入为执行准绳，卡住即回侦察（不硬造抽象）。
3. **领地确认**：Bravo 迁 desktop 侧调用点+bootstrap 新增函数实现；app.manage 留壳（侦察②判断采纳）；static 全局锁+invoke_handler 注册表留壳。
4. **验收面同意**：check+clippy -D warnings+fmt 三件+tsc；行为级=混合检索 get_hybrid 恒等+插件 cmd 全零回归+tick 触发路径不回归（日志锚或代码走读证据）。

## 派单（Bravo 执行）

**DK-1.3 setup 巨石下沉**（S）——按侦察卡三块方案+本批复边界执行；完成后交付注记 issues/wf/bravo-1.3-setup下沉.md（含 lib.rs 行数前后对比+bootstrap 新增函数清单+行为级证据）。
