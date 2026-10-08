//! DK-08 §7.3 同步门 command 层（wifi_only 设置端到端 · Alpha 装配切片
//! 2026-09-24）。
//!
//! # 设计
//! - 读写经 `BootedApp` 同源语义：KV 权威（`settings:sync.wifi_only`，
//!   崩溃安全）+ `SyncGate::set_wifi_only` 运行时即时生效；
//! - BOOTED 句柄复用 APP_STATE——gate 与 core 同源装配，读 KV 用 core
//!   的 kv_store（BOOTED 生命周期与 APP_STATE 一致）。

use aurora_bootstrap::BootedApp;
use std::sync::{Arc, Mutex};

/// bootstrap 结果的进程级缓存（wifi_only 读写需 BootedApp 方法；
/// setup 阶段与 APP_STATE 同步注入）。
pub(crate) static BOOTED_STATE: Mutex<Option<Arc<BootedApp>>> = Mutex::new(None);

pub(crate) fn get_booted() -> Result<Arc<BootedApp>, String> {
    BOOTED_STATE
        .lock()
        .expect("BOOTED_STATE mutex poisoned")
        .clone()
        .ok_or_else(|| "应用尚未初始化".into())
}

/// 查询「仅 Wi-Fi 同步」开关（KV 权威值 + 门内当前值一致性由装配保证）。
///
/// # Errors
/// 应用未初始化 / KV 读取失败（透传）。
#[tauri::command]
pub async fn cmd_get_wifi_only() -> Result<bool, String> {
    let booted = get_booted()?;
    booted.wifi_only().map_err(|e| e.to_string())
}

/// 设置「仅 Wi-Fi 同步」开关（KV 持久化 + 门运行时切换，即时生效）。
///
/// # Errors
/// 应用未初始化 / KV 写入失败（透传）。
#[tauri::command]
pub async fn cmd_set_wifi_only(on: bool) -> Result<(), String> {
    let booted = get_booted()?;
    booted.set_wifi_only(on).map_err(|e| e.to_string())
}

// ===========================================================================
// DK-44: 桌面同步体验消费端 —— 同步状态 / 离线队列 / 冲突处理 三块 command
// 引擎数据面只读消费（aurora-sync 既有 API；drain 走 DK-41 桥接后的真实补发）。
// ===========================================================================

use aurora_sync::conflict::ConflictResolution;
use serde::Serialize;

/// 同步状态聚合视图（前端壳层指示器数据源）。
#[derive(Debug, Serialize)]
pub struct SyncStatusDto {
    /// 门控判定：Allow / DeferUntilUnmetered / DeferUntilOnline
    pub gate: String,
    /// 仅 Wi-Fi 开关当前值
    pub wifi_only: bool,
    /// 离线队列积压条数
    pub queue_len: usize,
    /// 待处理真冲突副本数
    pub pending_artifacts: usize,
    /// 待人工介入语义冲突数
    pub pending_semantic: usize,
}

/// 查询同步状态聚合（门控判定 + 队列积压 + 冲突待处理）。
///
/// # Errors
/// 应用未初始化。
#[tauri::command]
pub async fn cmd_sync_status() -> Result<SyncStatusDto, String> {
    let booted = get_booted()?;
    let gate = match booted.sync_gate.evaluate() {
        aurora_sync::sync_gate::GateDecision::Allow => "Allow",
        aurora_sync::sync_gate::GateDecision::DeferUntilUnmetered => "DeferUntilUnmetered",
        aurora_sync::sync_gate::GateDecision::DeferUntilOnline => "DeferUntilOnline",
    };
    Ok(SyncStatusDto {
        gate: gate.to_string(),
        wifi_only: booted.sync_gate.wifi_only(),
        queue_len: booted.offline_queue.len(),
        pending_artifacts: booted.conflict_artifacts.pending().len(),
        pending_semantic: booted.conflict_resolver.pending_conflicts().len(),
    })
}

/// 队列条目 DTO（可视化列表行）。
#[derive(Debug, Serialize)]
pub struct QueueItemDto {
    pub id: String,
    pub doc_id: String,
    /// Priority 权重（数值语义见 Priority；前端仅展示排序）
    pub priority: u8,
    pub attempts: u32,
    pub created_at: String,
}

/// 列出离线队列全部积压项（只读快照）。
///
/// # Errors
/// 应用未初始化。
#[tauri::command]
pub async fn cmd_queue_list() -> Result<Vec<QueueItemDto>, String> {
    let booted = get_booted()?;
    Ok(booted
        .offline_queue
        .items()
        .into_iter()
        .map(|i| QueueItemDto {
            id: i.id,
            doc_id: i.doc_id,
            priority: i.priority.weight(),
            attempts: i.attempts,
            created_at: i.created_at.to_rfc3339(),
        })
        .collect())
}

/// drain 结果 DTO。
#[derive(Debug, Serialize)]
pub struct DrainReportDto {
    pub synced_docs: usize,
    pub failed_docs: Vec<String>,
    pub acked_items: usize,
}

