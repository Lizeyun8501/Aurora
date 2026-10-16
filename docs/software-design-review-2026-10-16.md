# Aurora Note 深度审阅报告（2026-10-16）

**性质**: 插件系统暂停后全软件设计审阅 ｜ **范围**: bug 调研/架构/功能完备性/UI ｜ **基线**: 5176113（122 单元+22 e2e 全绿）

---

## 0. Bug 调研

**结论：无需立即修复的代码 bug。** 扫描面与证据：

- **静态面**：TODO 全景=桌面平台能力占位（托盘/快捷键/剪贴板/通知，接口就绪+日志占位——已知挂起非缺陷）+L2 IPC 鉴权挂点（设计内）；生产代码 unwrap/expect 全部有守卫语义或测试域
- **新链复核**：installer 环 0 重装检查/batch_set 事务性（SqliteStorage→Storage::transaction 真事务 ✓）/audit dirty 省写时序（restore 回灌不递增 dirty ✓）/hot_update 与环 0 无误伤（独立状态机）
- **既有全绿**：122+22 全 guest 重建 SOP 持续绿
- **升级为架构项**（非 bug）：**hot_update 域割裂**——HotUpdateManager 自持内存表与 installer `plugins:installed:` KV 域不互通→桌面安装的插件走 preload 报 NotFound（升级流对桌面件未接线）→归架构优化 4.1
- desktop tauri 层本机不可编译运行——运行时 bug 面依赖 CI 桌面 job（无 token 挂起中）——诚实边界

## 1. 架构评审

### 先进之处（保持并强化）

1. **分层纪律**：11 crate 职责清晰（l1 基础设施→领域服务→应用装配），traits DI 贯穿（KVStore/Storage/CryptoProvider/PluginRuntime），密码学全走 CryptoProvider（V19 §28.6）
2. **插件三层安全模型**（L0 token 消费/L1 namespace 强隔离/L2 share 只读授权）+六环 fail-closed 安装链+epoch 中断+审计双写——**系统化插件沙箱在个人笔记软件中罕见**（Obsidian 无沙箱、Notion 无本地插件、Logseq 插件无计量）
3. **E2EE 默认态**（LocalDekVault+write_path 密封+明文回退兼容）+附件内容寻址去重
4. **loro CRDT 底座**：协同与版本化有正确基座；iroh P2P 同步+离线队列+冲突两段式审查
5. **fail-closed 文化已成体系**：错误域显式、损坏快照 fail-open 仅限无害面（审计）、语义决策均有单测锚

### 优化点（按收益排序）

| # | 项 | 等级 | 说明 |
|---|---|---|---|
| 1.1 | **hot_update 域割裂** | M | 统一安装注册表：installer KV 域为唯一事实源，HotUpdateManager 改为消费方（事件广播/启动装载）——否则升级流永不对桌面件生效 |
| 1.2 | **错误域双轨** | S-M | aurora_core::Error 与 aurora_plugin::Error 手工转接（Kv/Network map_err 散点）——统一 thiserror #[from] 链或写明跨 crate 错误映射规范 |
| 1.3 | **lib.rs setup 巨石**（~1700 行） | M | 装配逻辑下沉 aurora-bootstrap（desktop 侧只留壳）——现在 bootstrap 迁移/DEK/AppCore 已下沉，但 tick/混合检索/插件状态仍在 setup 内联 |
| 1.4 | **KV 键域无 registry** | S | 九域前缀散落——HOST_RESERVED_PREFIXES 常量化（KV capability 设计稿 S1 已备） |
| 1.5 | **desktop command 层可测性** | S | 薄委托已对但无测试（GTK 限制）——把参数校验/DTO 投影提为纯函数即可本地测 |
| 1.6 | wasmtime 每启动重编译 | M | Bravo 在途（模块编译缓存） |
| 1.7 | 传输层耦合 | 远期 | iroh 硬依赖——传输 trait 化（P2P/WebDAV/S3 可换） |

