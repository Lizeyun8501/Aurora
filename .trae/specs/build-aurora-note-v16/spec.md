# Aurora Note V15.1+V16 系统构建 Spec

## Why

Aurora Note 是一个本地优先、端到端加密的跨平台知识管理与笔记系统。当前需要从零构建完整的 V16 优化版，涵盖 6 层架构、11 大 Trait 抽象、19 大功能模块、3 端部署（Desktop/Mobile/Web），以及完整的运维可观测性体系，为研发团队提供完整的技术实现指南。

## What Changes

### 架构层
- 搭建「主三层分离 + 核心层内三子层」的双层架构体系
  - 外层主三层：视图层（React 18 + TypeScript）/ 适配层（Tauri v2 / Capacitor v8 / WASM）/ 核心层（Rust）
  - 内层子三层：L1 基础设施层 / L2 核心引擎层 / L3 领域服务层
- 实现 11 大 Trait 抽象接口：CrdtEngine、SyncTarget、VectorStore、AIProvider、Storage、PluginRuntime、AgentProtocol、CryptoProvider、KVStore、OcrProvider、SearchBackend
- 搭建事件总线（tokio::sync::broadcast）实现层间通信
- **BREAKING**：从 V15.1 经典加密升级为 V16 后量子加密（ML-KEM-768）双轨并行

### L1 基础设施层
- 集成 Loro（CRDT 引擎）、iroh（P2P 同步）、Tantivy（全文检索）、LanceDB（向量数据库）
- 集成 sqlite-vec（轻量级向量检索）、Wasmtime（WASM 运行时）、rust-crypto/ring（密码学）、PaddleOCR（OCR 引擎）

### L2 核心引擎层
- 实现事件溯源引擎、工作流引擎、权限引擎、属性引擎、查询引擎、捕获引擎

### L3 领域服务层（19 大功能模块）
- **P0**：内容编辑系统、知识网络系统、GTD 效能系统 2.0、AI 智能系统
- **P1**：安全加密系统（含后量子加密）、同步服务系统、插件管理系统、导入导出系统、素材库系统、系统设置、TodayView、OCR 服务
- **P2**：AgentGateway、ExternalSyncHub、CaptureMatrix

### 视图层与适配层
- 实现 React 18 组件化架构（components/blocks/editors/layouts/views/hooks/stores/adapters）
- 实现 Tauri v2（Desktop）、Capacitor v8（Mobile）、WASM（Web）三端适配

### 运维可观测性（PART VI）
- 实现日志系统（tracing）、指标系统（prometheus）、分布式追踪（OpenTelemetry）、崩溃报告（Sentry）
- 实现监控告警体系（健康检查、告警规则、智能降噪、监控仪表板）
- 实现测试质量保障（单元/集成/E2E/CRDT 一致性/性能基线/覆盖率管控）
- 实现灰度发布与回滚（灰度更新、功能开关、热修复、版本回滚）
- 实现日志诊断与排障（诊断包导出、自助修复工具、远程协助、知识库）

## Impact
- Affected specs: 全新项目，无既有 spec 受影响
- Affected code: 全新代码库，核心结构如下：
  - `crates/aurora-core/` — 核心层 Rust crate（L1/L2/L3）
  - `crates/aurora-sync/` — 同步服务
  - `crates/aurora-ai/` — AI 智能系统
  - `crates/aurora-security/` — 安全加密系统
  - `crates/aurora-plugin/` — 插件系统
  - `apps/desktop/` — Tauri 桌面端
  - `apps/mobile/` — Capacitor 移动端
  - `apps/web/` — Web/PWA
  - `apps/extension/` — 浏览器扩展
  - `shared/types/` — 共享 TypeScript 类型
  - `shared/ui-components/` — 共享 React 组件

## ADDED Requirements

### Requirement: 双层架构体系
系统 SHALL 采用「主三层分离 + 核心层内三子层」的双层架构，外层主三层负责跨端解耦，内层子三层负责业务复杂度隔离。

#### Scenario: 跨层调用约束
- **WHEN** 视图层需要访问业务逻辑
- **THEN** 必须通过适配层 → L3 领域服务层暴露的接口访问，严禁直接访问 L1/L2 层

#### Scenario: 核心层复用
- **WHEN** 在不同平台编译核心层
- **THEN** 核心层代码 100% 复用，视图层 94% 复用，适配层按平台差异化实现

### Requirement: 十一大 Trait 抽象
系统 SHALL 定义 11 个核心 Trait 接口（V19 §28 原始 7 个 + V16 扩展 4 个），所有 L3 领域服务通过 Trait 与 L2 引擎通信。V19 §28 原始指定 `async_trait`，本批次 PR 推进异步化迁移。

#### Scenario: Trait 实现
- **WHEN** L3 领域服务需要 CRDT 操作能力
- **THEN** 通过 CrdtEngine Trait 调用，底层实现为 LoroCrdtEngine，支持运行时替换

