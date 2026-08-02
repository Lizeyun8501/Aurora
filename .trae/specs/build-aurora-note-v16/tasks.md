# Tasks

## Phase 0: 项目初始化与架构骨架

- [x] Task 0.1: 初始化 Monorepo 项目结构
  - [x] SubTask 0.1.1: 创建 `crates/aurora-core/`、`crates/aurora-sync/`、`crates/aurora-ai/`、`crates/aurora-security/`、`crates/aurora-plugin/` Rust crate 骨架
  - [x] SubTask 0.1.2: 创建 `apps/desktop/`（Tauri v2）、`apps/mobile/`（Capacitor v8）、`apps/web/`（Vite + WASM）、`apps/extension/` 目录骨架
  - [x] SubTask 0.1.3: 创建 `shared/types/`（共享 TypeScript 类型）和 `shared/ui-components/`（共享 React 组件）目录骨架
  - [x] SubTask 0.1.4: 配置根 `Cargo.toml` workspace 和根 `package.json` workspace
  - [x] SubTask 0.1.5: 配置 rustfmt、clippy、EditorConfig 代码规范工具链

- [x] Task 0.2: 搭建核心层 L1 基础设施层基础
  - [x] SubTask 0.2.1: 集成 Loro CRDT 引擎 crate 依赖
  - [x] SubTask 0.2.2: 集成 iroh P2P 同步库依赖
  - [x] SubTask 0.2.3: 集成 Tantivy 全文检索库依赖
  - [x] SubTask 0.2.4: 集成 LanceDB + sqlite-vec 向量存储依赖
  - [x] SubTask 0.2.5: 集成 Wasmtime WASM 运行时依赖
  - [x] SubTask 0.2.6: 集成 rust-crypto/ring 密码学库依赖
  - [x] SubTask 0.2.7: 集成 PaddleOCR / Tesseract OCR 引擎依赖（FFI 绑定）

- [x] Task 0.3: 定义七大 Trait 抽象接口
  - [x] SubTask 0.3.1: 定义 `CrdtEngine` Trait（create_document, apply_ops, get_snapshot, get_history, merge_branch）
  - [x] SubTask 0.3.2: 定义 `SyncTarget` Trait（connect, sync, watch, disconnect）
  - [x] SubTask 0.3.3: 定义 `VectorStore` Trait（add, search, delete, hybrid_search）
  - [x] SubTask 0.3.4: 定义 `AIProvider` Trait（embed, complete, stream_complete, chat, function_call）
  - [x] SubTask 0.3.5: 定义 `Storage` Trait（get, put, delete, query, transaction）
  - [x] SubTask 0.3.6: 定义 `PluginRuntime` Trait（load, invoke, unload, list_hooks）
  - [x] SubTask 0.3.7: 定义 `AgentProtocol` Trait（register_tool, execute, subscribe, get_context）

- [x] Task 0.4: 实现事件总线与层间通信
  - [x] SubTask 0.4.1: 定义 `CoreEvent` 枚举（DocumentChanged, SyncProgress, TaskDue, AIGenerationComplete, PermissionChanged, PluginLoaded）
  - [x] SubTask 0.4.2: 实现 `EventBus`（基于 tokio::sync::broadcast，支持多消费者 subscribe/publish）
  - [x] SubTask 0.4.3: 定义层间通信数据序列化规范（核心层内 bincode、跨层 JSON、网络 protobuf/MessagePack）

## Phase 1: L2 核心引擎层

- [x] Task 1.1: 实现事件溯源引擎
  - [x] SubTask 1.1.1: 定义 Event 结构（event_id, block_id, op_type, payload, timestamp, user_id, device_id, signature）
  - [x] SubTask 1.1.2: 实现聚合根（DocumentAggregate, BlockAggregate, WorkspaceAggregate）
  - [x] SubTask 1.1.3: 实现快照策略（每 1000 个事件自动生成快照，启动加载最新快照 + 增量事件）
  - [x] SubTask 1.1.4: 实现 SQLite WAL 模式事件存储，按 workspace 分库

