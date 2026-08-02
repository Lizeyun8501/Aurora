# Checklist

## Phase 0: 项目初始化与架构骨架
- [x] Monorepo 目录结构完整（crates/aurora-*, apps/desktop|mobile|web|extension, shared/types|ui-components）
- [x] Cargo.toml workspace 和 package.json workspace 配置正确
- [x] rustfmt、clippy、EditorConfig 代码规范工具链配置完成
- [x] L1 基础设施层所有依赖集成（Loro, iroh, Tantivy, LanceDB, sqlite-vec, Wasmtime, rust-crypto, PaddleOCR）
- [x] 7 大 Trait 接口定义完整（CrdtEngine, SyncTarget, VectorStore, AIProvider, Storage, PluginRuntime, AgentProtocol）
- [x] EventBus 实现支持 publish/subscribe 多消费者模式
- [x] 层间通信数据序列化规范（bincode/JSON/protobuf）定义完成

## Phase 1: L2 核心引擎层
- [ ] 事件溯源引擎：Event 结构包含完整字段（event_id, block_id, op_type, payload, timestamp, user_id, device_id, signature）
- [ ] 事件溯源引擎：支持快照策略（每 1000 事件自动快照 + 启动加载快照+增量）
- [ ] 事件溯源引擎：SQLite WAL 模式事件存储按 workspace 分库
- [ ] 工作流引擎：支持 DSL 状态机定义和三种触发器（时间/事件/API）
- [ ] 权限引擎：RBAC 五级角色 + ABAC 属性条件 + 四级层级权限传播
- [ ] 属性引擎：8 种基础类型 + JSON Schema 校验 + 热点属性自动索引
- [ ] 查询引擎：统一 Query DSL + 成本优化器自动选择执行路径 + LRU 缓存
- [ ] 捕获引擎：管道架构完整 + SimHash + URL 指纹双重去重
- [ ] L1 Trait 实现全部完成（LoroCrdtEngine, IrohSyncTarget, LanceDbStore, SqliteStorage, WasmtimeRuntime, McpAgentProtocol 等）

## Phase 2: P0 领域服务模块
- [ ] 内容编辑：块级文档模型支持 9 种核心块类型 + 插件自定义类型注册
- [ ] 内容编辑：TipTap/ProseMirror 与 Loro CRDT 双向绑定（pmToLoro/loroToPm）
- [ ] 内容编辑：IME 合成期间暂停同步，compositionend 后批量提交
- [ ] 内容编辑：Markdown 实时转换 + CommonMark/GFM/Aurora 扩展导出
- [ ] 内容编辑：协作编辑基于 Loro 自动合并 + Awareness 协议协作光标
- [ ] 内容编辑：版本历史支持时间旅行 checkout + 版本对比 diff + 分支合并
- [ ] 内容编辑：评论批注使用基于内容的锚定 + ProseMirror Decoration 渲染
- [ ] 知识网络：双链引用 WikiLink + MarkdownLink 解析 + SQLite links 表索引
- [ ] 知识网络：链接索引 100ms debounce 异步更新
- [ ] 知识网络：反向链接面板实时更新 + 上下文预览
- [ ] 知识网络：知识图谱 D3.js 力导向图 + WebGL 加速 >1000 节点
- [ ] 知识网络：关系属性支持语义标签（支持/反驳/引用/扩展）
- [ ] 知识网络：图谱探索支持 BFS + 向量相似度推荐 + 最短路径发现
- [ ] GTD：工作流状态机完整（Inbox → Clarified → Organized → Scheduled → Doing → Done | Archived）
- [ ] GTD：项目/任务无限层级嵌套（closure table 模式）
- [ ] GTD：重复任务 RRULE 解析 + 模板/实例分离
- [ ] GTD：提醒系统 Desktop 系统通知 + Mobile APNs/FCM 推送
- [ ] GTD：习惯追踪链式模型 + streak 指标 + 热力图
- [ ] GTD：时间线视图日/周/月 + 拖拽调整 + 日历事件集成
- [ ] GTD：自动化规则 IFTTT 模式（When/And/Then）
- [ ] AI：混合推理架构 AIProviderRouter 支持 LocalFirst/CloudOnly/Auto 策略
- [ ] AI：智能续写 debounce 500ms + 流式 + ghost text + Tab/Esc 交互
- [ ] AI：内容摘要三级粒度 + Map-Reduce + 缓存关联版本号
- [ ] AI：问答对话 RAG 架构（Embedding → LanceDB → 上下文 → LLM）
- [ ] AI：混合搜索 Tantivy + LanceDB + Cross-Encoder 重排序 + RRF 融合
- [ ] AI：自动标签 TF-IDF + TextRank + 用户确认不强制
- [ ] AI：任务分解 LLM 生成 + 预览编辑 + 批量创建

