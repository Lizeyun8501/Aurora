# bravo-note：审阅第二批三卡对向复核回执（G2/G3/G4）

> Bravo ｜ 2026-10-21 18:2x ｜ 背景：用户派「第二批开工」，pull 后发现 Alpha 已三小时连推全落（ab2248b/3b866e2/b486105）——撞车未遂（开工前 pull+grep 沉淀兑现），转复核哨兵位串审三卡

## 终案：三卡零阻断，全部干净

### G4 看板（3b866e2）
- **「事件溯源律：变更经统一实现」宣称核实 ✓**——apply_status 三调用点（定义/`set_row_status` 看板换列/事件 `TaskStatusChanged` apply 回放）共用一函数，stamp 语义零分叉；测试含事件路径 stamp 断言
- GTD 七枚举校验+幽灵行拒 ✓；`board_tasks` 全量读 v1 可接受（投影大时窗口化候 v2）

### G3 闪卡（ab2248b）
- `cmd_add_review_card`：note 存在校验拒 ✓；card_id=card:note:{id} 幂等重排 ✓；summary 首非空行 80char **char 截断**（中文友好——与 R-α1 byte 偏移对照，此处 Alpha 用 char，说明 byte/char 区分在能力内，R-α1 更可能疏忽）
- 首评 Good：建卡场景=用户正看笔记内容，Good=刚复习过——语义可辩护，卡面注记已在

### G2 journal（b486105）
- `/^\d{4}-\d{2}-\d{2}$/` 严格 0 填充+localeCompare desc+slice(10)+空态零噪音隐藏 ✓；「今日创建走今日视图入口链」边界诚实

## P3 级观察（不阻断，裁量在 Alpha）
1. G3 `cmd_due_review_cards` N+1 读（卡面逐条）——注记已在，到期队列小时可接受
2. G4 `board_tasks` 全量行——同上注记
3. R-α1（R3-1 span 字节偏移）仍未裁——本批复核加重裁量候选（char/byte 区分能力已实证）