- [x] Task 1.2: 实现工作流引擎
  - [x] SubTask 1.2.1: 实现状态机 DSL（基于 serde_json 描述状态节点与迁移条件）
  - [x] SubTask 1.2.2: 实现触发器（时间触发、事件触发、API 触发）
  - [x] SubTask 1.2.3: 实现异步任务执行器（tokio::sync::mpsc，支持重试与死信队列）

- [x] Task 1.3: 实现权限引擎
  - [x] SubTask 1.3.1: 实现 RBAC 五级角色（Owner/Admin/Editor/Commenter/Viewer）
  - [x] SubTask 1.3.2: 实现 ABAC 属性条件扩展（如"仅工作日可编辑"、"仅特定 IP 可访问"）
  - [x] SubTask 1.3.3: 实现权限传播（Workspace → Collection → Document → Block 四级层级继承）

- [x] Task 1.4: 实现属性引擎
  - [x] SubTask 1.4.1: 实现基础类型系统（Text/Number/Date/Checkbox/Select/MultiSelect/Relation/Formula）
  - [x] SubTask 1.4.2: 实现基于 JSON Schema 的运行时类型校验
  - [x] SubTask 1.4.3: 实现索引策略（热点属性自动建立 SQLite 索引，冷属性按需查询）

- [x] Task 1.5: 实现查询引擎
  - [x] SubTask 1.5.1: 实现统一 Query DSL（JSON 格式，支持过滤/排序/分页/聚合）
  - [x] SubTask 1.5.2: 实现基于成本的查询优化器（自动选择 Tantivy/LanceDB/SQLite 执行路径）
  - [x] SubTask 1.5.3: 实现 LRU 缓存层（TTL 基于数据变更频率动态调整）

- [x] Task 1.6: 实现捕获引擎
  - [x] SubTask 1.6.1: 实现管道架构（Source → Parser → Enricher → Normalizer → Storage）
  - [x] SubTask 1.6.2: 实现来源插件接口（浏览器扩展、系统分享、邮件 IMAP、API Webhook）
  - [x] SubTask 1.6.3: 实现去重策略（SimHash 内容哈希 + URL 指纹双重去重）

- [x] Task 1.7: 实现 L1 基础设施 Trait 实现
  - [x] SubTask 1.7.1: 实现 LoroCrdtEngine（基于 loro crate）
  - [x] SubTask 1.7.2: 实现 IrohSyncTarget、WebSocketSyncTarget、LanSyncTarget
  - [x] SubTask 1.7.3: 实现 LanceDbStore、SqliteVecStore
  - [x] SubTask 1.7.4: 实现 SqliteStorage、SledStorage、S3Storage
  - [x] SubTask 1.7.5: 实现 WasmtimeRuntime（WASM）、IframeRuntime（Web）
  - [x] SubTask 1.7.6: 实现 McpAgentProtocol、NativeAgentProtocol

## Phase 2: L3 领域服务层 — P0 模块

- [x] Task 2.1: 实现内容编辑系统
  - [x] SubTask 2.1.1: 实现块级文档模型（Block 结构：id, block_type, content, properties, children；Document 结构）
  - [x] SubTask 2.1.2: 实现块类型注册表（text/heading/code/image/table/divider/quote/list_item/todo_item，支持插件注册自定义类型）
  - [x] SubTask 2.1.3: 实现富文本编辑核心（TipTap + ProseMirror Schema，pmToLoro/loroToPm 双向转换层）
  - [x] SubTask 2.1.4: 实现 IME 合成期间暂停同步（compositionend 后批量提交）
  - [x] SubTask 2.1.5: 实现 Markdown 支持（InputRule 实时转换 + 自定义序列化器导出 CommonMark）
  - [x] SubTask 2.1.6: 实现协作编辑（基于 Loro 自动合并 + Awareness 协议协作光标）
  - [x] SubTask 2.1.7: 实现版本历史（Loro Version Vector + 快照+增量存储 + 时间旅行 checkout）
  - [x] SubTask 2.1.8: 实现评论批注（独立 Comment 模型 + 基于内容的锚定 + ProseMirror Decoration 渲染）

