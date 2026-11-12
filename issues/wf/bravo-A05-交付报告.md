# A-05 交付报告：块引用（transclusion）+ 页面面包屑

> 执行: Bravo ｜ 工单: TA 2026-11-12 08:23 ｜ 终 commit: 28cb435

## 首节 · 开工令假设与代码实况披露

| # | 开工令假设 | 实况 | 处置 |
|---|---|---|---|
| 1 | 「块 id 数据面已就绪」 | NoteDoc blocks 树有 block_id UUID 但 **PM 编辑器面不填充该树**（编辑器走 loro-prosemirror 同步,块无稳定 id） | **锚点=heading 文本**（`((noteId#标题))`——Obsidian 章节锚语义）;heading 承载锚点是数据现实下的诚实设计,block_id 全链填充登记 v2 |
| 2 | 「EventBus Low 通道增量刷新」 | 事件字典契约冻结:DocumentChanged/BlockChanged 均为 **Medium**（唯一消费者投影环路）;Low 通道无文档内容事件,改通道需 ADR | v1 走**保存路径 emit 桥**:cmd_update_note/cmd_save_note_snapshot 广播 Tauri `aurora:doc-updated`（多窗口 A-10 场景 1s 内刷新——主场景即达）;桌面 NodeView 订阅匹配 noteId 重解析,异步不阻塞编辑 |
| 3 | 「双链投影解析」 | backlinks_of 为标题反链倒排——解析 (noteId,anchor) 的正确通路=目标笔记单点读取 | 单点读+core `resolve_section` 内存切片——**零全库扫描**（万级库不卡顿语义达成） |

## 实现清单

### core（单一递归源——双端共用 VM）
- `l3_domain/block_ref.rs`: `breadcrumb_of`（parent_id 上溯/hop16/环守护/缺父宽容）+ `resolve_section`（heading 锚点→同级截断章节/无锚点 3 块预览）——**单测 3/3**

### 桌面
- `block_ref_commands.rs`: cmd_resolve_block_ref（三态 **ok/not_found/locked**——锁定≠失效语义分离;backlink 样板对齐含 legacy 明文回退）+ cmd_breadcrumb
- 保存路径 emit: 两保存命令加 AppHandle+广播（多窗口增量刷新）

### 移动 FFI+Java 桥（五层链全落——DK-02 S2 教训纪律）
- mobile-ffi: `block_ref_json`/`breadcrumb_json` + JNI 双导出（cargo check ✅）
- UniffiAppCore.java: blockRef/breadcrumb+native 声明 ｜ MainActivity: @JavascriptInterface 双方法
- androidPlatform.ts 桥接口+MobileApp: resolver 注入+fetchBreadcrumb+跳转链（NoteEditor onOpenNote→setEditing）
- **Java 面验证**: E25 javac 聚焦验证法——增量面签名编译实证（FOCUS-RC=0+运行至 UnsatisfiedLinkError=签名解析达链接期）+三面 rg 核对（Java native 声明↔Rust JNI 符号名↔TS 接口）

### shared
- schema: block_ref 原子节点（role=blockquote）;auroraMarkdown: `((id))`/`((id#锚点))` 解析+发射
- blockRefNodeView: 三态嵌入卡（源标题+章节预览+跳转源块/引用已失效/锁定/未接入提示——无空白无崩溃）+resolver 模块注入+doc-refresh/open-note 双事件桥
- NoteBreadcrumb: nav aria-label="位置"+**>4 级中间折叠「…」**（首尾各留 2）+各级可点+末级 aria-current
- RichEditor fetchBreadcrumb prop（宿主注入,未注入不渲染）;DesktopShell 全挂（resolver+Tauri→DOM 桥+路由）

## 验证矩阵
- 本机: core 单测 3/3;shared a05 测试 **7/7**（面包屑三态+折叠/引用 md 往返+混排不丢型）;clippy workspace --all-targets RC=0;fmt RC=0;双端 tsc 零错
- CI（28cb435）: **desktop-check ✅ / Clippy ✅ / Rustfmt ✅ / MSRV ✅ / Wasm×3 ✅**
- Test: 唯一 FAILED=dk02_s2_trash_flow_roundtrip——**四卡一致基线红**（A-01/02/03/05 同名同案分离）;A-05 面零失败

## DoD 对照
- ✅ 块引用跨笔记可用（`((noteId#heading))`;源块修改后引用处 1s 内更新（保存路径广播——多窗口主场景;同窗口单编辑器互斥场景挂载期解析覆盖）
- ✅ 失效引用明确视觉态（「引用已失效」/「锁定」/「未接入」三态分流——无空白无崩溃）
- ✅ 面包屑 4 层以上折叠「…」、各级可点
- ✅ 无障碍: nav+aria-label="位置" / 引用卡 role="blockquote" / 末级 aria-current="page"
- ⚠️ 诚实边界: 移动端刷新=v1 挂载期解析（Tauri 事件面移动无桥,增量刷新事件桥登记后续卡）;PM 块稳定 id（NoteDoc block_id 编辑器填充）登记 v2

## 停手边界遵守
不做循环引用检测之外的图算法 ✅（breadcrumb 环守护=防御非图算法）/ 引用块双向编辑不做（v1 只读——NodeView 无 contentDOM）✅
