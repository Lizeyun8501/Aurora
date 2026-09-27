# Bravo 任务书 — DK-16 静默占位清除（OCR / 向量）

> 派发对象：Bravo 工作流 agent
> 生成：2026-09-27 · Repo: 本仓 main · 基线 commit: 126960f
> 派发人：Alpha（集成复核方）· 用户指令「按建议推进」
> 优先级：**P0**（静默欺骗类产品质量债）· Estimate: 5–8 人日（诚实化主体 3–5 + 引擎接口预留 2–3）

---

## 1. 背景与证据（实地审计 2026-09-27，行号为当前 main）

铁律 7 已扩展为**「禁止伪成功」**：任何返回成功语义的分支，必须对应真实发生的副作用。以下两处违反：

### 占位一：OCR mock（947 行，伪文本危害最高）

- `crates/aurora-core/src/l3_domain/ocr_service.rs`
  - L7/L10：模块注释自述「各 provider 实现为 mock」
  - L67 `PaddleOCR Provider（mock）` / L88 `Tesseract Provider（mock）` → 均调 `mock_recognize`（6 处引用）
  - L108 `mock_recognize`：**基于图像内容哈希派生确定性伪文本**，confidence 0.85+ —— 能进搜索索引、能被用户当真实识别结果引用
  - 表格识别 mock：网格化文本重组

### 占位二：向量库死代码（284 行）

- `crates/aurora-core/src/l1_infrastructure/vector_db.rs`
  - L56 add / L71-72 search / L77 delete / L93-94 hybridSearch —— 四方法全 `TODO` 返回空
  - `Cargo.toml` L18：`# lancedb = { workspace = true }` 已注释 —— **无依赖死代码在假装工作**

### 调用面（Alpha 已核实，零波及/最小波及）

- `LanceDbStore`：**零外部调用方**（仅 vector_db.rs 自身 + `l1_infrastructure/mod.rs` L17 re-export）→ 删除零波及
- `OcrEngine`：apps/ 层零消费；crate 内部调用方仅 `l3_domain/capture_matrix.rs`（L32 use + 截图 OCR 复用）与 `l3_domain/sublayers.rs`（L52 re-export）

---

## 2. 方案裁决（Alpha 裁决，勿自行变更方向）

### 裁决一：OCR = 诚实化 + feature-gate 接口预留（不在本卡接真引擎）

1. **删除全部 mock 路径**：`mock_recognize`、哈希派生、表格重组、两个 mock provider 本体全部移除；
2. 无引擎（feature 未启用）时 `recognize` 返回 **`OcrError::NotImplemented`**（新增错误变体，62 错误码字典同步登记），**禁止返回空成功**；
3. provider trait 与 `OcrTextLine/BoundingBox/OcrLanguage` 数据结构保留（接口契约面）；真实 PaddleOCR 以 `cfg(feature = "ocr-paddle")` 预留骨架（编译隔离，未启用走 NotImplemented）；
4. `capture_matrix.rs` 调用方适配：NotImplemented **传播不吞**、不 crash 主流程（截图链路 OCR 失败仅记录）；
5. OCR 结果**不进搜索索引**，直至真实引擎接入（文档注释中显式声明此决策）；
6. 卡内 DoD「中文准确率 ≥90%」在无真机验证环境下**无法验收** —— 本卡降级标注为「接口就绪 + 文档诚实化」，真实引擎验收挂起至有验证环境。**此降级写入交付回执，不得静默省略**。

### 裁决二：向量库 = 删除（YAGNI）

1. `vector_db.rs` 全文件删除 + `l1_infrastructure/mod.rs` L17 mod 声明删除 + Cargo.toml L18 注释行删除（不留尸体）；
2. 零调用方已核实，无需适配；如编译揭示隐藏引用，按 NotImplemented 策略适配并回执说明；
3. sqlite-vec 真实实现**不做**——生产链路当前无向量检索消费者，待 DK-03 检索域确有需求再立卡（回执中记录此决策）。

### 半接入警告（D1 教训，必读）

- 接入类改动必须审计**新逻辑 vs 既有检查的执行顺序**；
- 一切「已实现」断言必须在**无本地产物/无缓存的验证路径**下成立；
- `rg` 输出与 `ls-files` 输出勿混淆（D1 回核教训）。

---

## 3. 验收 DoD（可机器验证）

- [ ] `cargo test -p aurora-core --all-features` 全绿
- [ ] `cargo clippy --all-targets -- -D warnings` 零告警（CARGO_INCREMENTAL=0）
- [ ] `rg "mock_recognize|基于.*哈希派生|0\.85" crates/aurora-core/src/l3_domain/ocr_service.rs` 零命中
- [ ] `ls crates/aurora-core/src/l1_infrastructure/vector_db.rs` 不存在；`rg "vector_db|LanceDb" crates/` 仅剩回执/清单文档引用
- [ ] 新增测试：无 feature 下 `recognize()` 断言返回 `NotImplemented`（禁止 Ok）
- [ ] capture_matrix 既有测试适配后全绿，OCR 失败路径不 crash
- [ ] 错误码字典登记 `NotImplemented` OCR 变体
- [ ] 全 workspace `cargo check` 绿（CI Run 门禁同步绿）

## 4. 工作流规约

1. 直接 main 提交，前缀 `fix(DK-16): ...` / `feat(DK-16): ...`；
2. 本地验证铁律：clippy --all-targets + `CARGO_INCREMENTAL=0` 全绿才可 push；
3. 交付回执写入 `issues/wf/bravo-DK16-交付报告.md`（含：删除清单、NotImplemented 测试证据、降级项显式标注、验证输出摘要）；
4. 回执 push 后通知 Alpha 复核；复核通过前卡状态=待验收。

— Alpha 派发 2026-09-27
