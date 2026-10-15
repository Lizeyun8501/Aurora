# DK-16.1 修复注记（A-4 逃逸面堵口）（2026-10-16）

**性质**: Alpha 直做（XS，拍板序①）｜ **验证**: 本地 wasip2——**单元 99/99（+3 新测）+ e2e 18/18 全绿**，e2e 零改动

## 一、修复内容（最小 diff：lifecycle +33 / wasm_runtime +30-1）

1. **lifecycle.rs**：`Plugin::new` 强签名 Err——id 含 `:` 拒（诊断含原串+DK 编号）。15a 先例路径，fail-closed 一致。
2. **wasm_runtime.rs**：`register` 保 `()` 签名，内部堵口——含 `:` **不入 manifests + error 留痕**（tracing::error）。后续 invoke 走 NotFound（不进 namespace 校验体系）。

## 二、设计裁决：为何 register 不 Result 化（诚实记录权衡）

- 初版曾做 register→Result 强签名，但 27 处调用点（e2e 20+单测 7）需适配；批量文本手术**两次翻车**（括号平衡扫描遇字符串字面量括号错位截断 1306 行→git checkout 恢复；正则对嵌套括号覆盖不足 5/27）——**教训已录**：批量文本手术必须备份+编译验证三防线，本改弃用。
- **最终裁决**：`()` 签名+堵口语义（不入库即拒绝+error 可观测）；强签名 `try_register` **随 L2 安装面接线统一收口**（届时调用方才需要感知 Err——现在全内控无外部输入面）。lifecycle 侧已是强签名（Plugin::new Result），安装面新增路径默认走该侧。
- **可辩护性**：A-4 逃逸面实质堵死（非法 id 无法进入 namespace 校验体系），观测面有 error 留痕；强签名收口点已挂 DK-17 明示。

## 三、新测三例

- `plugin_id_with_colon_rejected_in_lifecycle`：Plugin::new 拒+诊断含原串与 `':'`
- `plugin_id_nested_colon_rejected`：`a` 合法+`a:b` 拒（**逃逸组合面直接断言**）
- `test_register_colon_id_rejected_silently`：register 不入库+合法 id 正常入库

## 四、状态

- **DK-16.1：完成关账**；A-4 关闭
- **DK-17 L2 设计稿已出**（随档派发）：share 只读授权+accept_share opt-in+A-1 CallerCtx 接口位+A-5 init 传 self-id
- 待办：Bravo 领 DK-17（M）
