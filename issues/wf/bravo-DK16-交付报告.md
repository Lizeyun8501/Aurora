# Bravo DK-16 交付报告 — 静默占位清除（OCR / 向量）

> 基线 126960f · 工具链 cargo 1.91.0 · CARGO_INCREMENTAL=0 全程

## 一、删除清单

### 占位二：向量库死代码（先行，284 行）
- 删 `crates/aurora-core/src/l1_infrastructure/vector_db.rs`（四方法全 TODO 返回空的死实现）
- 删 `l1_infrastructure/mod.rs` L17 `pub mod vector_db;`
- 删 `Cargo.toml` `# Vector database` + `# lancedb = { workspace = true }` 注释块

### 占位一：OCR mock（947 行诚实化改造）
- **删**：`mock_recognize`（哈希派生伪文本）全函数 + 2 处 provider 调用点 + `simple_hash` 的识别派生用途（保留：预处理元数据启发，注释精确化为「启发式内容指纹，不派生识别结果」）
- **删**：`FormulaRecognizer` 空文本硬编码默认公式 `"E = mc^2"`（伪造结果——改 `Err(OcrError::NoRecognizableContent)`）
- **删**：`process_one` 的文本前缀 `"[Paddle]"` 推断 engine_used（伪造依据——改语言选择语义推导）
- **措辞清理**：预处理/表格重组的「mock」注释 → 诚实声明（元数据推断 ≠ 识别结果；网格重组是通用逻辑）

## 二、诚实化 API 变更（NotImplemented + 接口预留）

| 项 | 变更 |
|---|---|
| `OcrError` | 新增枚举：`NotImplemented{engine}` / `NoRecognizableContent`；Display 引用错误码字典 **D07**（「此功能需要 OCR 引擎」，FallbackAction::NotImplemented——变体已预登记，本次零新增） |
| `OcrProvider::recognize` | `Vec<OcrTextLine>` → `Result<Vec<OcrTextLine>, OcrError>`；Paddle/Tesseract 实现体 = `Err(NotImplemented)` + 挂载点注释 |
| `OcrEngine::recognize` | Result 化；primary NotImplemented 时仍尝试兜底引擎（前瞻：可单侧先接入真实实现） |
| Table/Formula/OcrService 三入口 | 全部 Result 化传播不吞 |
| `BatchOcrProcessor::process_one` | Result 化；**失败计入 `failed` 字段**（该字段自此有真实语义），失败项不进 `results` |
| `capture_matrix` | `ScreenshotResult` + `ocr_error: Option<String>` 字段；OCR 失败时截图成功、`ocr_text` 空、错误显式记录；`screenshot_to_document` 区分「识别无文本」vs「OCR 不可用(D07)」降级块——**不进任何索引/伪造内容** |

**feature-gate 预留的形式**：接口层预留（`OcrProvider` trait 可插拔 + `NotImplemented` 错误语义 + 模块文档挂载点说明）；Cargo feature 与原生依赖在真实引擎接入卡引入——**空 feature 是假信号，不做**。

## 三、DoD 验证矩阵（可机器验证）

| DoD 项 | 结果 |
|---|---|
| `cargo test -p aurora-core --all-features` | ✅ **422 passed / 0 failed**（EXIT=0） |
| `cargo clippy -p aurora-core --all-targets -- -D warnings` | ✅ EXIT=0（零告警） |
| `rg "mock_recognize\|基于.*哈希派生\|0\.85" ocr_service.rs` | ✅ 零命中 |
| `vector_db.rs` 不存在 | ✅ |
| `rg "vector_db\|LanceDb" crates/` | ⚠️ **显式偏差**：`l2_engines/query.rs` 6 处保留（见降级项 1） |
| 新增 NotImplemented 测试（禁 Ok） | ✅ 9 个新测试：双 provider/三语言+空图/Display 引用 D07/Service 三入口/批量 failed 计数/percent 语义 |
| capture_matrix 失败路径不 crash | ✅ `test_screenshot_capture_ocr_not_implemented_degrades`（截图成功+无伪文本+错误记录+边界正常） |
| 错误码字典登记 | ✅ 复用既有 `ErrorCode::D07`（语义精确匹配，零新增变体） |
| workspace `cargo check` | ✅ 默认集 EXIT=0（见降级项 2 环境分工） |

## 四、降级项 / 偏差显式标注

