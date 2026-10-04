# DK-30 — 依赖安全审计首扫报告（Alpha 自立卡）

> 2026-10-04 · cargo-audit v0.22.2 + npm audit · 首次全量依赖安全基线 · commit 0b339bd 基准

## 一、cargo workspace：25 条漏洞（advisory 级）

| crate | 版本 | 条数 | 修复段 | 实际风险评估（桌面单用户+自有插件攻击面） |
|---|---|---|---|---|
| **wasmtime** | 27.0.0 | **18** | **27 线无补丁**——全段 ≥36.0.6+（24 线末班 24.0.13 不可达） | 低危（漏洞类型=guest→host 逃逸/panic/池间泄漏，**触发前提=执行恶意 wasm**；现状自有插件无第三方市场）；**纵深防御仍应升**——沙箱逃逸类（RUSTSEC-2026-0096/0269）一旦插件生态开放即高危 |
| rustls-webpki | 0.101.7 | 3 | ≥0.103.13 | 低-中（CRL 解析 panic/名称约束误收——TLS 链传递依赖） |
| h2 | 0.3.27 / 0.4.15 | 2 | ≥0.4.16 | 低（空 DATA 帧 DoS——客户端场景被动） |
| rustls | 0.23.43 | 1 | ≥0.23.45 | 低（TLS 1.3 握手边界） |
| protobuf | 2.28.0 | 1 | ≥3.7.2 | 低-中（递归 crash——需不受信输入解析） |

**关键结论**：wasmtime 27 线零补丁——**唯一修复路径 = 27→36 跨 9 个 minor**（API 变动+插件兼容回归——非无痛升级）。

## 二、npm（apps/desktop）：6 条——运行时零暴露

| 包 | severity | 性质 |
|---|---|---|
| vitest（critical） | 间接（@vitest/mocker 路径穿越） | **devDependencies 链——不进 dist 产物** |
| vite（high）/ vite-node（moderate） | 构建工具 | 同上 |
| nanoid（high） | size=0 死循环 | dev 链 |
| esbuild（moderate） | dev server 请求面 | 仅开发期 |

**结论**：前端漏洞全在 devDependencies 构建/测试链——**产物运行时零暴露**——常规升级排期即可，无紧急项。

## 三、处置建议（Alpha 裁决）

1. **DK-31（派 Bravo，1-2 人日）：wasmtime 27→36 升级评估卡**——插件引擎跨版本升级：API 变更清单盘点 + aurora-plugin/aurora-core 回归（现有插件行为 parity 测试）+ 性能对照（启动/执行——bench 惯例）；**数据决策**：若 API 断裂面过大则产出迁移评估报告挂账（不为升级而升级）；
2. **随 DK-31 顺带**：rustls-webpki/rustls/h2/protobuf 的 Cargo.toml 版本要求变更（全部需跨 minor——cargo update 无痛路径不存在，已验证）；
3. **npm**：常规 `npm audit fix` 排期（dev 链，低优先，不立卡）；
4. **流程**：cargo audit 建议入 CI 周检（与 bench.yml 同窗口——待 DK-31 后评估告警噪声再定）。

## 四、环境注记

cargo-audit 安装踩坑：apps/desktop 子目录无 rust-toolchain.toml → rustup default 指向损坏的 1.88.0（missing manifest）——**repo 根（钉 1.91）执行可解**；损坏的 1.88.0 toolchain 建议清理（`rustup toolchain uninstall 1.88.0`，挂下次）。

— Alpha 2026-10-04
