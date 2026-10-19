# bravo-note：2.9v2-b 浏览器扩展预研——a 桥已交付（c087ec7），协议面冻结

> Bravo ｜ 2026-10-19 ｜ 对象: alpha-approval-2.8v2-2.9v2.md ②b 卡
> **状态：预研落盘——a CI 绿+回执后即可开工（协议面已从 a 实际形态抄死）**

## a 实际协议面（c087ec7 冻结——b 按此写死，零再确认）

| 项 | 值 |
|---|---|
| 端点 | `POST http://127.0.0.1:{port}/capture`（基座 17633 顺延 8 候选） |
| 鉴权 | `Authorization: Bearer {token}`（token=64 hex——desktop `cmd_capture_bridge_status` 取） |
| CORS | 预检 OPTIONS 204 + `Access-Control-Allow-Origin: *`（a 面已回——扩展 fetch 无阻） |
| 载荷 | `{"title": str, "url": str, "content": str}`（camelCase——**url 必填可为空串**） |
| 响应 | 201 `{"ok":true}` / 401 token 错 / 400 坏 JSON / 413 >1MB / 404 零 GET |
| ingest | a 壳自动建笔记（来源 url 引用行前缀——SharePack 同链） |

## b 实施预案（MV3，S-M）

```
apps/browser-extension/
  manifest.json   # MV3: permissions=[activeTab, scripting, storage]；
                  # host_permissions=["http://127.0.0.1/*"]——**不用 17633 精确口**
                  # （顺延机制下精确口会漏；host 127.0.0.1 全口放行——只写面已兜底）
  background.js   # service worker：icon 点击→executeScript 抓正文→fetch POST
  popup.html/js   # token 配置（chrome.storage.local）+ 状态显示 + 手动发送
  icons/          # 16/48/128
```

- **正文提取 v1**：executeScript 取 `document.title` + `location.href` +
  （selection 优先 `window.getSelection().toString()`，空则 `document.body.innerText`）
  ——readability 解析器 v2 单列
- **token 流**：popup 粘贴（desktop status cmd 查）→ storage.local → 每请求带
- **错误面**：401→提示查 token；ECONNREFUSED→提示启 desktop；201→成功角标

## 边界预写死（防执行期扯皮）

1. b 仅装 a 已交付协议——**不加新服务端面**（发现缺口→回卡另批，不硬改 a 冻结面）
2. 正文提取质量承诺 v1 诚实：innerText 全文兜底——剪藏美化/图片捕获延后 v2
3. 领地：`apps/browser-extension/` 新目录（a 未触碰）；不动 desktop/aurora-import
4. 验收：手动装 Chrome 走通（本机无浏览器自动化）——行为级=协议对拍单测（载荷构造）
