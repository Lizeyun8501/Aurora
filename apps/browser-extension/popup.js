// DK-2.9v2-b: Aurora Web Clipper popup——协议对拍 aurora-import/capture_bridge（c087ec7 冻结面）。
// 载荷 {title,url,content} camelCase；POST /capture + Bearer token；401/400/413/404 显式提示。
// v1 边界：正文=选区优先/innerText 全文兜底（readability 解析 v2 单列）；端口单值（顺延以桌面显示为准）。

/** 载荷构造（纯函数——node scripts/dk29b-smoke.mjs 对拍）。 */
function buildCapturePayload(page, overrides) {
  const o = overrides || {};
  return {
    title: o.title !== undefined && o.title !== '' ? o.title : (page.title || ''),
    url: page.url || '',
    content: o.content !== undefined && o.content !== '' ? o.content : (page.content || ''),
  };
}

/** 页面抓取（executeScript 注入——选区优先，全文兜底）。 */
async function grabActivePage() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  const [res] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func: () => ({
      title: document.title || '',
      url: location.href || '',
      content: (window.getSelection() && String(window.getSelection()).trim()) ||
        (document.body ? document.body.innerText : ''),
    }),
  });
  return res.result;
}

function setStatus(text, cls) {
  const el = document.getElementById('status');
  el.textContent = text;
  el.className = cls || '';
}

async function capture() {
  const token = document.getElementById('token').value.trim();
  const port = Number(document.getElementById('port').value) || 17633;
  if (!token) {
    setStatus('先填 token（桌面 palette「Web Clipper 配置」查看）', 'err');
    return;
  }
  await chrome.storage.local.set({ token, port });

  let page;
  try {
    page = await grabActivePage();
  } catch (e) {
    setStatus('页面抓取失败（chrome:// 等受保护页不可捕获）', 'err');
    return;
  }
  const payload = buildCapturePayload(page, {
    title: document.getElementById('title').value,
    content: document.getElementById('content').value,
  });
  if (!payload.title && !payload.content) {
    setStatus('页面内容为空，无可捕获', 'err');
    return;
  }

  setStatus('发送中…', '');
  try {
    const resp = await fetch(`http://127.0.0.1:${port}/capture`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify(payload),
    });
    if (resp.status === 201) {
      setStatus('✓ 已捕获——Aurora 收件笔记已建', 'ok');
    } else if (resp.status === 401) {
      setStatus('token 不对——到桌面 palette 重新复制', 'err');
    } else if (resp.status === 413) {
      setStatus('内容超限（>1MB）——减少选区后重试', 'err');
    } else if (resp.status === 400) {
      setStatus('载荷被拒——内容含非法编码？', 'err');
    } else {
      setStatus(`桥返回 ${resp.status}`, 'err');
    }
  } catch (e) {
    setStatus('连不上桥——确认 Aurora 桌面已启动（端口 ' + port + '）', 'err');
  }
}

async function init() {
  const { token, port } = await chrome.storage.local.get(['token', 'port']);
  if (token) document.getElementById('token').value = token;
  if (port) document.getElementById('port').value = port;
  let page;
  try {
    page = await grabActivePage();
    document.getElementById('title').value = page.title || '';
    document.getElementById('content').value = page.content || '';
  } catch (e) {
    setStatus('此页不可捕获（受保护页）', 'err');
  }
  document.getElementById('send').addEventListener('click', capture);
}

if (typeof chrome !== 'undefined' && chrome.tabs) {
  document.addEventListener('DOMContentLoaded', init);
}

// node 烟测导出（浏览器环境无 module——防御式挂载）
if (typeof module !== 'undefined' && module.exports) {
  module.exports = { buildCapturePayload };
}
