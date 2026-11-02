# bravo-NOTEBOOK-OPT-E2 — Aa 面板五合一·bug 整改·回执

**执行**: bravo ｜ 2026-11-02 ｜ 派单: TA 07:43（图示 Aa 点击→下方出全部文字样式,如加粗等）

## TA 报 bug 语义核验（VLM 三读）

VIVO Aa 一钮弹**全部**文字格式: B/I/U/S/高亮+字号档+色板——
Aurora 现状=5 独立钮（A 色板/A⁺ 字号/▮ 高亮各自为弹+B/I/U/S 散平铺（**且 MARK_BTNS 中 B/I/U/S 平铺钮在桌面端残留）——非图示模式。

## 交付面

| 项 | 内容 |
|---|---|
| Aa 五合一 | A/A⁺/▮ 三独立弹组**删除**;新「格式」tab 首位并入 Aa 面板 |
| 格式 tab | B/I/U/S 四钮行+字号档 S/M/L/XL+文字色板 7 色+高亮色板 6 色四段流式纵排 |
| tab 构成 | 格式｜文字样式｜符号｜图文模板（四 tab——VIVO 图层对标） |
| active 态 | Aa 钮高亮=七 mark 任一命中;残留独立钮清零断言 |
| CSS 修 | `.tb-pop` 在 fmt 面板内覆流式（`position: static`）——**修互叠拦截真 bug**（三色板 absolute 同位叠=指针事件被顶层子树拦截,即 TA 所报 bug 本体） |

## 验证矩阵

- PW **12/12**（① 五合一结构断言: 四 tab+格式钮 4+字号 4+色板 13+独立钮残留 false;②③④ 色板/字号/高亮应用链;⑤-⑩ 既有断言全保;⑥ 零错）
- mobile tsc 0 + build 31s ✓
- shared tsc 5 错=既有测试文件基线错（stash 验证非本次引入）

## 坑实录

- **tb-pop 复用陷阱**: 独立组的 `.tb-pop` 样式是 absolute 弹层——并入 fmt 面板后三组色板**同位叠**互相拦截 pointer events（PW 报「subtree intercepts pointer events」）——覆写 `position: static` 流式化即修
- PM 选区: DOM Range 在 PW 下被 PM 失焦重建吞——`window.__editorView` 暴露通道+`TextSelection` 直驱（RichEditor 挂/卸对称清理）
- Aa 钮 toggle 语义: 测试脚本连点防御（ensureAaOpen util——面板已开时直入不重开）

## 剩余

- MARK_BTNS B/I/U/S 平铺残留系桌面 EditorToolbar 旧渲染——本卡未触（mobile 面已清）