1. **`query.rs` 6 处 LanceDb 保留**：DoD rg 该条按字面无法满足——`l2_engines/query.rs` 的 LanceDb/vector 调度是**活跃查询语义**（`nl_query.rs` 依赖 QueryEngine；LanceDb 执行路径 today 不可达但 is_applicable/estimate_cost 逻辑活跃），删除会破坏检索正确性。**裁决**：保留调度面，死实现（vector_db.rs）清零——与任务书「YAGNI 裁决」精神一致（删的是零调用死代码，不是接口）。traits/vector_store.rs 同理保留（DK-03 检索域接口预留层）。若 Alpha 判定需连调度分支一起清，另立小卡（涉及 nl_query 适配）。
2. **workspace check 环境分工**：`cargo check --workspace` 含 tauri 桌面端 → 本机无 GTK3 dev 库不可达（gdk-sys pkg-config 失败，S4 冒烟 WebKitGTK 栈级定性同族）。按 **Cargo.toml default-members 既定注释**（headless 环境不编译桌面端，桌面端验证在装齐系统库的开发机/CI）执行默认集 check 绿；全量 workspace check 以 **CI Run 为准**（本报告 push 后复核）。
3. **中文 OCR 准确率 DoD**：真机/真引擎环境缺失（本次清理的正是伪实现）——**挂起**，随真实 PaddleOCR 接入卡兑现。
4. **测试量净变化**：旧 mock 断言测试删除，新增 9 个诚实化测试（总数 422 含全部既有回归，DK-09/DK-08 系列全数 ok——`attachment_store::tests::dk09_attach_delete_cascade_index` 等名单逐一确认，无 feature 门控静默吞测试）。

## 五、验证输出摘要

```
CARGO_INCREMENTAL=0 cargo test -p aurora-core --all-features
  → test result: ok. 402 passed; 0 failed (lib) + 5 (doc) + 15 (integration) — EXIT=0
CARGO_INCREMENTAL=0 cargo clippy -p aurora-core --all-targets -- -D warnings
  → Finished, EXIT=0
CARGO_INCREMENTAL=0 cargo check (default-members)
  → Finished, EXIT=0
rg "mock_recognize|基于.*哈希派生|0\.85" crates/aurora-core/src/l3_domain/ocr_service.rs
  → (no output) 零命中
```

**DK-16 交付完成，等 Alpha 复核。**

— Bravo 2026-09-27

---

## 六、CI 双红修复与终验（7327c44 · RUN 36299313586 ✅ SUCCESS）

首轮 push（61b3df6）CI 双红，复盘修复：

| 红 job | 根因 | 归属 | 修复 |
|---|---|---|---|
| Rustfmt | DK-16 新测试 `matches!` 宏不合 rustfmt | Bravo 本次 | `cargo fmt --all`（7327c44）；**教训：本地验收门槛升级为四项（fmt --check 加入三门槛）** |
| desktop-check | 16c7f37 起 dist 退追踪 → 干净 clone 无 frontend 产物 → tauri generate_context 校验 frontendDist 失败 | **D1 小卡漏项**（退追踪前只审了 workflows 文本引用，未审 tauri-build 运行时行为） | desktop-check job 前置 setup-node + npm ci + vite build + 产物探活（7327c44） |

**终验 RUN 36299313586 五 job 全绿**（Rustfmt/MSRV/desktop-check/Clippy/Test）——desktop-check 绿即 **D1「dist 不入仓」模式在 CI 干净拉取视角闭环实证**。历史红灯 run（16c7f37/64f4ffd/126960f/03c4c98）均因同一 desktop-check 断链，随本次修复全部消解。

— Bravo 2026-09-27（CI 终验补记）

---

## 六、Alpha 集成复核（用户指令 2026-09-27 14:29）——✅ 通过

### 独立验证矩阵（不复读回执）

| 验证线 | 结果 |
|---|---|
| rg 清场断言 | ✅ `mock_recognize`/`E = mc`/`0.85`/哈希派生伪文本 **零命中**；`vector_db.rs` 已删；仓内 LanceDb 残留仅 query.rs（见裁决①） |
| NotImplemented 真路径审计 | ✅ `OcrEngine::recognize` L186-215：primary Err→fallback Err→**如实传播 primary 的 D07**，无伪成功分支 |
| simple_hash 暗处保留嫌疑 | ✅ 现用途=ImagePreprocessor 元数据（skew/layout 启发），不产出文本行、不进索引；与回执披露一致 |
| failed 字段真实语义 | ✅ `process_one`：Err→`failed += 1` + 显式报错（L561-568） |
| 半接入排查（D1 教训） | ✅ `execute_lancedb` L852-856：无 store 注册→`Err(InvalidInput)`——调度面保留**无残留伪成功** |
| 全量测试独立佐证 | ✅ CI `RUN 36299313586 @7327c44 = success`（五 job 全绿，含 fmt 双红修复） |

### 偏差裁决（四项全部采纳）

1. **query.rs 调度面保留**：合理——调度枚举活跃、执行面无 store 时显式报错，与「YAGNI 删死代码」不矛盾；无需另立小卡；
2. workspace check 环境分工：符合 Cargo.toml default-members 既定注释，全量以 CI 为准 ✓；
3. 准确率 DoD 挂起：符合任务书降级条款，显式标注 ✓（随真实 PaddleOCR 卡兑现）；
4. 测试净变化：CI 佐证 ✓。

### 复核备注

- Alpha 本机全量复跑因环境磁盘满（target 18G/30G 盘）SIGBUS 中断——已切换 CI 独立背书完成验证；本机磁盘大 workspace 全量验证不可行，后续以 CI 为准（教训入 MEMORY）；
- DK-16 状态：**交付验收通过**，卡关闭。V26 P0 剩余：DK-05M 移动编辑器（Android 面）。

— Alpha 复核 2026-09-27
