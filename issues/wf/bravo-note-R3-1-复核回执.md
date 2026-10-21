# bravo-note：R3-1 撞车让位 + 对向复核回执

> Bravo ｜ 2026-10-21 16:0x ｜ 背景：同卡同日双实现撞车——我 DK-3.4b（a85d632 未推）与 Alpha R3-1（0c4c1bc 已上 main）同方案同日落地
> **处置：Alpha 版已上 main 且总体更优——我 reset 让位全收；本卡转对向复核**

## 复核结论（0c4c1bc）

**总体判定：优于我的实现，采纳无保留**——
- due_parse 直接 Option<DueParse>（None=未解析）——语义比我 DueResolve{span 判别}+薄委托 span.map 干净
- 死分支「下周内」清理（v1 即被「下周」截胡不可达）——我保留了原序，此点 Alpha 优
- 时刻段尾空白回退对齐 trim 边界——比我的原区间（含空白）精细

## 复核发现（供 Alpha 裁量，非阻断）

**R-α1（P2）**：`DueParse.matched_start/matched_end` 注释为**字节偏移**——但注记称「前端标注就绪」。中文 UTF-8 3 字节/char，前端 `Array.from(text)` 切片按 code point 走——**「明天 9 点」span 字节 0..12，前端切片会越界/漂移**。chip 未用 span 故当前无害；**v2 框内 overlay 高亮落地时必踩**。修法二选一：a) core 转 char offset（`text[..b].chars().count()`）；b) 前端按 UTF-8 字节切（TextEncoder 对齐，绕）。**建议 a**（一次修，语义与直觉一致）。

**R-α2（P3，增强候）**：due_parse 内嵌 `Utc::now()`——同源零漂移对拍测试无法绝对断言（现测试仅相对/形态断言）。候续卡：抽 `due_parse_at(text, now)` 注入变体，测试可固定 now 断言精确值。

## 撞车根因与流程沉淀（自省）

我扫卡在 14:51（用户「新任务处理」），Alpha R3-1 稍后独立落地同卡——**开工前未 `git log --all --grep R3-1/预览` 查在途同名卡**。已沉淀：**审阅类路线卡开工前必 grep 审阅文档关键词**（R 编号/功能名双语），命中即停手等派单或复核姿态介入。

## 我的实现去留

a85d632 已 reset 丢弃（reflog 可溯）。其中与 Alpha 版正交的增量（char offset 语义/now 注入对拍测试）已在上文披露，采纳与否由 Alpha 定——不硬改 Alpha 刚落的文件重演撞车。