- [x] Task 2.2: 实现知识网络系统
  - [x] SubTask 2.2.1: 实现双链引用（WikiLink `[[标题]]` 和 MarkdownLink 解析，SQLite links 表索引）
  - [x] SubTask 2.2.2: 实现链接索引异步更新（100ms debounce，监听 DocumentChanged 事件）
  - [x] SubTask 2.2.3: 实现反向链接面板（反向查询 + 上下文预览 + 实时更新）
  - [x] SubTask 2.2.4: 实现知识图谱可视化（D3.js 力导向图 + WebGL 加速 >1000 节点 + 聚类着色）
  - [x] SubTask 2.2.5: 实现关系属性（语义标签：支持/反驳/引用/扩展 + Property Engine Relation 类型）
  - [x] SubTask 2.2.6: 实现图谱探索（BFS 遍历 + LanceDB 相似度推荐 + 最短路径发现 + 探索历史记录）

- [x] Task 2.3: 实现 GTD 效能系统 2.0
  - [x] SubTask 2.3.1: 实现 GTD 工作流引擎（状态机：Inbox → Clarified → Organized → Scheduled → Doing → Done | Archived）
  - [x] SubTask 2.3.2: 实现项目/任务无限层级嵌套（closure table 模式存储层级关系）
  - [x] SubTask 2.3.3: 实现收件箱（快速创建 + 批量 Clarify + CaptureMatrix 内容默认入口）
  - [x] SubTask 2.3.4: 实现重复任务（RRULE 库解析 iCalendar 标准 + 模板/实例分离模型）
  - [x] SubTask 2.3.5: 实现提醒系统（tokio::time 定时器 + Desktop 系统通知 + Mobile APNs/FCM 推送）
  - [x] SubTask 2.3.6: 实现习惯追踪（链式习惯模型 + streak 指标 + 热力图 + 习惯与任务联动）
  - [x] SubTask 2.3.7: 实现时间线视图（日/周/月粒度 + 拖拽调整 due_date + 日历事件集成）
  - [x] SubTask 2.3.8: 实现自动化规则（IFTTT 模式：When [Trigger] And [Condition] Then [Action]）

- [x] Task 2.4: 实现 AI 智能系统
  - [x] SubTask 2.4.1: 实现混合推理架构（AIProviderRouter：LocalFirst / CloudOnly / Auto 策略 + 运行时选择）
  - [x] SubTask 2.4.2: 实现 AIProvider 实现（LlamaCppProvider 本地、OpenAIProvider 云端、OllamaProvider 本地 API）
  - [x] SubTask 2.4.3: 实现智能续写（debounce 500ms + 流式 stream_complete + ghost text 显示 + Tab 接受/Esc 取消）
  - [x] SubTask 2.4.4: 实现内容摘要（三级粒度：段落/文档/多文档 + Map-Reduce 策略 + SQLite 缓存关联版本号）
  - [x] SubTask 2.4.5: 实现问答对话（RAG 架构：Embedding → LanceDB 检索 → 上下文拼接 → LLM 生成）
  - [x] SubTask 2.4.6: 实现混合搜索（Tantivy 关键词 Top-20 + LanceDB 向量 Top-20 + Cross-Encoder 重排序 Top-5）
  - [x] SubTask 2.4.7: 实现语义搜索（all-MiniLM-L6-v2 本地 Embedding + LanceDB IVF_PQ 索引 + RRF 融合排序）
  - [x] SubTask 2.4.8: 实现自动标签（TF-IDF + TextRank 关键词提取 + LLM 候选标签 + 用户确认）
  - [x] SubTask 2.4.9: 实现任务分解（LLM 生成子任务列表 + 预览编辑 + 批量创建关联父任务）

## Phase 3: L3 领域服务层 — P1 模块

- [x] Task 3.1: 实现安全加密系统
  - [x] SubTask 3.1.1: 实现密钥层次结构（用户密码 → Argon2id 派生 Master Key → 每 Workspace 独立 DEK）
  - [x] SubTask 3.1.2: 实现端到端加密（明文 → DEK AES-256-GCM 加密 → 存储/同步密文 + 零知识架构）
  - [x] SubTask 3.1.3: 实现后量子加密（ML-KEM-768 与 X25519 双轨并行 + 混合密钥交换 + 算法迁移支持）
  - [x] SubTask 3.1.4: 实现生物识别保护（Mobile FaceID/TouchID + Desktop TPM/Secure Enclave + 有效期配置）
  - [x] SubTask 3.1.5: 实现密钥恢复（BIP39 助记词 + Shamir 秘密共享 3 选 2 + 设备授权 QR 码）

