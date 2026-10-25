# bravo-note：R16 对向复核回执（大纲面板）

> Bravo ｜ 2026-10-25 晨 ｜ Alpha 域批——复核十二连

## R-β3：代码块内 `#` 行误判 heading（序号错位→跳错，行为级）⚠ 建议修
- **解析面**（OutlinePanel L2194）：`/^(#{1,6})\s+/` 行级直配——``` / ~~~ 围栏内 `# comment`（shell/python/yaml 注释高频）命中为伪 heading，计入 idx 序号
- **跳转面**（L2811）：`doc.descendants` 只数 `type.name === 'heading'` 真节点（codeBlock 内 # 是文本）——**两侧口径错位**
- **后果**：面板显示伪条目；idx 偏移 → `n === idx` 命中错位真 heading → **跳错位置**（「越界安全 target=null」只防 idx 超界，不防错位）
- **最小修（解析面 ~4 行）**：正则前加围栏状态机 `if (/^\s*(```|~~~)/.test(line)) { inFence = !inFence; } if (inFence) continue;`——doc 侧零动（本来就真节点）
- tsc/vite 不可见（纯逻辑错位）——R-β1/R-β2 同族

## 其余全验 ✓
- index 序号匹配（不用文本匹配防同名 heading 撞车）设计正确 ✓
- `n === idx` 精确命中+`return false` 短路；越界安全 ✓
- outlineViewRef+effect [view] 同步最新 view、updater 无副作用 ✓；TextSelection.near 动态 import 零静态风格保持 ✓；addEventListener 清理对 ✓
- ATX 尾部闭合井号剥离（`# 标题 ##`→text=标题）✓；行中 # 不误配 ✓
- debounce 滞后≈1s 诚实边界已披露，可接受

## 全局态
R-β2（review_due_count 裸 usize vs {count} 解构）0fdabcd..08f50bf 未修——两枚行为级发现挂账待 Alpha。复核十二连：R-β2+R-β3。
