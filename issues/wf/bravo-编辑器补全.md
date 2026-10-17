# bravo-编辑器补全 注记+交付（2026-10-17）

## 落实

1. **cmd_wiki_link_titles**（wiki_commands.rs 新文件）：scan note:*→unwrap_note_bytes（aab8997 通路——record 整包 vault 设备级 seal 恒解，笔记级锁定只密 content 字段，**title 恒明文可读**）→kind 排除 Folder/SmartFolder→trash:{id} marker 集对账排除→空标题跳过→去重排序；record 解析失败跳过非阻断
2. **浮层**（EditAuroraEditor 内，desktop 领地零触碰 ui-components）：光标前 80 字符窗最近 `[[`+其后至光标无 `]`/换行→弹层；子串过滤大小写不敏感（v1）；↑↓/Enter/Esc capture 路由+点击插入（onMouseDown preventDefault 保焦）；插入=替换 [[+前缀→[[title]]；失配/选择非空/闭合即关
3. 候选供给：EditorPane invoke 拉一次+切笔记刷新（容忍陈旧——新建标题下次进入编辑面入池，诚实边界）

## 诚实边界

- 浮层定位=容器顶部定条（coordsAtPos 追光标另批）——不遮正文足够达意
- 编辑既有 [[title|alias]] 链接不触发补全（卡面 v1 边界照办）
- 本机无 GTK：键盘导航/触发/插入交互未真跑（TS+check+CI 编译面兜底；代码级走查=三路关闭条件全覆盖）

## 验证矩阵

| 门 | 结果 |
|---|---|
| TS | 0 |
| fakepc DESK check | 0 |
| fmt 实跑+check | 0 |
| core clippy all-targets -D warnings（底面回归） | 0 |
| CI | 盯至全绿 |

## CI 终态（8b2e251）8/8 全绿 ✓ 2026-10-17

rebase 撞车处理：Alpha 在途 push 图谱原型（ece2e30——InfoPanelTabs 反链|图谱 Tab 重构 DesktopShell+lib.rs invoke_handler）——lib.rs invoke_handler 行冲突手解（双 cmd 并存）+DesktopShell auto-merge 后语义复验（tsc 0+fakepc check 0+wikiTitles 接线在位确认）。
