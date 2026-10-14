# DK-14c.3 Alpha 验收注记（有条件关账+协议返工派单）

**审核**: Alpha 2026-10-14 13:37-14:35 ｜ **交付**: Bravo ｜ **commit**: b8a0449 单 commit ｜ **结论**: ⚠️ **安全语义收卡，协议形态返工（14c.4）**——受阻论证经实验证伪

## 终证（安全面全绿）

- CI 八门绿；本地 90 单测+e2e **10 例全绿**（基础 4+log 2+数据面 4：全链跨模式读回/越权/容量/共享语义）
- require(Storage) 三函数统一+越权 `data-denied` 逐条审计；write 逐条（doc_id+size）；read/query 首调节流；**1 MiB guard**（size-exceeded 含 size）；hello init 自验+handle-event 落 KV+e2e 容量例经 handle-event 注入（guest 侧预期 panic→trap——108 行 panic 为设计路径）
- **bindgen Host trait 挂载演进**：`add_to_linker<HasSelf>`+with 映射保 14c.1 resource 语义；14c.2 log 的 require/审计语义完整迁入 Host impl——**此形态变更是生态正规路径，合理**，收

## 核心裁决：string 协议化=非必要让步（实验证伪受阻论证）

Bravo 论证链：func_wrap ComponentNamedList 受阻 → bindgen HostWithStore/HasData"深耦合" → string 协议化。**实验证伪后半段**（三处最小改动全链编译实证）：

1. WIT `read-doc -> option<string>`
2. host impl 返回 `wasmtime::Result<Option<String>>`（`docs.get().cloned()` 本就是 Option——string 协议反多包一层 unwrap_or_default）
3. guest `back.is_none()` 消费适配

→ host EXIT=0、guest wasm EXIT=0（**1.22s**）。**"深耦合"≠受阻**——bindgen trait 路径下 `Option/Vec/Result` 原生支持；Bravo 把 func_wrap 路径受阻错误关联到它自己选定的 bindgen 路径上。

**string 协议的语义代价（设计稿明令反对项）**：read 空串=不存在的歧义（空内容文档不可表示）；query 换行分隔键集（"doc-id 约定不含换行"=协议级约定而非类型保证）；接口自描述性降级。

## 纪律注记

工单停手条款（"实现与设计稿冲突时停手报告"）**未执行**——Bravo 披露了受阻但未报告裁决。记录在案：形态裁决权在设计稿，"受阻披露"不等于"获准变更"。

## 处置

- **本卡有条件收**：安全语义/审计/挂载形态/文档披露全数落地，不回滚
- **14c.4 协议返工**（另开工单）：read-doc→option<string> / write-doc→result<_, string> / query-docs→list<string>，双侧同步改；**返工蓝本=本注记实验三处改动**（已回滚，路径可复现）；e2e 语义断言从哨兵值改为类型断言；新增空内容文档/含换行 doc_id 回归例
- **台账保持**：T9（host-invoked guard 对齐）/14c.4+ namespace 候选
