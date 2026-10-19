//! DK-2.9v2-a: Web Clipper localhost 桥 desktop 壳（批复方案 A 2026-10-18）。
//!
//! 协议与安全面（token 鉴权/只写不读/CORS 预检）全在
//! `aurora_import::capture_bridge`（可链接 crate——行为级测试同仓）；本壳只做
//! 生命周期（start/stop/status）+ token 持久化 + 收包建笔记（import_share_code
//! 同链——来源 url 以引用行前缀入 content，schema 零变更）。
//!
//! 诚实边界：
//! - 桥 setup 自动启动（只写面无副作用——token 未配置则生成并持久化）；
//! - UI 面延后（token 复制入口留给扩展卡 2.9v2-b 一起落——status cmd 已可查）；
//! - 仅 localhost bind，外部网络不可达。

use crate::{share_commands, sync_commands::get_booted};
use aurora_import::capture_bridge::{self, CaptureRequest};
use std::sync::Mutex;

/// 桥端口基址（占用顺延试 8 个——多实例开发友好）。
const BRIDGE_PORT_BASE: u16 = 17633;
/// 端口尝试数上限。
const BRIDGE_PORT_TRIES: u16 = 8;
/// token KV 键（首次生成持久化——扩展配置一次后长期复用）。
pub(crate) const BRIDGE_TOKEN_KEY: &str = "capture.bridge.token";

/// 运行中桥句柄（shutdown 信号+实际端口）。
struct BridgeHandle {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    port: u16,
}

static BRIDGE_STATE: Mutex<Option<BridgeHandle>> = Mutex::new(None);

/// 生成新 token（uuid v4 双拼熵 → sha256 hex 定长 64）。
fn generate_token() -> String {
    let entropy = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    capture_bridge::format_token(entropy.as_bytes())
}

/// 读 token（无则生成并持久化——KV 权威）。
async fn ensure_token() -> Result<String, String> {
    let booted = get_booted()?;
    if let Some(existing) = booted
        .core
        .kv_store
        .get(BRIDGE_TOKEN_KEY)
        .await
        .map_err(|e| e.to_string())?
    {
        return Ok(String::from_utf8_lossy(&existing).to_string());
    }
    let token = generate_token();
    booted
        .core
        .kv_store
        .set(BRIDGE_TOKEN_KEY, token.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    Ok(token)
}

/// 捕获落库：SharePack 同链（来源 url 引用行前缀——schema 零变更诚实简化）。
async fn ingest_capture(req: CaptureRequest) {
    let content = if req.url.is_empty() {
        req.content
    } else {
        format!("> 来源: {}\n\n{}", req.url, req.content)
    };
    let pack_json = serde_json::json!({
        "title": if req.title.is_empty() { "未命名捕获".to_string() } else { req.title },
        "content": content,
        "createdAt": chrono::Utc::now().to_rfc3339(),
    });
    let pack_b64 = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(pack_json.to_string())
    };
    match share_commands::import_share_code(&pack_b64).await {
        Ok(_) => tracing::info!("capture ingested: url={}", req.url),
        Err(e) => tracing::error!("capture ingest failed: {e}"),
    }
}

/// 启桥（幂等——已运行则返回现端口）。
///
/// # Errors
/// 应用未初始化 / token 持久化失败 / 全部候选端口被占。
#[tauri::command]
pub async fn cmd_capture_bridge_start() -> Result<serde_json::Value, String> {
    // 幂等：已运行直接报端口
    if let Some(h) = BRIDGE_STATE.lock().expect("BRIDGE_STATE poisoned").as_ref() {
        return Ok(serde_json::json!({ "running": true, "port": h.port, "already": true }));
    }
    let token = ensure_token().await?;
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
    // 端口占用顺延：bind 成功的最小候选生效
    let mut bound = None;
    let mut last_err = String::from("no port tried");
    for i in 0..BRIDGE_PORT_TRIES {
        match tokio::net::TcpListener::bind(("127.0.0.1", BRIDGE_PORT_BASE + i)).await {
            Ok(l) => {
                bound = Some(l);
                break;
            }
            Err(e) => last_err = format!("{e}"),
        }
    }
    let listener = bound.ok_or_else(|| format!("capture bridge 全部候选端口被占: {last_err}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<CaptureRequest>();
    // 收包建笔记任务（串行——捕获低频）
    tauri::async_runtime::spawn(async move {
        while let Some(req) = rx.recv().await {
            ingest_capture(req).await;
        }
    });
    // 桥主循环（shutdown 信号触发退出）
    tauri::async_runtime::spawn(async move {
        let bridge = aurora_import::capture_bridge::run_bridge(listener, token, tx);
        tokio::select! {
            _ = bridge => {},
            _ = shutdown_rx.changed() => {},
        }
    });
    *BRIDGE_STATE.lock().expect("BRIDGE_STATE poisoned") = Some(BridgeHandle { shutdown_tx, port });
    tracing::info!(port, "capture bridge started");
    Ok(serde_json::json!({ "running": true, "port": port, "already": false }))
}

/// 停桥（幂等——未运行返回 running:false）。
#[tauri::command]
pub fn cmd_capture_bridge_stop() -> Result<serde_json::Value, String> {
    if let Some(h) = BRIDGE_STATE.lock().expect("BRIDGE_STATE poisoned").take() {
        let _ = h.shutdown_tx.send(true);
        tracing::info!(port = h.port, "capture bridge stopped");
    }
    Ok(serde_json::json!({ "running": false }))
}

/// 查桥状态（running/port/token——扩展配置面数据源）。
///
/// # Errors
/// 应用未初始化 / token 读失败。
#[tauri::command]
pub async fn cmd_capture_bridge_status() -> Result<serde_json::Value, String> {
    let token = ensure_token().await?;
    let running = BRIDGE_STATE
        .lock()
        .expect("BRIDGE_STATE poisoned")
        .as_ref()
        .map(|h| serde_json::json!({ "running": true, "port": h.port }));
    Ok(match running {
        Some(v) => serde_json::json!({
            "token": token,
            "running": v.get("running").cloned().unwrap_or(serde_json::json!(false)),
            "port": v.get("port").cloned().unwrap_or(serde_json::Value::Null),
        }),
        None => serde_json::json!({ "token": token, "running": false }),
    })
}

/// setup 挂钩：app 启动即启桥（只写面无副作用；失败不阻断启动——log 降级）。
pub(crate) fn setup_capture_bridge() {
    tauri::async_runtime::spawn(async {
        match cmd_capture_bridge_start().await {
            Ok(v) => tracing::info!("capture bridge auto-start: {v}"),
            Err(e) => tracing::warn!("capture bridge auto-start deferred: {e}"),
        }
    });
}

#[cfg(test)]
mod capture_server_tests {
    use super::*;

    #[test]
    fn dk29a_token_gen_shape() {
        let t = generate_token();
        assert_eq!(t.len(), 64);
        assert_ne!(generate_token(), t, "两次生成须不同（uuid 熵）");
    }
}
