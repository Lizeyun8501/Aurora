# DK-14c.3 开工令 · 数据面 guest-invoked（设计稿→实现）

**派发**: Alpha 2026-10-14 10:45 ｜ **Assignee**: Bravo ｜ **Estimate**: 3-4h ｜ **依据**: `issues/wf/DK-14c3设计稿-数据面guest-invoked.md`（**实现以设计稿为准，冲突时停手报告**）｜ **依赖**: 14c.2（c136b2c 已关账）

## 任务摘要（细则见设计稿）

### 1. WIT：capability 接口加三自由函数（0.2.0 内）

`read-doc(token, doc-id) -> option<string>` / `write-doc(token, doc-id, data) -> result<_, string>` / `query-docs(token, pattern) -> list<string>`——形态复用 14c.2 裁决（自由函数+token borrow；payload=string）

### 2. 宿主：func_wrap 三函数+Storage require+差异化审计

- require(Storage) 统一；越权 `CapabilityDenied(Storage)`+审计 `reason=data-denied`（含 method，逐条不节流）
- write-doc：1 MiB guard→insert→**逐条审计**（doc_id+size）；read/query：首调+越权
- docs 共享空间直通（与 host-invoked 同一 `RwLock<HashMap>`）

### 3. e2e 四例（设计稿 §5 契约）

granted 全链写读查 / 越权拒 / 容量 err / **共享语义实证**（两插件同 runtime A 写 B 读——设计使然，注释+文档双披露）

### 4. 文档

日志约定文档+README 数据语义节：共享 KV 语义披露+审计策略说明

## DoD

- [ ] 三函数 wit 编译过+wasm-tools import 面实证；CI wasm job 合约断言同步
- [ ] require+差异化审计+1 MiB guard 绿；e2e 四例全绿
- [ ] 共享 KV 语义文档化（文档+README）
- [ ] CI 八门绿+WASI 收紧零回退；独立 commit（可拆 feat+fix）直推 main

## 台账登记（报告内列明即可，无需单独文件）

T9（host-invoked 容量 guard 对齐）/ 14c.4+ 候选（per-plugin namespace）

## 停手边界

设计稿 §6 全部适用；**实现与设计稿冲突时停手报告**（本卡是数据面首次开放，裁决权在设计稿）
