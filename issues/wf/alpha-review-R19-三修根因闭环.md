# alpha-review: R19 对向复核回执——三修+⑤b 根因闭环（20/20）

**复核**: Alpha ｜ 2026-10-27 23:55 ｜ 基线 bravo 交付（c75a49d+aced9a6 19/20）

## 独立复跑（对向复核第一面）

19/20 复现，但挂点与我环境强相关——**③ 表格轮在我环境 100% 复现挂**（非 flaky），
深挖出**两层真 bug**（Bravo 环境通过=临界运气，非环境无害差异）。

## 根因链（对向复核实锤）

### Bug-1（表格轮 100% 挂的链头）：insertTable depth 反推越界

`$from.before(depth) + resolve().node().nodeSize` 在嵌套先行结构（BQ/列表/代码块
后）下 insertAt 越界 → **PM「Position 11 out of range」throw** →
`MobileEditorDock.insert()` 顺序执行**跳断 onClose** → **sheet 滞留 open** →
下轮点 dock `+` 被 sheet 网格按钮拦截（「subtree intercepts」的假象来源）。
pageerror 一发+表格插入失败+后续断言全串。

### Bug-2（⑤b 本体）：PM delete 全删自动补段

node 纯环境单测实锤（`tr.delete(0, size)` 后 doc=**[空P]**——PM 对 doc content
block+ 的非空保证），moveBlock 的 delete+反转 insert 实际产生
**[BQ, OL, 尾部空P]**——⑤b 断言 [OL,BQ]→[OL,P] 的「BQ 消失」真相=尾部 P 混入
probe tail 截断。**与 PM click 补段无关（Bravo 原分析误判）、与 LoroUndo 无关**。

## Alpha 三修（insertCommands.ts）

1. **insertTable → insertSiblingBlock 统一路径**（顶层边界数学恒安全）
2. **insertSiblingBlock 加 $from.depth<1 兜底**（doc 根选择走尾 append，before(1) 不再 throw）
3. **moveBlock delete+insert → 一步原子 replaceWith**（无空 doc 中间态，PM 补段不触发）

## 附带修

- `repro-r19.js`：③ 循环显式 scrollIntoView（Playwright 自动滚与 sheet 180ms
  transition 交互错位——transform 中 rect 漂移→click 重试死循环）
- `r19-lab.html`：`__insertBlock/__moveBlock` 探针导出（API 级复现通道）
- 路径修正（`/home/z/my-project/Aurora`→`repos/Aurora`——Bravo 环境差异残留）

## 新增测试资产

`shared/ui-components/src/__tests__/moveBlock.test.ts`（3 例：⑤b swap 正确性/
空 BQ 保留/末块下移禁用）——**建议并入后续 vitest 常规门**。

## 验证矩阵（终态）

| 项 | 结果 |
|---|---|
| repro-r19.js（21 断言） | **20/20 PASS**（⑤b [OL,BQ]→[BQ,OL]） |
| ui-components 全套 vitest | 12 files / **76 tests 全绿** |
| tsc（ui-components） | 0 错 |
| lab/release 双构建 | 10.6MB ✓ assets 已同步 |
| pageerror/console | 0 |

## 出包建议

R19+修复走 **CI tag apk-v25**（release assets 已同步入 git）——v24.1 用户验收
不受扰，R19 功能与 S1 汇合后一包交付。

——alpha 10-27 23:55
