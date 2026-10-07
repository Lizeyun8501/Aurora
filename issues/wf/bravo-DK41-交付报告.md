# DK-41 交付报告：sync 层 updatelog↔传输桥接（选项 A 拆卡首张）

> 执行：Bravo 2026-10-07 深夜 · 依据：bravo-request-DK38-裁决输入（cc25641）建议 A 拆卡方案
> +「基于最优方式执行」授权语义（TA「开工继续」）· 40c 实测数据支撑（f7a1146）
> Commit：见 git log（feat(DK-41)）

## 落地面

### 1. 接收侧桥接（本卡核心——防重启丢对端编辑）
- `IrohTransport` 新增 `update_sink: Mutex<Option<UpdateSink>>` + `set_update_sink()`
- `sync_with_peer` / `accept_sync` 两个 import 成功点后调 sink——远端增量
  交宿主持久化（None = 纯内存 sync，既有行为不变）
- **语义**：sync 收到的对端 update 追加本地 KV updatelog → 重启后打开链
  重放（快照+log，40a 语义）含对端编辑——P2P 同步与混合存储持久化闭环

### 2. 发送侧
- 无需改动：`sync_with_peer` 已按版本向量差量导出 update（增量交换既有）

### 3. core 原语 pub 化（write_path 40a 领地内）
- `append_update_log / read_update_log / count_update_log` → `pub`
  （38b 桥接原语，宿主层调用面）

## DoD 验证

**dk41_updatelog_bridge_persistence**（内存仿真网 e2e）：
1. 双端各持 `MemoryKVStore` + sink 注入（收到的 update → `append_update_log`）
2. 单轮双向 sync
3. 断言双端 updatelog 非空（对端增量已持久化）
4. **核心断言**：模拟 B 重启——全新空 doc 仅灌 B 的 updatelog 重放 →
   恢复出 A 端编辑（KV 态一致 → 打开链重放语义闭环）

## 验证矩阵

| 门 | 结果 |
|---|---|
| dk41 | 1/1 ✅ |
| sync 全量（dk08 回归+dk40c+dk41） | 全绿 ✅ |
| TEST（全量 workspace --exclude desktop） | 0 ✅ |
| CLIPPY | 0 ✅（iroh-transport + default 双 feature 组合，-D warnings） |
| FMT | 0 ✅ |
| DESK | CI main-only（dev 不触发，历史同款） |

## 坑沉淀
- parking_lot Mutex（iroh 依赖树）`lock()` 直接返回 Guard 非 Result——
  `.ok()/.expect()` 不可用；guard 借用期间调 sink 需作用域包裹（防重入死锁）
- clippy `type_complexity`：`Mutex<Option<Arc<dyn Fn>>>` 须抽 type 别名
- 2 核机上 iroh-transport/default 双 feature 变体交替编译 = 全量门 ~40min（长命令
  setsid 落盘轮询不变量再证）

## 剩余（38 系列后续）
- 选项 A 正式裁决后：38b 传输单元协议帧（updatelog 批量交换/ack 游标）可基于
  本桥接面展开；真实双机实测需物理环境