/// 手动触发离线队列补发（DK-41 桥接后即真实同步）。
///
/// P2P 未启用（`enable_p2p` 未调用/失败）时返回明确错误——
/// 前端据此提示用户先完成 P2P 配对。
///
/// # Errors
/// 应用未初始化 / P2P 未启用 / 对端地址未配置。
#[tauri::command]
pub async fn cmd_queue_drain() -> Result<DrainReportDto, String> {
    let booted = get_booted()?;
    let Some(transport) = booted.p2p() else {
        return Err("P2P 未启用：请先完成设备配对".into());
    };
    // 对端地址由配对面写入 KV（`sync.peer.addr`）；缺省即无对端可补发。
    let addr_b64 = booted
        .core
        .kv_store
        .get("sync.peer.addr")
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "尚未配置同步对端".to_string())?;
    let addr: aurora_sync::iroh_transport::PublicEndpointAddr =
        serde_json::from_slice(&addr_b64).map_err(|e| format!("对端地址解析失败: {e}"))?;

    // doc 解析（40a 打开链语义，公开面拼装——write_path 禁触）：
    // drain 前先在 async 上下文把队列涉及的 doc 全部预解析（快照+updatelog 重放），
    // resolver 闭包只查内存 map（tauri command 要求 Send+Sync 捕获）。
    use std::collections::HashMap as StdHashMap;
    use std::sync::Arc as StdArc;
    let kv = booted.core.kv_store.clone();
    let doc_ids: Vec<String> = booted
        .offline_queue
        .items()
        .into_iter()
        .map(|i| i.doc_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut pre: StdHashMap<String, StdArc<loro::LoroDoc>> = StdHashMap::new();
    for doc_id in &doc_ids {
        let snap = kv
            .get(&format!("notesnap:{doc_id}"))
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let inner = if snap.is_empty() {
            loro::LoroDoc::new()
        } else {
            match aurora_core::l1_infrastructure::note_doc::NoteDoc::from_snapshot(&snap) {
                Ok(d) => d.inner().clone(),
                Err(_) => continue,
            }
        };
        let updates = match aurora_core::write_path::read_update_log(kv.as_ref(), doc_id).await {
            Ok(u) => u,
            Err(_) => continue,
        };
        let mut ok = true;
        for upd in updates {
            if inner.import(&upd).is_err() {
                ok = false;
                break;
            }
        }
        if ok {
            pre.insert(doc_id.clone(), StdArc::new(inner));
        }
    }
    let resolver =
        move |doc_id: &str| -> Option<StdArc<loro::LoroDoc>> { pre.get(doc_id).cloned() };

    let report = aurora_sync::drain::drain_to_peer_sync(
        &booted.offline_queue,
        &transport,
        &addr,
        &resolver,
        64,
    )
    .await;
    Ok(DrainReportDto {
        synced_docs: report.synced_docs,
        failed_docs: report.failed_docs,
        acked_items: report.acked_items,
    })
}

/// 冲突 DTO（真冲突副本 + 语义冲突统一列表行）。
#[derive(Debug, Serialize)]
pub struct ConflictDto {
    pub kind: String, // "artifact" | "semantic"
    pub id: String,
    pub doc_id: String,
    /// 展示摘要（artifact=远端路径；semantic=字段名）
    pub summary: String,
    pub created_at: String,
}

/// 列出待处理冲突（真冲突副本 + 语义冲突合并视图）。
///
/// # Errors
/// 应用未初始化。
#[tauri::command]
pub async fn cmd_conflict_list() -> Result<Vec<ConflictDto>, String> {
    let booted = get_booted()?;
    let mut out: Vec<ConflictDto> = booted
        .conflict_artifacts
        .pending()
        .into_iter()
        .map(|a| ConflictDto {
            kind: "artifact".into(),
            id: a.artifact_id,
            doc_id: a.doc_id,
            summary: a.remote_path,
            created_at: a.created_at.to_rfc3339(),
        })
        .collect();
    out.extend(
        booted
            .conflict_resolver
            .pending_conflicts()
            .into_iter()
            .map(|c| ConflictDto {
                kind: "semantic".into(),
                id: c.conflict_id.clone(),
                doc_id: c.doc_id.clone(),
                summary: c.field.clone(),
                created_at: c
                    .local_updated_at
                    .map(|t| t.to_rfc3339())
                    .unwrap_or_default(),
            }),
    );
    Ok(out)
}

/// 解决真冲突副本（人工调和完成 → artifact 状态闭环）。
///
/// # Errors
/// 应用未初始化 / artifact 不存在。
#[tauri::command]
pub async fn cmd_conflict_resolve(artifact_id: String) -> Result<(), String> {
    let booted = get_booted()?;
    booted
        .conflict_artifacts
        .mark_resolved(&artifact_id)
        .map_err(|e| e.to_string())
}

/// 解决语义冲突（手动选择策略——既有 resolve 面透传）。
///
/// # Errors
/// 应用未初始化 / 冲突不存在 / 策略非法。
#[tauri::command]
pub async fn cmd_conflict_resolve_semantic(
    conflict_id: String,
    resolution: String,
) -> Result<serde_json::Value, String> {
    let booted = get_booted()?;
    let strategy = match resolution.as_str() {
        "LocalWins" => ConflictResolution::LocalWins,
        "RemoteWins" => ConflictResolution::RemoteWins,
        "LastWriteWins" => ConflictResolution::LastWriteWins,
        "Branch" => ConflictResolution::Branch,
        _ => return Err(format!("非法解决策略: {resolution}")),
    };
    booted
        .conflict_resolver
        .resolve(&conflict_id, strategy)
        .map_err(|e| e.to_string())
}
