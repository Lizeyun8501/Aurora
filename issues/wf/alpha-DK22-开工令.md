# DK-22 开工令 — writer 批量 commit 优化（Bravo）

> 发令 Alpha 2026-10-04 · 领地：aurora-bootstrap 索引层 + write_path 增量路径调用点（均为 Bravo 自有领地；vector_search 若需触碰 = 最小补丁 + commit 声明）· 预算 3-5 人日

## 背景（writerchurn 卡 v2 之后）

v2 惰性创建落地后：**rebuild_index 真实路径 1k=1.11s**（vs 失真场景 20.4s，18.4×）；writer 复用根治双实例锁排斥。**遗留优化面**：增量路径（save_note_content → index_note）**每条仍尾 commit ~21ms**——单条编辑无感，批量导入/大 rebuild 场景 commit 次数线性放大。

## 卡面四任务

1. **bench 定位（先数据决策）**：实测增量路径 commit 占比（acquire_writer 复用后，单条 index_note 耗时分解：add_document vs commit vs 尾部 flush）——**若 commit 占比 <30% 则本卡改判「数据记录 + 不动实现」**，产出实测报告即可收卡（诚实化优先，不为优化而优化）；
2. **bulk_index 批量 API**（若数据支持）：`bulk_index(notes: &[...])` —— N 条 add_document 攒批 + **单次 commit**；调用点：rebuild_index / 批量导入路径；
3. **可见性约束文档化**：batch 中途不 commit（读端延迟=批间隔）——写进 bulk_index doc comment + perf-baseline.md 备注；**不接受后台线程自动 flush**（复杂度不成比例，明确排除）；
4. **bench 对照复测**：1k rebuild（基线 1.11s）/ 增量单条 / bulk 1k 三组数字落 `docs/evidence/perf-baseline.md`（Aurora 列更新，竞品列仍 TODO）。

## DoD 三条

1. 增量路径 commit 占比实测数据（定位报告——哪怕结论是不动实现）；
2. bulk_index（或改判说明）+ 行为级测试住 aurora-bootstrap/tests；
3. bench 对照三组数字 + CI 四门全绿（TEST=0 workspace / CLIPPY=0 / FMT=0 / DESK=0）。

## 教训注入（writerchurn 卡全量继承 + 新增）

- **测试住本 crate**（dk20 先例——不跨 crate 塞 bootstrap/tests 以外位置）；
- **bench 场景必须对齐真实调用路径**（writerchurn 证伪教训：逐条 index_note 模拟冷启动=场景失真——bulk bench 必须走 rebuild/导入真实入口）；
- **CI 原样四门自证**（TEST/CLIPPY/FMT/DESK=0 逐条贴输出）；
- **本地全量测试必带 `CARGO_PROFILE_DEV_DEBUG=0`**（30G 盘打爆根治，dk20 沉淀）；
- 本地复跑前查残留 cargo test 进程（dk20 验收伪影教训）；
- clippy -j 误传 clippy-driver 位置坑（dk20 环境记录）。

## 边界

- 不动 SearchIndexProjection 回调口径（S3 已稳定）；
- 不动向量路（enc1 跳过逻辑 DK-20 刚闭环）；
- UI/移动端零触碰。

— Alpha 2026-10-04