- [x] Task 3.2: 实现同步服务系统
  - [x] SubTask 3.2.1: 实现 P2P 同步（iroh + QUIC + NAT 穿透 + Loro Sync Protocol 版本向量交换）
  - [x] SubTask 3.2.2: 实现云端同步（WebSocket 实时推送 + HTTPS 批量 + 密文中继 + 不可读存储）
  - [x] SubTask 3.2.3: 实现局域网同步（mDNS 自动发现 + 局域网 IP 直连 QUIC + 优先于云端）
  - [x] SubTask 3.2.4: 实现冲突解决（CRDT 自动合并 + 语义冲突 UI 手动选择 + 分支模式）
  - [x] SubTask 3.2.5: 实现增量同步（CRDT ops 增量 + 媒体文件 rsync-like 块级增量 + zstd 压缩）
  - [x] SubTask 3.2.6: 实现离线队列（SQLite sync_queue 表 + 优先级 + 幂等性 + 批量压缩）
  - [x] SubTask 3.2.7: 实现多设备管理（Device ID Ed25519 + QR 码授权 + 远程撤销 + DEK 失效）

- [x] Task 3.3: 实现插件管理系统
  - [x] SubTask 3.3.1: 实现双模式架构（WASM 模式 + iframe 模式 + 插件生命周期管理）
  - [x] SubTask 3.3.2: 实现 WASM 插件运行时（Wasmtime + WASI preview1 + 宿主函数 + Capability 清单控制）
  - [x] SubTask 3.3.3: 实现 iframe 插件隔离（sandbox iframe + postMessage JSON-RPC 2.0 + CSS 变量透传）
  - [x] SubTask 3.3.4: 实现插件权限控制（最小权限原则 + 运行时权限检查 + 动态升级）
  - [x] SubTask 3.3.5: 实现插件市场（去中心化 + 官方/第三方/本地三种来源 + 代码签名 + 更新检查）
  - [x] SubTask 3.3.6: 实现热更新（后台预加载 + 用户确认切换 + 失败回滚 + 灰度发布）

- [x] Task 3.4: 实现导入导出系统
  - [x] SubTask 3.4.1: 实现 Markdown 导入（pulldown-cmark 解析 → 中间 AST → ProseMirror 文档树 → Loro CRDT + YAML frontmatter）
  - [x] SubTask 3.4.2: 实现 Notion 迁移（Notion API 导出 + 块类型映射 + 数据库映射 Collection + 富文本注释映射 mark 系统）
  - [x] SubTask 3.4.3: 实现 PDF 导出（浏览器打印 headless Chrome + 自定义模板 + 批量导出）
  - [x] SubTask 3.4.4: 实现批量导入（Zip 包 + manifest.json + 断点续传）

- [x] Task 3.5: 实现素材库系统
  - [x] SubTask 3.5.1: 实现内容寻址存储（SHA-256 哈希命名 + 天然去重 + SQLite assets 表元数据）
  - [x] SubTask 3.5.2: 实现缩略图生成（image crate 异步 + ffmpeg 视频关键帧 + WebP 优先 + 按需延迟生成）
  - [x] SubTask 3.5.3: 实现 EXIF 元数据解析（kamadak-exif + 拍摄时间/GPS/设备信息 + JSON 存储）
  - [x] SubTask 3.5.4: 实现重复检测（精确 SHA-256 + 感知哈希 pHash / CNN Embedding 相似检测）

- [x] Task 3.6: 实现系统设置
  - [x] SubTask 3.6.1: 实现设置分层存储（系统级 → 用户级 → Workspace 级 + serde 强类型 + 版本迁移）
  - [x] SubTask 3.6.2: 实现主题系统（CSS Variables 设计令牌 + Light/Dark/Sepia/HighContrast + Auto 跟随系统）
  - [x] SubTask 3.6.3: 实现快捷键配置（全局快捷键 + 编辑器快捷键 + 冲突检测 + 平台适配 Ctrl/Cmd 映射）

