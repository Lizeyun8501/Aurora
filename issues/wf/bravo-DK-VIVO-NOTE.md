# bravo-DK-VIVO-NOTE — 原子笔记对标·笔记编辑功能完善回执

**执行**: bravo ｜ 2026-11-01 ｜ 派单: TA（VIVO 原子笔记调查+图示 Aa 样式面板）

## 调研结论（公开资料+TA 截图 VLM 识图双源）

原子笔记编辑页核心 = 底部「Aa」文字样式面板（四 tab:
文字样式/符号/分割线/图文模板）+ 富文本（**文字颜色/字号调节**/
加粗高亮）+ 待办清单 + 表格。Aurora 已有 strong/em/underline/
strikethrough/code/highlight/task_block/table/HR——**差距=文字色/
字号双缺 + 高亮 schema 有 UI 无 + Aa 预设面板无**。

## 交付面

| 模块 | 落点 |
|---|---|
| schema | fontColor mark（7 色板 attrs）+ fontSize mark（S/M/L/XL 四档 px）+ tagMark（‹› 包裹）/quoteMark（“” 蓝斜体）——Aa 预设原子 |
| 工具栏 | A（红）/A⁺/▮ 三弹出组（色板/字号档/高亮色板+「无」removeMark 正法清除） |
| Aa 预设面板 | 六块（普通=清 mark/引用=blockquote/标记/引语/高亮/红强调=fontColor+strong 组合）——图对标主面 |
| 浮动菜单 | 选中即高亮▮+红色 A 快钮 |
| CSS | tb-pop/tb-swatch/tb-style-pop 面板系+伪元素装饰（‹›“”） |

## 验证矩阵

- vitest **18/18**（fontColor rgb 归一往返/fontSize px 往返/非 px 拒收/
  三 mark 正交叠加/旧文档零影响/tagMark class 往返）
- Playwright **7/7**（repro-vivo-note.js: 三弹钮/红字 rgb/20px/mark 标签/
  浮动快钮/Aa 六块+标记应用/零错误）
- tsc 双端 0;mobile build 10.6MB;r19 回归 **19/20**（⑤b 上移 swap
  为 c75a49d 历史遗留——本卡 diff 面不含 moveBlock/insertCommands,
  逻辑实证非本卡引入）
- cal01 回归 6/6 ✓

## 边界与红-line

- 符号 tab/图文模板 tab（原子笔记另两 tab）未做——v2 待拍板
- 段落拖拽交互（触控专属）未做——desktop 键鼠面不适用
- 语音/涂鸦/扫描（原生面）——Aurora 无此基建,出局

## 坑注记

- `span[color]` 属性选择器匹配不到 style 设色（jsdom/浏览器归一
  rgb(220,38,38)）——改 `span[style*="color"]`+getAttrs 拒空兜底
- PM marks 回读序不保（mark[0] 断言 flaky——find 断言稳）

——bravo 11-01 20:1x
