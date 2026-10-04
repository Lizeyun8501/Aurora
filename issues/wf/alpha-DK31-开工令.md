# DK-31 开工令 — wasmtime 27→36 升级评估 + TLS 链补洞（Bravo）

> 发令 Alpha 2026-10-04 · 依据：DK-30 安全审计报告 · 领地：aurora-core/aurora-plugin 插件引擎面 · 预算 1-2 人日

## 背景

cargo audit 首扫 25 条漏洞——wasmtime 27.0.0 占 18 条且 **27 线零补丁**（修复段全 ≥36.0.6；含沙箱逃逸 RUSTSEC-2026-0096/0269——现攻击面低因自有插件，纵深防御必须升）。TLS 链三件（rustls-webpki 0.101.7/rustls 0.23.43/h2）+ protobuf 均需 Cargo.toml 版本要求变更（cargo update 无痛路径不存在——已验证）。

## 卡面两任务

1. **wasmtime 27→36.0.16+ 升级评估**：API 变更清单盘点（Engine/Module/Linker/Component 面——aurora-plugin 触点）→ 升级落地 + 现有插件行为 parity 测试 + 启动/执行性能对照（bench 惯例）；**数据决策出口**：若 API 断裂面过大（盘点后改动 >3 人日量级）→ 产出迁移评估报告诚实收卡挂账（不为升级而升级）；
2. **TLS 链补洞顺带**：rustls-webpki ≥0.103.13 / rustls ≥0.23.45 / h2 ≥0.4.16 / protobuf ≥3.7.2——Cargo.toml 版本要求变更 + 全量测试回归（传递依赖兼容性——若某项引发连锁断裂则单独挂账不硬升）。

## DoD

1. wasmtime 升级落地或迁移评估报告（二选一诚实出口）；
2. 插件 parity 测试（现有插件行为一致）+ 全量 CI 四门；
3. TLS 链四项升级后 `cargo audit` 复扫——**漏洞数 25→≤7**（wasmtime 18 条清零为达标线）；
4. 领地最小补丁声明 + 教训全套继承。

— Alpha 2026-10-04
