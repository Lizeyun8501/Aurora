# Alpha DK-21 S3 复核回执（自领自复核）— 依赖编辑器（前端交互收口）

- **对象**：`c4e4bc4` · **日期**：2026-09-29 18:24–19:40 · 基线 3f4369e
- **结论**：**通过（core 402 passed + clippy 0 + fmt 净 + vite/tsc 净；CI 排队——
  下会话首查 c4e4bc4）**

## 一、前置确认

**ef77d8b 全绿**（S2 依赖骨架 CI 闭环——desktop Rust 四命令编过，内联 downcast
跨 await Send 无虞）+ 3f4369e 四绿（纯 docs）。

## 二、S3 交付（依赖编辑器——用户可操作的依赖管理闭环）

1. **读边 API**：`TaskProjection::get_dependencies(task_id)`（精准 KV get，无边
   返回空）+ tauri `cmd_get_task_dependencies`——S2 缺的查询面补全（五个依赖命令
   齐：set/remove/get/blocked_ids/today_task_rows）；
2. **前端 DepEditor**（DesktopShell）：
   - 任务行**点击展开**依赖区（收起切换，行尾「依赖 ▼」提示）；
   - 现有前置列表 + 逐项移除按钮；
   - 「添加前置」下拉（候选=今日任务排除自身与已有前置——防重复边）；
   - **成环拒绝消息直显用户**（`A → B → C → A` 可读路径——core 层校验的用户
     可见化，错误展示于编辑区内不弹窗）；
3. 交互闭环：编辑动作 → onDone → 刷新任务行（blocked 徽章即时重算）。

测试：get_dependencies 读取往返（移除后空/正常读回/无边任务空）——core 全量
402 passed（403→402 注：S2 测试合并断言重整后总数微调，零失败）。

## 三、DK-21 卡进度

- ✅ S1 引擎面 / S2 投影 blocked 骨架 / **S3 依赖编辑器（本片）**
- 卡面四任务项状态：「依赖创建/删除 API」✅（引擎+投影+tauri 全链）、「环检测
  拒绝+可读提示」✅、「blocked 联动规则落码」✅（Alpha 裁决派生态）、「UI 展示」✅
  （今日视图徽章+依赖编辑器）——**卡实质完成**（TaskDependencyChanged 事件发射
  挂事件总线装配，余量为后续增强非卡面阻塞项）
- 建议：下会话确认 CI 后**关闭 DK-21**

## 四、教训

- TS 模块私有 const（btn）跨组件不可见——样式常量要么提取共享要么每组件自定义
  （本片 depBtn 就地定义）；
- heredoc 转义（\* 序列）在 python -c 单行字符串里失效——多行块用 heredoc EOF
  传 stdin 而非 -c 字符串。

— Alpha 2026-09-29 晚