## Phase 3: P1 领域服务模块
- [ ] 安全加密：三级密钥结构（密码 → Master Key → DEK）+ Argon2id（64MB/3次/4并行）
- [ ] 安全加密：E2EE 明文 → AES-256-GCM → 密文存储/同步 + 零知识架构
- [ ] 安全加密：后量子 ML-KEM-768 + X25519 双轨并行混合密钥交换
- [ ] 安全加密：生物识别（FaceID/TouchID/Windows Hello + TPM/Secure Enclave）
- [ ] 安全加密：密钥恢复 BIP39 助记词 + Shamir 3 选 2 + 设备 QR 码授权
- [ ] 同步：P2P iroh + QUIC + NAT 穿透 + Loro Sync Protocol
- [ ] 同步：云端 WebSocket 实时 + HTTPS 批量 + 密文中继
- [ ] 同步：局域网 mDNS 发现 + IP 直连 + 优先于云端
- [ ] 同步：冲突解决 CRDT 自动 + 语义冲突 UI + 分支模式
- [ ] 同步：增量同步 ops 增量 + rsync-like 块级 + zstd 压缩
- [ ] 同步：离线队列 SQLite + 优先级 + 幂等 + 批量压缩
- [ ] 同步：多设备 Device ID + QR 授权 + 远程撤销 + DEK 失效
- [ ] 插件：双模式 WASM + iframe + 完整生命周期
- [ ] 插件：WASM Wasmtime + WASI + 宿主函数 + Capability 控制
- [ ] 插件：iframe sandbox + postMessage JSON-RPC 2.0 + CSS 变量透传
- [ ] 插件：最小权限 + 运行时检查 + 动态升级
- [ ] 插件：市场去中心化 + 官方/第三方/本地 + 签名 + 热更新 + 灰度
- [ ] 导入导出：Markdown pulldown-cmark 管道模式 + YAML frontmatter
- [ ] 导入导出：Notion API 块类型映射 + 数据库映射 Collection
- [ ] 导入导出：PDF 浏览器打印 + 自定义模板 + 批量
- [ ] 导入导出：批量 Zip + manifest.json + 断点续传
- [ ] 素材库：SHA-256 内容寻址 + 天然去重
- [ ] 素材库：缩略图 image crate + ffmpeg + WebP + 按需生成
- [ ] 素材库：EXIF kamadak-exif + GPS/时间/设备
- [ ] 素材库：重复检测 pHash / CNN Embedding
- [ ] 系统设置：分层存储 + serde 强类型 + 版本迁移
- [ ] 系统设置：主题 CSS Variables + Light/Dark/Sepia/HighContrast/Auto
- [ ] 系统设置：快捷键全局+编辑器 + 冲突检测 + 平台适配
- [ ] TodayView：聚合视图 QueryEngine 预计算 + Zustand + EventBus 增量
- [ ] TodayView：时间线日/周/月 + react-window 虚拟列表 + 拖拽回写
- [ ] TodayView：专注模式 Pomodoro + 白噪音 + 专注统计
- [ ] TodayView：每日回顾自动生成 + 历史浏览
- [ ] OCR：双引擎 PaddleOCR + Tesseract + OCRProvider Trait
- [ ] OCR：图像预处理流水线（去噪/二值化/倾斜校正/版面分析）
- [ ] OCR：表格识别 PP-Structure + 单元格 OCR + Markdown 重组
- [ ] OCR：公式识别 Texify + LaTeX + MathBlock
- [ ] OCR：批量 tokio 线程池 + EventBus 进度