## 2. 功能完备性（对照 Notion/Obsidian/Logseq/思源/Bear/UpNote/Todoist）

### 已有底座（扎实）

笔记 CRUD+混合检索（向量 768+BM25）/画布/回收站/五格式导入/附件 E2EE/P2P 同步+冲突审查/AI 两段式+Agent 面板/智能文件夹/移动端 APK/插件沙箱系统

### 关键缺口（按用户价值排序）

| # | 功能 | 参照 | 规模 | 说明 |
|---|---|---|---|---|
| 2.1 | **双向链接**（`[[]]`/反链面板/图谱） | Obsidian 核心 | M-L | **个人笔记软件最大缺口**——loro/检索底座可撑，需链接解析+反链索引+编辑器补全 |
| 2.2 | **待办/任务聚合** | Todoist | M | checkbox 任务跨笔记聚合+到期/重复+今日视图集成——「笔记+待办」双定位的关键拼图 |
| 2.3 | **导出面**（PDF/HTML/MD 批量） | 全员标配 | S-M | 只有导入无导出——数据可携性是 E2EE 软件的信任承诺 |
| 2.4 | **模板系统+每日笔记流** | Notion/Obsidian | S | 模板注入+today 视图接 daily note 一键建 |
| 2.5 | **标签系统完善** | 全员 | S | smartfolder 雏形→分层标签+标签面板+组合过滤 |
| 2.6 | **版本历史 UI** | Notion/思源 | S | loro 底座有版本化——缺回滚/对比 UI |
| 2.7 | 编辑器增强（所见即所得/表格/高亮/公式/mermaid） | Typora/Notion | L | 渐进：先代码高亮+表格，WYSIWYG 大工程单列 |
| 2.8 | 发布/公开分享 | Notion/Obsidian Publish | M | iroh 可支撑只读分享链接 |
| 2.9 | Web clipper | Notion/Obsidian | M | 浏览器扩展+导入管线复用 |
| 2.10 | 快捷键自定义+全局快速捕获 | Bear/Todoist | S-M | 桌面平台能力占位已备 |

## 3. UI 改进清单（主流模式提取）

| # | 模式 | 参照 | 现状→改进 |
|---|---|---|---|
| 3.1 | **三栏布局+右侧常驻面板**（反链/大纲/日历） | Obsidian/Notion | MainView 枚举整页切换→编辑态常驻右侧信息面板 |
| 3.2 | 命令面板增强 | Obsidian/Raycast | 已有→模糊匹配+最近文件+AI 动作（liquify/摘要）入口 |
| 3.3 | **全局快速捕获** | Bear/Todoist | 全局快捷键弹速记窗（落 inbox 笔记）——平台 TODO 已占位 |
| 3.4 | 自然语言日期快速添加 | Todoist | 「明天 9 点」解析→任务到期 |
| 3.5 | 今日仪表盘卡片化 | Notion/Sunrise | today 视图→今日任务+最近编辑+日历卡片 |
| 3.6 | 编辑器 /斜杠命令+@提及 | Notion | 编辑器内联动作 |
| 3.7 | 空状态+引导 | 全员 | 空库 onboarding/示例库导入一键 |
| 3.8 | 主题：亮暗跟随系统+accent 自定义 | 全员 | iframe 主题透传已有——app 全局主题化 |
| 3.9 | 搜索结果高亮+片段预览 | Obsidian | 列表→带高亮片段 |
| 3.10 | 移动端底部 tab+FAB | 移动标配 | APK 已有——交互对齐现代惯例 |

## 建议路线（非市场，功能+性能）

**P0**：2.1 双向链接（价值最大）+1.1 hot_update 统一注册表（架构债清偿）+2.3 导出（信任承诺）
**P1**：2.2 待办聚合、1.3 setup 下沉、2.4 模板+每日笔记
**P2**：2.5-2.10 按 3.x UI 改进配套铺开；1.2/1.4/1.5 搭车
