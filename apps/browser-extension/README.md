# Aurora Web Clipper（DK-2.9v2-b，MV3）

捕获网页到 Aurora 笔记——经 desktop localhost 桥（`aurora-import/capture_bridge`，c087ec7 冻结协议面）。

## 安装（Chrome/Edge）
1. `chrome://extensions` → 开发者模式 → 加载已解压的扩展程序 → 选本目录
2. Aurora 桌面启动（桥 setup 自动起，默认端口 17633）
3. 桌面 palette 搜「Web Clipper 配置」复制 token
4. 扩展 popup 粘贴 token → 页面预览 → 「捕获到 Aurora」

## 协议（冻结——变更须 a/b 两卡同步）
`POST http://127.0.0.1:{port}/capture` + `Authorization: Bearer {token}`
载荷 `{"title","url","content"}` camelCase → 201 建 / 401 token 错 / 413 >1MB

## 正文提取（Rv2）
- `vendor/readability.js`（@mozilla/readability 0.6.0，Apache-2.0——LICENSE 同仓）两段 executeScript 注入解析
- 解析失败/空正文自动回退 v1 路径（选区优先→innerText 全文）
- 烟测：`cd apps/desktop && node ../../scripts/rv2-smoke.mjs`（jsdom fixture 对拍 6 例——噪声剔除实证）

## 测试
`node scripts/dk29b-smoke.mjs`（载荷构造对拍 4 例）+ 上行 Rv2 烟测
