# DK-14c.3 设计稿 · 数据面 guest-invoked（ReadDoc/WriteDoc/QueryDocs）

**出稿**: Alpha 2026-10-14 10:40 ｜ **评审**: 用户可随时否决（默认直派） ｜ **Estimate**: 3-4h ｜ **Labels**: `plugin-ecosystem`, `area/wasm`, `area/security`, `dk-14c收官`

## 1. 背景与定性

14c.2 落地 guest-invoked 机制（log 首端点）。本卡开放**数据面三函数**，14c 收官。

**关键定性（纠正直觉）**：d2 的 `docs` 是**宿主内存 KV 存储**（`RwLock<HashMap<String, Value>>`，插件数据空间），**不是用户笔记文档库**——数据暴露面是插件间共享 KV，非用户私有数据直读。用户笔记库访问属未来独立设计（若需要，另行立项审查）。

## 2. 安全审查（本卡核心）

### 2.1 共享 KV 语义（必须文档化披露）

`docs` 无 plugin_id 维度——**所有插件共享一个 KV 空间**：插件 A 写 doc-X，插件 B（有 Storage）可读改。L0 语义下可接受（granted per-set+同机互信模型），但：

- **文档化**：日志约定文档+README 数据语义节披露共享语义
- **L1+ 台账**：per-plugin namespace（doc-id 强制前缀 `{plugin_id}/`）——本卡不做，进 14c.4+ 候选

### 2.2 审计策略（数据面≠log，差异化）

| 操作 | 审计 | 理由 |
|---|---|---|
| write-doc | **逐条**（含 doc_id+data 大小） | 状态变更必须留痕 |
| read-doc | 首调+越权 | 查询类可节流（对齐 14c.2 log 先例） |
| query-docs | 首调+越权 | 同上（键枚举暴露面小） |
| 三者越权 | 逐条 `reason=data-denied`（含 method） | 越权不节流 |

### 2.3 容量防御（新增约束，guest 侧先行）

write-doc data 加上限（**1 MiB**，`result` 错误语义化）。**不动 d2 host-invoked**（最小 diff）——host-invoked 同步加 guard 写台账（T9 候选）。

### 2.4 require 语义

三函数统一 `require(Storage)`（对齐 d2 `required_capability` 映射——Capability::Storage 复用，无新变体）。token-None 拒路径沿用现有占位语义（台账①）。

## 3. WIT 设计（0.2.0 内演进，形态复用 14c.2 裁决）

```wit
interface capability {
    // 14c.2 已有: resource capability-token; log: func(...)

    /// 读 KV（d2 docs 空间，全 runtime 共享——语义见文档）。不存在→none。
    read-doc: func(token: borrow<capability-token>, doc-id: string) -> option<string>;
    /// 写 KV（data=JSON 序列化字符串；上限 1 MiB）。超限→err。
    write-doc: func(token: borrow<capability-token>, doc-id: string, data: string) -> result<_, string>;
    /// 键检索（doc-id contains 匹配，非内容搜索）。
    query-docs: func(token: borrow<capability-token>, pattern: string) -> list<string>;
}
```

- **payload=string（JSON 序列化）**：对齐 d2 Value 语义+确定性优先（level string 同思路）；KV 值不执行，无注入面
- **read 返回 option**：d2 Null→`none` 语义化
- **query→query-docs**：避开未来内容搜索 `query` 撞名
- **自由函数+token borrow**：14c.2 形态裁决复用，不重开

## 4. 宿主实现要点

- `func_wrap` 平名三函数；`Resource<CapabilityTokenData>` typed 参数（14c.2 同构）
- `docs` 访问：runtime 的 `RwLock<HashMap>` 直通（宿主与 host-invoked 同一存储——**同一数据空间跨调用模式可见**，设计使然）
- write-doc：size guard→insert→逐条审计；read/query：首调标记（per-plugin state 复用 `log_first_called` 模式扩为 `data_first_called` 或 per-method 标记——实现裁决）

## 5. e2e 契约

1. **granted 全链**：hello（manifest 补 Storage）写→读→query→断言数据一致
2. **越权**：granted 无 Storage→write-doc 拒（CapabilityDenied(Storage)+审计 data-denied）
3. **容量**：>1 MiB write→err（审计含 size）
4. **共享语义实证**：两插件（同一 runtime 注册）A 写 B 读——**设计使然的文档化实证**（非 bug 回归）

## 6. 停手边界

- **不做** per-plugin namespace（L1+ 台账）
- **不做** 用户笔记库访问（另行立项）
- **不做** d2 host-invoked 容量 guard（T9 台账）
- **不做** 内容全文搜索（query-docs 仅键检索）
- payload WIT 强类型化（变体/record）不做——string 确定性优先

## 7. 台账增量

- **T9**（新）：d2 host-invoked write-doc 容量 guard 对齐
- **14c.4+ 候选**（新）：per-plugin namespace（L1 数据隔离）