- [x] Task 3.7: 实现 TodayView 今日视图
  - [x] SubTask 3.7.1: 实现聚合视图架构（QueryEngine 预计算查询 + Zustand 缓存 + EventBus 增量更新）
  - [x] SubTask 3.7.2: 实现时间线视图（日/周/月粒度 + 虚拟列表 react-window + 拖拽回写 due_date）
  - [x] SubTask 3.7.3: 实现专注模式（Pomodoro tokio::time 倒计时 + 25min/5min 循环 + 白噪音 + 专注统计）
  - [x] SubTask 3.7.4: 实现每日回顾（自动生成当日报告 + 任务完成率/时间分配/习惯连续性 + 历史浏览）

- [x] Task 3.8: 实现 OCR 服务
  - [x] SubTask 3.8.1: 实现双引擎 OCR（PaddleOCR 主引擎中文 + Tesseract fallback 英文 + OCRProvider Trait 抽象）
  - [x] SubTask 3.8.2: 实现图像预处理流水线（去噪 → 二值化 → 倾斜校正 → 版面分析）
  - [x] SubTask 3.8.3: 实现表格识别（PP-Structure 版面分析 + 单元格独立 OCR + Markdown 表格重组）
  - [x] SubTask 3.8.4: 实现公式识别（Texify 模型 + LaTeX 代码输出 + MathBlock 插入文档）
  - [x] SubTask 3.8.5: 实现批量 OCR（tokio 线程池并行 + EventBus 进度推送 + 结果关联素材元数据）

## Phase 4: L3 领域服务层 — P2 模块

- [x] Task 4.1: 实现 AgentGateway 智能体网关
  - [x] SubTask 4.1.1: 实现 MCP 协议（JSON-RPC 2.0 + initialize/tools.list/tools.call/resources.list + stdio/SSE 传输）
  - [x] SubTask 4.1.2: 实现工具注册与发现（AgentProtocol Trait + ToolRegistry 聚合 + 动态发现）
  - [x] SubTask 4.1.3: 实现 Agent 编排（Sequential/Parallel/Hierarchical 三种模式 + LLM 生成执行计划 + 可视化）
  - [x] SubTask 4.1.4: 实现上下文持久化（Context 对象 + SQLite 存储 + 会话恢复 + 窗口压缩）
  - [x] SubTask 4.1.5: 实现安全沙箱（权限引擎校验 + 审计日志 + 只读模式）

- [x] Task 4.2: 实现 ExternalSyncHub 外部同步中心
  - [x] SubTask 4.2.1: 实现连接器架构（SyncConnector Trait + 连接器注册表 + 状态管理）
  - [x] SubTask 4.2.2: 实现日历同步（CalDAV RFC 4791 + GTD 任务截止日期映射 + 增量 CTag/ETag + 冲突处理）
  - [x] SubTask 4.2.3: 实现邮件同步（IMAP + 邮件捕获为文档 + 附件提取至素材库 + 过滤规则）
  - [x] SubTask 4.2.4: 实现云盘同步（WebDAV + Google Drive/Dropbox/OneDrive API + 选择性同步 + 双向同步）
  - [x] SubTask 4.2.5: 实现 Webhook 接收（本地 HTTP 服务器 + GitHub/Jira/Slack 来源 + HMAC-SHA256 签名验证）

- [x] Task 4.3: 实现 CaptureMatrix 捕获矩阵
  - [x] SubTask 4.3.1: 实现网页剪藏（浏览器扩展 + Readability 算法提取正文 + Markdown 转换 + Message Passing）
  - [x] SubTask 4.3.2: 实现截图 OCR（全局快捷键 + 系统截图 API + OCR 识别 + 悬浮窗 + 截图即笔记工作流）
  - [x] SubTask 4.3.3: 实现语音速记（Whisper.cpp 本地 STT / 云端 API + WebRTC getUserMedia + 实时转写插入文档）
  - [x] SubTask 4.3.4: 实现 RSS 订阅（feed-rs 解析 + 15 分钟轮询 + 新文章转文档 + 全文抓取）

## Phase 5: 视图层与适配层

