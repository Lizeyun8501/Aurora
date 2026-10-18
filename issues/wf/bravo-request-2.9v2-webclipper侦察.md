# bravo-request：DK-2.9 v2 Web Clipper 浏览器扩展——方案侦察（窗口切片）

> 派单方: Alpha ｜ 2026-10-18 ｜ 依据: 2.9 v1 本地剪贴板捕获桥已落（palette「从剪贴板捕获」——SharePack 复用 cmd_import_share 全纪律+#clipper 标签）+ 审阅定义「浏览器扩展+导入管线复用」
> 性质: **侦察报告非派单**——批复后按卡落实现

## 现状盘点（本机实测）

- **v1 已备**：剪贴板捕获桥（前端 readText→SharePack b64→cmd_import_share→#clipper 标签）——**扩展端可完全复用此协议**（SharePack 即捕获载荷标准）
- **导入管线**：import_commands 五格式栈（markdown_dir/enex/opml/plan）+ write_path 全纪律——扩展落点=新 cmd 或复用 import_share
- **缺口**：浏览器→desktop 的本地通道（扩展无法直调 tauri invoke）

## 建议方案对比（待批复裁定）

| 方案 | 形态 | 代价 | 风险 |
|---|---|---|---|
| A. localhost bridge | desktop 起 127.0.0.1 HTTP 端口（tauri axum/轻量 http）+扩展 fetch POST SharePack | desktop 侧 ~80 行+扩展 MV3 ~100 行 | 端口占用/CORS/本地 token 鉴权（防任意网页投递——**必须 token**：desktop 生成 token 用户复制进扩展配置一次） |
| B. Native Messaging | 扩展↔native host 二进制↔KV 落盘→desktop 轮询 | host 二进制独立分发+注册表/manifest 安装链 | 安装复杂（用户门槛高——MV3 host 注册） |
| C. 扩展导出文件→用户手动 import | 零新通道 | 体验差（非"捕获"） | 低 |

**建议 A**（体验+工程量平衡；token 鉴权=安全线——本地端口只收 SharePack 建笔记，不提供读取面=零信息泄露面）。

## 风险与边界（预写死）

1. **领地**：desktop 侧 HTTP 端口（Bravo 现役 desktop src-tauri——新 capture_server.rs）；扩展仓=独立目录 `apps/browser-extension/`（MV3 manifest+background service worker+content script 最小集）
2. **安全线**：token 鉴权（desktop 端口首启生成+UI 展示复制）；**只写不读**（POST /capture→建笔记——无 GET 数据面）；origin 无限制（token 是唯一门槛——诚实注记）
3. **验收**：desktop 五件+CI；行为级=curl POST 模拟扩展投递→笔记落库（e2e 单测或手测注记）；扩展侧 load-unpacked 手测截图注记

## 预估

M（desktop ~100 行+扩展 ~150 行——超 S 上限，建议独立派单日）
