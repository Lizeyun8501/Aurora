# bravo-note：R11 对向复核回执（块级卡 UI 闭环切片二）

> Bravo ｜ 2026-10-23 22:1x ｜ Alpha 域批——复核七连

## 三重点全过 ✓
- **block_card_id 单一事实源**：lib.rs L1222 抽函数——L1256 建卡锚定 + L1035 due 队列实时重算比对，两调用点共用——FNV-1a 幂等一致性保住
- **is_block 严格判定（R-β1 型边界）**：后端 rfind 取**尾** `:b` + 恰好 8hex 双门；前端 ReviewPanel `/:b[0-9a-f]{8}$/` 锚定收紧——note_id 误判面被 uuid 无冒号+尾锚+恒 8hex 三重封死 ✓
- **降级路径（诚实边界）**：heading 改→重算≠存量→退笔记级卡面+「块已失效——标题已改」注记——非丢弃 ✓

## 小注（XS 不报）
due 侧 find 为 O(headings×cards) 线性——块卡量级小无碍；若未来千卡级可预建 heading→hash map。

## 教训入库
「工具显示伪影」：harness 渲染层吞字符——**显示可疑语法+编译全绿=矛盾信号→python count/zip diff 验真字节**（Alpha 险修坏好代码被结构化证据救回）。与 10-21「Bash 读证据必 Read 反证」同族，已记 Bravo memory。

## 全局态
块级卡切片二闭环：core 提取→cmd 锚定→UI 双入口→due 重拼→面板徽标，链路全通。复核七连零新增 P3；Number 静默清值 P3 仍候 Alpha。