- [ ] Task 5.1: 搭建视图层基础架构
  - [ ] SubTask 5.1.1: 初始化 React 18 + TypeScript 项目（严格模式 ≥5.0）
  - [ ] SubTask 5.1.2: 集成 TipTap/ProseMirror v2+ 富文本编辑器
  - [ ] SubTask 5.1.3: 集成 Zustand v4+ 状态管理 + TanStack Query v5+ 服务端状态
  - [ ] SubTask 5.4.4: 集成 React DnD v16+ 拖拽 + Framer Motion v10+ 动画
  - [ ] SubTask 5.1.5: 创建目录结构（components/blocks/editors/layouts/views/hooks/stores/adapters）

- [ ] Task 5.2: 实现视图层核心组件
  - [ ] SubTask 5.2.1: 实现块级渲染器（TextBlock/CodeBlock/ImageBlock/TableBlock 等）
  - [ ] SubTask 5.2.2: 实现编辑器外壳（DocumentEditor/CanvasEditor）
  - [ ] SubTask 5.2.3: 实现布局组件（Sidebar/SplitPane/Modal）
  - [ ] SubTask 5.2.4: 实现页面级视图（TodayView/GraphView/SettingsView）
  - [ ] SubTask 5.2.5: 实现自定义 Hooks（useBlock/useSync/useAI）
  - [ ] SubTask 5.2.6: 实现 Zustand 状态定义（stores/）

- [ ] Task 5.3: 实现适配层
  - [ ] SubTask 5.3.1: 定义 PlatformAPI 统一接口（readFile/writeFile/httpRequest/showNotification/generateKey/encrypt/authenticateBiometric）
  - [ ] SubTask 5.3.2: 实现 Tauri v2 适配（Desktop：系统托盘、全局快捷键、文件系统、原生菜单 + IPC）
  - [ ] SubTask 5.3.3: 实现 Capacitor v8 适配（Mobile：推送通知、相机、生物识别、离线存储 + Bridge Call）
  - [ ] SubTask 5.3.4: 实现 WASM 适配（Web：Rust 编译 WASM32 + PWA Service Worker + IndexedDB + OPFS）

- [ ] Task 5.4: 实现模块间数据交互视图
  - [ ] SubTask 5.4.1: 实现内容编辑 ↔ 知识网络交互（BlockChanged → 链接解析 → BacklinksUpdated → UI 更新）
  - [ ] SubTask 5.4.2: 实现 GTD ↔ 内容编辑交互（CreateTask → TaskBlock 嵌入 → TaskUpdated → 计数更新）
  - [ ] SubTask 5.4.3: 实现 AI ↔ 内容编辑 ↔ 知识网络交互（AIComplete 流式 + SemanticSearch 图谱高亮）
  - [ ] SubTask 5.4.4: 实现同步服务横切关注点（监听所有模块 EventBus → 本地队列 → P2P/云端/局域网同步）

## Phase 6: 运维可观测性体系（PART VI）

- [ ] Task 6.1: 实现可观测性架构
  - [ ] SubTask 6.1.1: 实现日志系统（tracing crate 结构化日志 + span 上下文 + 5 级日志 + 文件 JSON 旋转归档 + 采样 + 脱敏）
  - [ ] SubTask 6.1.2: 实现指标系统（prometheus crate + /metrics 端点 + 业务/性能/资源/质量四类指标 + 标签切片）
  - [ ] SubTask 6.1.3: 实现分布式追踪（OpenTelemetry Rust SDK + EventBus/FFI/HTTP 上下文传播 + 尾采样 + OTLP 导出）
  - [ ] SubTask 6.1.4: 实现崩溃报告（Sentry SDK + panic 捕获 + 堆栈/设备/版本/日志 breadcrumbs + 加密场景脱敏）

- [ ] Task 6.2: 实现监控告警设计
  - [ ] SubTask 6.2.1: 实现健康检查体系（/health 端点 + 启动自检 + 60 秒周期巡检 + 状态栏指示器）
  - [ ] SubTask 6.2.2: 实现告警规则（P0 紧急/P1 重要/P2 提示三层 + 本地通知 + 云端 Webhook 推送）
  - [ ] SubTask 6.2.3: 实现智能降噪（告警聚合 + 静默期 + 依赖抑制 + 自愈检测）
  - [ ] SubTask 6.2.4: 实现监控仪表板（Grafana 预置模板 + Desktop 内嵌轻量面板 + 关键视图看板）

