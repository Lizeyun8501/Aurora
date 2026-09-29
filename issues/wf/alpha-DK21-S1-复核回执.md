# Alpha DK-21 S1 复核回执（自领自复核）— 任务依赖 API + 环检测 + blocked 联动

- **对象**：`5ea3054` · **日期**：2026-09-29 09:59–11:00
- **结论**：**通过（本地矩阵闭环：401 passed 零回退 + clippy 0 + fmt 净；CI 五 job 排队中
  ——runner 拥挤时段，下会话首查终态）**

## 一、交付面

1. **Task.depends_on 字段**：`#[serde(default)]` 向后兼容（旧 JSON 无字段 → 空 Vec——
   存量数据零迁移成本）；随 task 序列化落盘，不另设存储面；
2. **GtdEngine 依赖 API**：
   - `add_dependency`：自依赖拒绝 / 双端存在性校验 / 重复幂等 / **环检测接线
     `detect_dependency_cycle`（DK-06 交付后首次真调用）**——成环拒绝并返回可读路径
     `A → B → C → A`；
   - `remove_dependency`（幂等）/ `get_dependencies` / `is_blocked` / `blocked_tasks`；
   - `TaskStatus::is_terminal`（终态=Done/Archived）；
3. **blocked 联动规则（Alpha 预裁决，落码即文档——卡面要求项）**：
   - **派生态不落存储**：实时计算（any 前置非终态）——避免 blocked 标志与前置状态双写
     不一致（C03 fail-closed 同构思路）；
   - **软联动不硬拒绝**：blocked 仍可手动 transition_to（硬拒绝卡死工作流；UI 徽章提示，
     GTD 哲学=系统提示、人来决策）；
   - 完成前置 → blocked 派生自动解除（无事件写状态）。

## 二、验证矩阵

| 项 | 结果 |
|---|---|
| cargo test -p aurora-core dk21 | ✅ 2 passed（生命周期全链 + serde 向后兼容） |
| cargo test -p aurora-core 全量 | ✅ 401 passed 0 failed（399→401 零回退） |
| clippy -p aurora-core --all-targets | ✅ 0 |
| fmt | ✅ 净 |
| CI 5ea3054 | Rustfmt ✅；余四 job 排队（下会话首查） |

测试生命周期覆盖：自依赖 / 直接环+路径可读 / 三方环 / 重复幂等 / blocked 派生+前置
Done 解除 / 移除。

## 三、过程教训（入档）

- **记忆漂移抓正**：memory 里「DK-20=合并裁决三件套（锁定态索引+密钥导出）」与卡面
  「单篇笔记加密 Vault」不符——**开工前必读卡面原文，记忆仅作索引**。且 DK-20「锁定不进
  索引」必碰 l2_engines（Bravo DK-03 S2 占）+NoteRecord（DK-02 S2 占）——领地冲突改选
  DK-21 ✓；
- 测试住址错位一例：dk21 测试初写到 bootstrap（patch 脚本 p2 锚点错位）——
  `cargo test -p aurora-core dk21` 0 passed 暴露——**测试必须住被测对象 crate**
  （gtd_system.rs 本地 tests mod），已迁移重验；
- 范围裁决：desktop 无任务 UI 组件（TodayView/TaskDetailView 均不存在）——UI 面挂 S2
  （需先建任务 UI 骨架，独立体量）。

## 四、挂起项

- UI 面：任务详情 blocked 徽章 + 依赖列表 + 依赖编辑器（S2——随任务 UI 骨架）；
- TaskDependencyChanged 事件发射（事件面已有定义；S1 引擎内 API 直调场景无事件总线
  注入——随任务 UI 骨架接 EventBus）；
- blocked 徽章在 TodayView/看板视图的展示（随 UI 骨架）。

## 五、DK-21 卡进度

- S1 core 面 ✅（依赖 API+环检测+blocked 联动规则+测试）——卡面四任务项之「环检测」
  「blocked 联动规则落码」两项完成；「依赖创建/删除 API」完成（引擎层）；「UI 展示」挂 S2。
- 卡关闭条件：S2 UI 面 + CI 终态确认。

— Alpha 2026-09-29 上午


---

## 终态补记（2026-09-29 12:25）

**CI 5ea3054 五 job 全绿**（Rustfmt/desktop-check/Test(stable)/MSRV 1.91/Clippy——GitHub API 确认）——**DK-21 S1 core 面正式闭环**。S2（UI 面挂起项）排队。
