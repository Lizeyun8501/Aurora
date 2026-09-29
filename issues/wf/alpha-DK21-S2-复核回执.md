# Alpha DK-21 S2 复核回执（自领自复核）— 任务依赖 UI 骨架（投影链 blocked 徽章）

- **对象**：`ef77d8b` · **日期**：2026-09-29 18:05–19:00 · 基线 70fd872
- **结论**：**本地矩阵通过（core 403 passed + clippy 0 + fmt 净 + vite/tsc 净）；CI
  排队中——下会话首查 ef77d8b**

## 一、前置确认

**9855c8d/70fd872 全绿**（本会话开头）——DK-10 切片 7 与全部七切片 CI 闭环。

## 二、S2 交付（挂点裁决 + 三层链路）

### 挂点裁决（本片关键决策）

桌面任务真实数据源是 **TaskProjection（l2_engines）而非 GtdEngine（l3 内存引擎）**——
两条任务链并存（内存引擎 vs 投影读模型）。blocked 徽章挂投影链（TodayView 真渲染
路径）；GtdEngine 链的 is_blocked 保留（任务编辑器落地后经事件流喂投影）。**终态
值域按链独立**：投影链 = "done"/"cancelled"（字符串），引擎链 = Done/Archived（枚举）。

### 三层链路

1. **TaskProjection 依赖面**（l2_engines/task_projection.rs）：
   - 依赖边存 KV `gtd:dep:{task_id}` → JSON Vec<String>（随任务行分离存储——
     边增删不触碰行投影）；
   - `set_dependency`（自依赖/成环拒绝，**可读路径 `A → B → C → A`**）/
     `remove_dependency`（幂等）/ `blocked_ids()`（**派生态实时计算不落存储**——
     any 前置 status 非终态即阻塞，S1 裁决延续）；
   - DFS **内联**（l2 不反向依赖 l3——与 l3_domain::detect_dependency_cycle 同构
     独立，注释注明）；
2. **tauri 四命令**：set/remove/blocked_task_ids + **today_task_rows**（today 行
   join blocked 标记，blocked 优先排序——阻塞任务前置引导用户先处理依赖）；
   内联 downcast 借 core（局部 Arc 跨 await 安全）；
3. **前端 TodayTaskRows**（DesktopShell）：⛔ blocked 徽章（红）vs 状态徽章（蓝）
   双载体 / blocked 前置排序 / browser-mock 提示态（模式照抄 TrashView）。

测试：dk21_dependency_blocked_derived 全链（自依赖/成环路径可读/blocked 派生+前置
done 解除/移除幂等）——core 403 passed（402→403）。

## 三、CI 终态汇总

| commit | 五 job | 备注 |
|---|---|---|
| 9855c8d/70fd872（切片 7+回执） | **全绿** | DK-10 七切片 CI 全闭环 |
| ef77d8b（本片） | 下会话首查 | 依赖 UI 骨架（vite/tsc 本地净） |

## 四、DK-21 卡进度

- ✅ S1 引擎面（依赖 API+环检测+blocked 联动+测试）
- ✅ **S2 投影链 blocked 骨架（本片）**——「依赖创建/删除 API」投影面补全 +
  「UI 展示」起步（今日视图 blocked 徽章）
- ⏸️ S3：依赖编辑器（add/remove 前端交互——命令面已就绪）/ 任务详情面板 /
  TaskDependencyChanged 事件发射接线（随事件总线装配）

— Alpha 2026-09-29 晚