- [ ] Task 6.3: 实现测试质量保障
  - [ ] SubTask 6.3.1: 实现单元测试与集成测试（cargo test + vitest + Mock L1 实现 + proptest 属性测试）
  - [ ] SubTask 6.3.2: 实现 CRDT 一致性测试（猴子测试随机操作序列 + delta-debugging 反例压缩 + 24 小时稳定性测试）
  - [ ] SubTask 6.3.3: 实现 E2E 测试（Playwright 跨端 + 关键场景覆盖 + 视觉回归截图对比 + 崩溃恢复测试）
  - [ ] SubTask 6.3.4: 实现性能基线与回归（criterion.rs 基线 + CI 10% 退化阈值 + k6 加载测试 + 内存分析）
  - [ ] SubTask 6.3.5: 实现覆盖率管控（tarpaulin Rust ≥70%/核心 ≥80% + vitest TS ≥60% + Codecov PR 评论 + 豁免机制）

- [ ] Task 6.4: 实现灰度发布与回滚
  - [ ] SubTask 6.4.1: 实现灰度更新策略（Tauri updater + Capacitor Appflow + 1%→5%→20%→50%→100% + 紧急制动）
  - [ ] SubTask 6.4.2: 实现功能开关（FeatureFlags + Boolean/Percentage/Targeting + 本地 SQLite + 离线生效 + 核心功能不设开关）
  - [ ] SubTask 6.4.3: 实现热修复（Web CDN 实时 + Desktop delta update + Mobile Appflow + 限制仅视图层/适配层）
  - [ ] SubTask 6.4.4: 实现版本回滚（保留最近 3 版本 + 崩溃 3 次自动回滚 + 数据格式版本检测 + 回滚触发条件）

- [ ] Task 6.5: 实现日志诊断与排障
  - [ ] SubTask 6.5.1: 实现诊断包导出（ZIP 格式 + 7 天脱敏日志 + 配置/指标/健康/设备/同步状态 + 加密 + 50MB 限制）
  - [ ] SubTask 6.5.2: 实现自助修复工具（索引重建 + 缓存清理 + 同步重置 + 权限修复 + 配置重置 + 修复前备份）
  - [ ] SubTask 6.5.3: 实现远程协助（企业版：一次性会话码 + 安全通道实时日志 + E2EE + 仅诊断禁止修改）
  - [ ] SubTask 6.5.4: 实现知识库与智能排障（FAQ 搜索 + AI 分析日志推荐方案 + 社区链接）

## Task Dependencies
- [Task 1.*] depends on [Task 0.*]（L2 引擎层依赖项目初始化和 Trait 定义）
- [Task 2.*] depends on [Task 1.*]（P0 领域服务依赖 L2 核心引擎）
- [Task 3.*] depends on [Task 1.*]（P1 领域服务依赖 L2 核心引擎，可与 Task 2.* 并行部分模块）
- [Task 4.*] depends on [Task 1.*]（P2 领域服务依赖 L2 核心引擎，可与 Task 2.*/3.* 并行）
- [Task 5.*] depends on [Task 0.3]（视图层依赖 Trait 接口定义，可部分与 Task 2.*/3.*/4.* 并行）
- [Task 6.*] depends on [Task 0.*]（运维体系可与业务模块并行开发，但 E2E 测试依赖 Task 2.*-5.*）
- [Task 2.2] depends on [Task 2.1]（知识网络依赖内容编辑的 BlockChanged 事件）
- [Task 2.4] depends on [Task 2.1, Task 2.2]（AI 智能依赖内容编辑和知识网络）
- [Task 3.7] depends on [Task 2.3]（TodayView 依赖 GTD 任务数据）
- [Task 4.1] depends on [Task 0.3.7]（AgentGateway 依赖 AgentProtocol Trait）
- [Task 4.2] depends on [Task 2.3]（ExternalSyncHub 日历同步依赖 GTD 任务）
- [Task 4.3] depends on [Task 3.8]（CaptureMatrix 截图 OCR 依赖 OCR 服务）
