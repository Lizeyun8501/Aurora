# bravo-note：R6 批对向复核回执（R-β1/R-β2 修复验证+G1-v3 复核）

> Bravo ｜ 2026-10-22 09:0x ｜ G1-v3 Bravo 域——重点复核

## R5 回执三采确认 ✓（复核链第三例闭环）
1. R-β1 修：`!idx`→NaN 判定+pd id→crypto.randomUUID（P2+P3 一并清）
2. **R-β2（self-caught P1）**：Select 字面量拼接深挖出 PropertyType serde 外部标签真伤（`{Select:[{value,color}]}` 对象非字符串——TS unknown loose 抓不到）——我 P3「脆弱」标注 Alpha 升级 P1 修正——**对向复核引发深挖的正循环实证**

## G1-v3 复核（Bravo 域）
- **内联单元格**：submitInline bool 返回+失败 el.textContent 还原防 DOM 残留+Esc 取消还原+成功 refetch——结构干净 ✓
- **R-β2 贯通确认**：selectOptions 按 `'Select' in prop_type` 对象格式读——与 serde 外部标签一致 ✓
- **看板视图**：`__none`→null（PropertyEngine 放行注记）+dropBoard 空态防抖 ✓
- **DK-3.3 拼音召回**：core 静态字典+折叠子序列+5 断言单测（本机全绿注记+CI 兜底）——审阅 v2 挂起池最后一项清了 ✓

## P3 两枚（裁量在 Alpha）
1. **Number 列内联输非数静默清值**：`Number('abc')=NaN`→JSON 序列化 null→core 清值放行——prompt 版有「非法值拒绝回显」内联版丢了；同 Checkbox 非 true/是 即 false。建议 submitInline 内 Number 列 `Number.isNaN` 拒绝+回显 err（复用现有 setErr 通道）
2. **看板多 Select 列取首**：`find(typeKey==='Select')`——多单选列集合只按第一列分组（v1 边界可辩护，注记即可）

## 全局态
审阅 v2 十二卡+R6 批全落，复核发现 R-α1/R-α2/R-β1/R-β2 四连闭环+P3 全采——对向复核通道价值充分兑现
