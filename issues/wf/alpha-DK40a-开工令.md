# DK-40a 开工令 V2：NoteDoc 混合存储与并发无损断言（loro 路线）

> 派发：Alpha 2026-10-06 10:05 · **V1（yrs 路线）作废**——栈内资产盘点实锤：loro 已承载全部 CRDT 面，yrs 属引入第二 CRDT 栈，纯负面。
> 派发对象：Bravo（**接力调整 2026-10-06 23:59：DK-39 已由 Alpha 完成交付（914d7a8），Bravo 到货直入本卡，无需接力等待**）
> 依据：DK-38 评估 §2.2R + 10:05 栈内盘点（note_doc.rs 1073 行 / crdt_engine.rs / iroh_transport.rs 693 行）

## V1 作废理由（存档）

- `aurora-core/l1_infrastructure/note_doc.rs`：NoteDoc 混合容器已定型——meta(LoroMap)/body(**LoroText 富文本**)/blocks_tree(**LoroTree 块树**，超出 V1 平序列设计)/backlinks
- `aurora-core/traits/crdt_engine.rs`：CrdtEngine trait 已在（create_document/apply_ops/get_snapshot/get_history/merge_branch）
- `aurora-sync/iroh_transport.rs`：P2P 双向同步已通（sync_with_peer/accept_sync，版本向量交换+增量导入），移动端 p2p_sync 已接
- 依赖已在栈内（loro 1.x），零新增供应链面；编辑器/快照/画布语义同源（DK-12 fork 教训资产直接适用）

## 本卡真实缺口（V2 范围，规模 大→中）

### 1. update log 持久化（混合存储补全）

- NoteDoc 编辑事务 → loro update 二进制（`doc.export(ExportMode::Update(since))`）追加持久化（sea-orm BLOB 列或新表——Bravo 探明 migration/entity 后定，倾向独立表）
- 打开 note：既有快照载入 → apply update log 重放 → 就绪
- compaction：log 超 N 条（建议 500，可配）→ 全量快照回写 + log 清空
- 与既有快照架构的关系**必须探明后对齐**：快照现走 `kv notesnap:{id}`（cmd_save 面）——本卡 log 键域与 kv vs DB 的归属由 Bravo 探明现状后裁决，卡内注记即可，不强制迁 DB

### 2. 并发无损断言（用户裁决核心验收点）

- 双实例 NoteDoc（A/B）基于同一基线：A 插块+改文本、B 删块+改同块文本 → 双向 export/apply 合并 → **两侧增量全保留、零丢失**（块级 LoroTree+字符级 LoroText 双断言）
- 断言住 aurora-core 本地 tests mod（测试住址铁律）

### 3. iroh 链路对接点探明（只探不接）

- sync_with_peer 拿到的增量与 update log 键域的衔接点写清注记（38b 的输入），本卡不做传输面改动

## DoD

1. 双端并发编辑零丢失断言绿（LoroTree 块级+LoroText 字符级）
2. 快照+log 重放等价（重放终态 == 连续编辑终态）
3. compaction 前后文档态等价
4. 存量 note 首开自动初始化（既有快照灌入，零迁移）
5. 四门 TEST=0 CLIPPY=0 FMT=0 DESK=0（DESK 本地环境缺失以 CI 为准）

## 边界外

- WebSocket/iha 传输面任何改动（38 系列裁决挂起中）
- 移动块 op 语义（LoroTree moveExisting 已有——探明即注记，不新做）
- 多 workspace 全局 doc（per-note 隔离够 V1）
