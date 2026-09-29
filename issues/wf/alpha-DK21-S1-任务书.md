# Alpha 自领任务书 — DK-21 S1：任务依赖 API + 环检测 + blocked 联动（core 面）

> 派发对象：Alpha（自领）· 2026-09-29 10:00 · 基线 `8c593ca`
> 领地隔离：Bravo 占 l2_engines（DK-03 S2）+ write_path/NoteRecord（DK-02 S2）——本卡只碰
  l3_domain/gtd_system.rs（Task 结构 + GtdEngine）+ UI 任务详情面——**零交集**。
> 卡面「blocked 联动规则 Alpha 预裁决」= 本任务书 §三。

## 一、范围

1. **依赖存储**：`Task` 加 `depends_on: Vec<String>` 字段（`#[serde(default)]` 向后兼容——
   旧 JSON 无字段反序列化为空）；随 task 序列化落盘，不另设存储；
2. **GtdEngine 依赖 API**：
   - `add_dependency(task_id, depends_on)`：自依赖拒绝；存在性校验；**环检测复用
     `detect_dependency_cycle`（DK-06 已交付零调用——本卡首次接线）**，成环拒绝并返回
     可读环路径 `A → B → C → A`；成功发 `TaskDependencyChanged { op: Add }`；
   - `remove_dependency`：删除 + 事件（Remove）；
   - `get_dependencies` / `is_blocked` / `blocked_tasks`；
3. **blocked 联动规则（Alpha 裁决，落码为引擎文档注释）**：
   - **派生态不落存储**：`is_blocked = any 前置 status 非终态（Done/Cancelled）`——实时计算，
     永不落盘（避免双写不一致，C03 fail-closed 同构思路）；
   - **软联动不硬拒绝**：blocked 任务仍可手动 transition_to（硬拒绝卡死工作流；UI 展示
     blocked 徽章提示即可——GTD 哲学：系统提示、人来决策）；
   - 完成前置 → blocked 派生自动解除（无需事件写状态）；
4. **UI 面**（今日若余量）：任务详情 blocked 徽章 + 依赖列表（ desktop 优先）。

## 二、DoD

1. 单测：自依赖拒绝 / 直接环拒绝（A↔B）+ 环路径可读 / 三方环（A→B→C→A）/
   正常 add/remove 往返 / is_blocked 派生（前置 Done 解除）/ **serde 向后兼容**（旧 JSON
   无 depends_on 字段 → 空 Vec）；
2. 全量 -p aurora-core 不回退 + clippy 0 + fmt 净 + tsc/build（若动 UI）；
3. CI 五 job 绿；UI 深度交互（依赖编辑器）挂起给 S2。

— Alpha 2026-09-29