### Requirement: 本地优先架构
系统 SHALL 保证所有写操作直接写入本地存储，离线状态下功能无差异，云端仅作为同步备份通道。

#### Scenario: 离线写入
- **WHEN** 用户在无网络环境下编辑文档
- **THEN** 操作直接写入本地 CRDT 文档和 SQLite，网络恢复后自动同步

### Requirement: 端到端加密（E2EE by Default）
系统 SHALL 从 V16 起默认强制开启端到端加密，服务器零知识，密钥由用户本地生成与保管。

#### Scenario: 数据加密存储
- **WHEN** 用户数据写入本地存储
- **THEN** 明文 → DEK 加密（AES-256-GCM）→ 存储密文，同步数据也为密文

#### Scenario: 后量子加密
- **WHEN** V16 密钥交换发生
- **THEN** 使用 ML-KEM-768 与 X25519 双轨并行，最终密钥 = KDF(X25519_shared || ML-KEM_shared)

### Requirement: 块级原子化
系统 SHALL 以块为最小内容单位，每个块是独立的 CRDT 文档，支持多态转换与属性扩展。

#### Scenario: 块多态转换
- **WHEN** 用户将文本块转换为代码块
- **THEN** 通过块类型注册表的 toJSON/fromJSON 转换器实现一键转换，保留块 ID

### Requirement: 事件溯源
系统 SHALL 将所有用户操作记录为不可变事件序列，每 1000 个事件自动生成快照。

#### Scenario: 事件回放
- **WHEN** 系统启动
- **THEN** 加载最新快照 + 增量事件重建文档状态

### Requirement: 三端部署
系统 SHALL 支持 Desktop（Tauri v2）、Mobile（Capacitor v8）、Web（WASM）三端部署。

#### Scenario: 模块部署矩阵
- **WHEN** 在 Web 端使用 AI 智能功能
- **THEN** 仅支持云端 API 推理（本地推理需 GPU，Web 端受限）

### Requirement: 模块间数据交互
系统 SHALL 通过事件总线实现模块间松耦合通信，遵循「单源真理」原则——本地 CRDT 文档是唯一数据源。

#### Scenario: 内容编辑与知识网络交互
- **WHEN** 用户编辑文档内容
- **THEN** 内容编辑模块发布 BlockChanged 事件 → 知识网络模块订阅并解析链接 → 更新 link 表 → 反向推送 BacklinksUpdated 事件

### Requirement: 可观测性体系
系统 SHALL 构建日志、指标、追踪三大可观测性支柱，支持全链路诊断。

#### Scenario: 分布式追踪
- **WHEN** 用户执行同步操作
- **THEN** 追踪链路覆盖 UI 点击 → 适配层 → L3 服务 → L2 引擎 → L1 基础设施 → 外部 API

### Requirement: 灰度发布与回滚
系统 SHALL 支持灰度更新（1% → 5% → 20% → 50% → 100%）、功能开关、热修复和紧急回滚。

#### Scenario: 自动回滚
- **WHEN** 新版本连续崩溃 3 次
- **THEN** 自动回滚至上一稳定版本并上报诊断信息

## MODIFIED Requirements

### Requirement: AI 智能系统（V15.1 → V16）
V15.1 仅支持云端 API 推理。V16 升级为本地 + 云端混合推理架构，Desktop 端优先使用 llama.cpp/Ollama 本地推理，Mobile/Web 端使用云端 API，通过 AIProvider Trait 抽象，运行时根据设备性能和网络状态自动选择 Provider。

### Requirement: GTD 效能系统（V15.1 → V16）
V15.1 仅支持基础任务管理。V16 升级为完整 GTD 2.0，新增收件箱工作流、重复任务（RRULE）、习惯追踪（链式习惯模型）、时间线视图、自动化规则（IFTTT 模式）。

### Requirement: 知识网络系统（V15.1 → V16）
V15.1 仅支持双链 + 简单图谱。V16 升级为 WebGL 图谱渲染 + 关系属性（支持为链接附加语义标签）+ 图谱探索（BFS 遍历 + 向量相似度推荐 + 路径发现）。

### Requirement: 安全加密系统（V15.1 → V16）
V15.1 使用经典加密。V16 新增后量子加密（ML-KEM-768）双轨并行、生物识别保护（FaceID/TouchID/Windows Hello + TPM）、密钥恢复（BIP39 助记词 + Shamir 秘密共享 3 选 2）。

### Requirement: 插件系统（V15.1 → V16）
V15.1 仅支持 iframe 插件。V16 升级为 WASM + iframe 双模式架构，WASM 插件通过 Wasmtime 运行时执行，支持 WASI 接口和能力控制（Capability-based Security）。

### Requirement: OCR 服务（V15.1 → V16）
V15.1 仅支持基础文字识别。V16 新增表格识别（PP-Structure）、公式识别（Texify）、批量 OCR（tokio 线程池并行）。

## REMOVED Requirements
无。这是一个全新项目的构建，不涉及移除既有功能。
