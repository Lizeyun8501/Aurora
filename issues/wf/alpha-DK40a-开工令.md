# DK-40a 开工令：aurora-crdt——yrs 集成与 per-note YDoc 数据面

> 派发：Alpha 2026-10-06 · 依据：DK-38 设计评估 §2.2R 修订（用户裁决 2026-10-06 09:05：不接受 LWW 语义收缩，CRDT 前置）
> 派发对象：Bravo（主力）。**DK-39（沙箱）交付验收后接力本卡；若并行有余量可与 DK-39 并行**

## 背景

多设备同步（DK-38 系列）冲突面采用 CRDT 路线：**块级+字符级并发编辑零丢失**（用户裁决核心验收点）。选型 yrs（y-crdt Rust 官方实现）——本卡建立 CRDT 数据面，为 38b 协议帧的 update 交换打地基。

## 技术方案（评估 §2.2R 展开）

### 新 crate：aurora-crdt

- 依赖：`yrs`（版本锁最小次版本；引入时跑 cargo audit 过供应链门）
- per-note YDoc 映射：
  - YArray 装块序列（块级并发：双端新增块全保留、删除块都删、移动块需显式 op——V1 移动=删除+插入，语义注记）
  - 每块 YText 装块内文本（字符级无损）
  - note 元数据（title/tags/updated_at）**不入 YDoc**——保留既有存储面（并发冲突极低，不过度设计）

### 混合存储（sea-orm 面）

- 既有快照（serde 序列化）+ 新增 yrs update log 持久化（BLOB 列 or 新表——**Bravo 探明 migration/entity 现状后定**，倾向独立表避免动既有 schema）
- 打开 note：load 快照 → apply update log 重放 → YDoc 就绪
- compaction：update log 超 N 条（建议 500，可配）后 encodeStateAsUpdate 全量快照回写+log 清空
- 写路径：编辑操作 → YDoc 事务 → update 二进制追加 log（这就是 38b 要传输的增量单元）

## DoD

1. **核心断言（用户裁决对应）**：双端并发编辑同 note e2e——A 端插块+改文本、B 端删块+改同块文本，交换 update 合并后**两侧增量全保留、零丢失**（块级+字符级断言）
2. 快照+log 重放等价性：重放结果与连续编辑态一致（属性测试或固定序列断言）
3. compaction 后行为不变（压缩前=压缩后文档态）
4. 存量 note 首次打开自动初始化 YDoc（快照灌入，零迁移脚本要求）
5. 四门 TEST=0 CLIPPY=0 FMT=0 DESK=0（DESK 本地环境缺失以 CI 为准——老规矩）

## 边界外（注记，不做）

- WebSocket 传输/协议帧（38a/38b 面）
- 移动块 op 语义（V1 删除+插入注记）
- 38b 做在线 update 交换，本卡只保证 update 二进制可编码可合并
- 多 workspace 全局 YDoc（per-note 隔离已够 V1）
