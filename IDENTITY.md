# IDENTITY.md — 仓库身份与版本约定

## 仓库身份
- **名称**: Aurora——本地优先（local-first）笔记与知识库应用
- **形态**: Rust workspace 核心（aurora-core 等 crates）+ Tauri 桌面端（apps/desktop）
  + 移动端 FFI（apps/mobile）+ WASM 插件运行时（wasmtime component model）
- **核心承诺**: 笔记正文端到端加密（AES-256-GCM + ML-KEM-768 后量子通道）;
  P2P 同步仅传密文;无遥测、无云端账号依赖
- **权威源三阶段与 WritePath 唯一写入入口**: 见 docs/adr/ADR-003 / ADR-004

## 版本约定
- **Rust 工具链**: 固定 1.91.0（MSRV=1.91,CI 门禁校验）
- **语义化版本**: crates 0.1.x 阶段（pre-1.0;minor=破坏性变更窗口）
- **Node 面**: workspace npm（root lockfile 单源）;Playwright 固定于根 devDeps
- **Wasm 插件**: WIT 接口冻结面——改 WIT 须同步三 guest（hello/wordcount/auditlog）
  + CI 校验,不经裁决不升接口版本

## 协作入口
- 工作规范: `AGENTS.md`（五条铁律）
- 文档锚点: README「文档索引」（索引自检: `scripts/check-doc-index.sh`,CI 门禁）