## Phase 4: P2 领域服务模块
- [ ] AgentGateway：MCP 协议 JSON-RPC 2.0 + stdio/SSE 传输
- [ ] AgentGateway：工具注册 AgentProtocol + ToolRegistry + 动态发现
- [ ] AgentGateway：编排 Sequential/Parallel/Hierarchical + LLM 生成计划 + 可视化
- [ ] AgentGateway：上下文 SQLite 持久化 + 会话恢复 + 窗口压缩
- [ ] AgentGateway：安全沙箱权限校验 + 审计日志 + 只读模式
- [ ] ExternalSyncHub：SyncConnector Trait + 注册表 + 状态管理
- [ ] ExternalSyncHub：日历 CalDAV + GTD 映射 + CTag/ETag 增量
- [ ] ExternalSyncHub：邮件 IMAP + 捕获为文档 + 附件提取
- [ ] ExternalSyncHub：云盘 WebDAV + 厂商 API + 选择性双向同步
- [ ] ExternalSyncHub：Webhook HMAC-SHA256 签名验证
- [ ] CaptureMatrix：网页剪藏 Readability + Markdown + Message Passing
- [ ] CaptureMatrix：截图 OCR 全局快捷键 + 悬浮窗 + 截图即笔记
- [ ] CaptureMatrix：语音速记 Whisper.cpp + WebRTC + 实时转写
- [ ] CaptureMatrix：RSS feed-rs + 15 分钟轮询 + 全文抓取

## Phase 5: 视图层与适配层
- [ ] React 18 + TypeScript ≥5.0 严格模式 + TipTap v2 + Zustand v4 + TanStack Query v5
- [ ] 视图层目录结构完整（components/blocks/editors/layouts/views/hooks/stores/adapters）
- [ ] 块级渲染器 9 种核心块类型实现
- [ ] 编辑器外壳 DocumentEditor/CanvasEditor 实现
- [ ] 布局组件 Sidebar/SplitPane/Modal 实现
- [ ] 页面级视图 TodayView/GraphView/SettingsView 实现
- [ ] 自定义 Hooks useBlock/useSync/useAI 实现
- [ ] PlatformAPI 统一接口定义完整（存储/网络/系统/加密/生物识别）
- [ ] Tauri v2 Desktop 适配（系统托盘/全局快捷键/文件系统/原生菜单 + IPC）
- [ ] Capacitor v8 Mobile 适配（推送/相机/生物识别/离线存储 + Bridge）
- [ ] WASM Web 适配（WASM32 编译 + PWA + IndexedDB + OPFS）
- [ ] 模块间数据交互视图全部实现（编辑↔知识网络、GTD↔编辑、AI↔编辑↔知识网络、同步横切）

## Phase 6: 运维可观测性体系
- [ ] 日志系统 tracing 结构化 + span 上下文 + 5 级 + JSON 旋转 + 采样 + 脱敏
- [ ] 指标系统 prometheus /metrics + 业务/性能/资源/质量四类 + 标签切片
- [ ] 分布式追踪 OpenTelemetry + 上下文传播 + 尾采样 + OTLP 导出
- [ ] 崩溃报告 Sentry + panic 捕获 + breadcrumbs + 加密脱敏
- [ ] 健康检查 /health + 启动自检 + 60 秒巡检 + 状态栏指示器
- [ ] 告警规则 P0/P1/P2 三层 + 本地通知 + 云端 Webhook
- [ ] 智能降噪 聚合 + 静默期 + 依赖抑制 + 自愈检测
- [ ] 监控仪表板 Grafana 模板 + Desktop 内嵌轻量面板
- [ ] 单元测试 cargo test + vitest + Mock + proptest 属性测试
- [ ] CRDT 一致性测试 猴子测试 + delta-debugging + 24 小时稳定性
- [ ] E2E 测试 Playwright 跨端 + 关键场景 + 视觉回归 + 崩溃恢复
- [ ] 性能基线 criterion.rs + CI 10% 退化 + k6 加载 + 内存分析
- [ ] 覆盖率 tarpaulin Rust ≥70%/核心 ≥80% + vitest TS ≥60% + Codecov
- [ ] 灰度更新 1%→100% + 紧急制动
- [ ] 功能开关 Boolean/Percentage/Targeting + 离线生效 + 核心功能不设开关
- [ ] 热修复 Web CDN + Desktop delta + Mobile Appflow + 仅视图层/适配层
- [ ] 版本回滚 保留 3 版本 + 崩溃 3 次自动 + 数据格式检测
- [ ] 诊断包 ZIP + 7 天脱敏日志 + 加密 + 50MB 限制
- [ ] 自助修复 索引重建/缓存清理/同步重置/权限修复/配置重置 + 备份
- [ ] 远程协助 企业版会话码 + E2EE + 仅诊断
- [ ] 知识库 FAQ + AI 智能排障 + 社区链接
