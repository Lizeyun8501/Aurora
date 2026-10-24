# bravo-note：R14 对向复核回执（Distill v2 列表项卡化）

> Bravo ｜ 2026-10-24 下午 ｜ Alpha 域批——复核十连

## 列表卡化防线全验 ✓
- **parse_list_card 三防线**：粗体起手（非 ** 直接 None）+非空术语（** 间 trim）+非空定义（中英冒号 ：/: 双收）——普通列表项/无冒号/空定义全挡 ✓；-/* 双列表符 ✓
- **:l hash8 与 :b 同族**：list_card_id 构造进提取器（block_card_id/list_card_id 全内联 extract_distill_cards）——建卡/due 重拼单一事实源 ✓

## id 双函数分工精确（非合一替换）✓
- **is_generated_card_id**（:b/:l 合一）→ sync 清理面：retain 布尔代数判定面扩宽，R12 四路语义保持、旧列表项改形态即失效出队 ✓
- **is_block_card_id 保留** → due 重拼按族分流（L441/L452 注释声明分工）——清理面与重拼面关注点分离，语义精确
- **due 统一 id 反查**：:b/:l 同提取集 find 命中即重拼+未命中失效降级注记——比逐族分流更简 ✓

## TS 侧互指 ✓
ReviewPanel :b/:l 双正则徽标（▸块 紫/▸项 青 8fd3c7）+L180 概念卡联合正则——三处与 core is_generated 语义互指 ✓
- block_index 死字段零残留（rg RC=1）✓；单测 id 边界（8hex/7位/非hex/笔记级）✓

## XS 备忘（设计权衡非缺陷）
heading 卡底空位时列表项被吃为块卡 A+同时独立列表卡=同内容双卡并存——heading 概念兜底×术语结构卡的隐含权衡，v2 选择性卡化意图内，无动作。

## 全局态
Distill 卡化两族（块+列表）收官。复核十连零新增 P3。
