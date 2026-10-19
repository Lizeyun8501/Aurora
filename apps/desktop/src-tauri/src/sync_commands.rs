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

/// drain 前置守卫（T4 抽取：drain/retry 共用）——门禁（DK-51 案二）+ P2P
/// transport + 配对地址三连检查。
///
/// # Errors
/// 门禁推迟 / P2P 未启用 / 对端地址未配置。
async fn drain_guard(
    booted: &BootedApp,
) -> Result<
    (
        std::sync::Arc<aurora_sync::iroh_transport::IrohTransport>,
        aurora_sync::iroh_transport::PublicEndpointAddr,
    ),
    String,
> {
    // DK-51 案二：drain 过门——网络策略拒绝不进入补发（Defer 不是错误，
    // 不入重试/熔断统计；错误信息携带 decision+reason，前端直接展示）。
    let decision = booted.sync_gate.evaluate();
    if !decision.is_allowed() {
        return Err(format!(
            "同步被门禁推迟（{decision:?}）：{}——检查网络或「仅 Wi-Fi」设置后重试",
            decision.reason()
        ));
    }
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
    Ok((transport, addr))
}

/// 单 doc 解析（T4 抽取：快照+updatelog 重放——40a 打开链语义，公开面拼装，
/// write_path 禁触）。解析失败返回 None（调用方决定回队/报错语义）。
async fn resolve_doc(booted: &BootedApp, doc_id: &str) -> Option<std::sync::Arc<loro::LoroDoc>> {
    let kv = booted.core.kv_store.clone();
    // DK-47：快照读走惰性迁移 helper（legacy 单用户 key 命中搬迁）——
    // user 段取 AppCore 上下文（不立账号，缺省 default）
    let snap = aurora_core::app_core::get_notesnap_with_migration(
        kv.as_ref(),
        &booted.core.user_id,
        doc_id,
    )
    .await
    .ok()
    .flatten()
    .unwrap_or_default();
    let inner = if snap.is_empty() {
        loro::LoroDoc::new()
    } else {
        aurora_core::l1_infrastructure::note_doc::NoteDoc::from_snapshot(&snap)
            .ok()?
            .inner()
            .clone()
    };
    let updates = aurora_core::write_path::read_update_log(kv.as_ref(), doc_id)
        .await
        .ok()?;
    for upd in updates {
        if inner.import(&upd).is_err() {
            return None;
        }
    }
    Some(std::sync::Arc::new(inner))
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
    let (transport, addr) = drain_guard(&booted).await?;

    // doc 解析：drain 前先在 async 上下文把队列涉及的 doc 全部预解析
    // （resolver 闭包只查内存 map——tauri command 要求 Send+Sync 捕获）。
    use std::collections::HashMap as StdHashMap;
    use std::sync::Arc as StdArc;
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
        if let Some(doc) = resolve_doc(&booted, doc_id).await {
            pre.insert(doc_id.clone(), doc);
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

// ===========================================================================
// DK-T4: 单条队列 drain/重试（2026-10-19 台账 T4 消化——验收回执建议序首位）
//
// 场景：队列堆积时选择性补发某 doc（重要笔记优先/反复失败单独试）——
// peek+ack 语义（失败路径队列原样，零数据丢失面）；清单走既有 cmd_queue_list
// （palette 列表选行）。
// ===========================================================================

/// 单条重试：指定 doc 选择性补发（peek+ack——失败路径队列原样零丢失）。
///
/// 与全量 drain 的差异：只动目标 doc；成功即 ack 该 doc 全部积压项；
/// 失败/解析未命中/无积压项均显式报错，队列无副作用。
///
/// # Errors
/// 应用未初始化 / 门禁推迟 / P2P 未启用 / 对端未配置 / 无该 doc 积压项 /
/// doc 解析失败（快照+updatelog 重放）/ 传输失败。
#[tauri::command]
pub async fn cmd_queue_retry(doc_id: String) -> Result<serde_json::Value, String> {
    let booted = get_booted()?;
    let (transport, addr) = drain_guard(&booted).await?;
    // 预检：无积压项先报（避免无谓 doc 解析）
    if aurora_sync::drain::select_queue_items(&booted.offline_queue, &doc_id).is_empty() {
        return Err(format!("队列中无 doc_id={doc_id} 的待补发项"));
    }
    let doc = resolve_doc(&booted, &doc_id)
        .await
        .ok_or_else(|| format!("doc={doc_id} 解析失败（快照+updatelog 重放）"))?;
    let report = aurora_sync::drain::drain_one_doc_sync(
        &booted.offline_queue,
        &transport,
        &addr,
        &doc_id,
        doc,
    )
    .await?;
    Ok(serde_json::json!({
        "synced_docs": report.synced_docs,
        "acked_items": report.acked_items,
        "doc_id": doc_id,
    }))
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

// ===========================================================================
// DK-44：P2P 设备配对（手动配对码模型——DK-40b 带外交换语义的 UI 接线）。
//
// 流程：双方各自「启用 P2P」（bind+导出本机地址 base64=配对码）→ 带外互发
// （剪贴板/IM）→ 粘贴导入对端码 → KV 写 `sync.peer.addr` → drain/直传/
// 收件循环立即可用（收件循环经 setup_share_accept 现场重接线，无需重启）。
// ===========================================================================

const PEER_ID_KV_KEY: &str = "settings:peer_id";
const PEER_ADDR_KV_KEY: &str = "sync.peer.addr";
/// 导出配对码时等待 home relay 连接的封顶（超时不阻塞配对——
/// 返回 addr 附 relay_ready:false 诚实提示，局域网直连面仍可用）。
const PAIR_RELAY_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// 配对状态 DTO（PairDialog 数据源）。
#[derive(Debug, Serialize)]
pub struct PairStatusDto {
    /// 本机 P2P transport 是否已启用（bind 完成）。
    pub p2p_enabled: bool,
    /// 本机 peer 标识（未生成时为空串——enable 时落 KV）。
    pub peer_id: String,
    /// 是否已导入对端地址（sync.peer.addr 存在）。
    pub paired: bool,
    /// 本机配对码（addr JSON 的 base64——未启用为空）。
    pub local_code: String,
    /// home relay 是否已连接（导出的码含 relay 可达面）。
    pub relay_ready: bool,
}

fn b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// 查询配对状态（只读——PairDialog 打开时刷新）。
///
/// # Errors
/// 应用未初始化。
#[tauri::command]
pub async fn cmd_pair_status() -> Result<PairStatusDto, String> {
    let booted = get_booted()?;
    let transport = booted.p2p();
    let peer_id = booted
        .core
        .kv_store
        .get(PEER_ID_KV_KEY)
        .await
        .map_err(|e| e.to_string())?
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .unwrap_or_default();
    let paired = booted
        .core
        .kv_store
        .get(PEER_ADDR_KV_KEY)
        .await
        .map_err(|e| e.to_string())?
        .is_some();
    let mut relay_ready = false;
    let local_code = match &transport {
        Some(t) => {
            if t.wait_home_relay_url(PAIR_RELAY_WAIT).await.is_ok() {
                relay_ready = true;
            }
            b64(t.export_addr_json()?.as_bytes())
        }
        None => String::new(),
    };
    Ok(PairStatusDto {
        p2p_enabled: transport.is_some(),
        peer_id,
        paired,
        local_code,
        relay_ready,
    })
}

/// 启用本机 P2P 传输（幂等）——bind+peer_id 落 KV+**收件循环现场重接线**
/// （setup_share_accept 重新调用即生效，v1 语义无需重启）+导出本机配对码。
///
/// # Errors
/// peer_id 生成/KV 写失败 / iroh bind 失败 / addr 导出失败。
#[tauri::command]
pub async fn cmd_pair_enable() -> Result<PairStatusDto, String> {
    let booted = get_booted()?;
    // peer_id：KV 缺省生成（随机 UUID——PeerId 语义即本机标识，非密码学身份）
    let peer_id = match booted
        .core
        .kv_store
        .get(PEER_ID_KV_KEY)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(v) => String::from_utf8_lossy(&v).into_owned(),
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            booted
                .core
                .kv_store
                .set(PEER_ID_KV_KEY, id.as_bytes())
                .await
                .map_err(|e| format!("peer_id 写入失败: {e}"))?;
            id
        }
    };
    booted
        .enable_p2p(&peer_id)
        .await
        .map_err(|e| format!("P2P 启用失败: {e}"))?;
    // DK-2.8v2 语义兑现：transport 就绪后重新接线收件循环（现场生效）
    let booted2 = get_booted()?;
    crate::share_commands::setup_share_accept(booted2);
    let booted = get_booted()?;
    let transport = booted
        .p2p()
        .ok_or_else(|| "P2P 启用后句柄仍缺失（异常态）".to_string())?;
    let relay_ready = transport.wait_home_relay_url(PAIR_RELAY_WAIT).await.is_ok();
    let local_code = b64(transport.export_addr_json()?.as_bytes());
    Ok(PairStatusDto {
        p2p_enabled: true,
        peer_id,
        paired: booted
            .core
            .kv_store
            .get(PEER_ADDR_KV_KEY)
            .await
            .map_err(|e| e.to_string())?
            .is_some(),
        local_code,
        relay_ready,
    })
}

/// 导入对端配对码（base64(addr JSON)）——KV 写 `sync.peer.addr`，
/// drain/直传发送立即可用。幂等（重复导入覆盖）。
///
/// # Errors
/// base64 解码 / addr JSON 解析失败（码损坏或非同版本导出面）。
#[tauri::command]
pub async fn cmd_pair_import(code: String) -> Result<(), String> {
    let booted = get_booted()?;
    let raw = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(code.trim())
            .map_err(|e| format!("配对码 base64 解码失败: {e}"))?
    };
    let json = String::from_utf8(raw).map_err(|_| "配对码不是合法 UTF-8".to_string())?;
    // 解析校验先行（坏码不落 KV）
    let _addr: aurora_sync::iroh_transport::PublicEndpointAddr =
        aurora_sync::iroh_transport::IrohTransport::import_addr_json(&json)?;
    booted
        .core
        .kv_store
        .set(PEER_ADDR_KV_KEY, json.as_bytes())
        .await
        .map_err(|e| format!("对端地址写入失败: {e}"))
}

/// 解除配对（删 sync.peer.addr——transport 保持启用；对端可重新导入）。
///
/// # Errors
/// 应用未初始化 / KV 删除失败。
#[tauri::command]
pub async fn cmd_pair_unpair() -> Result<(), String> {
    let booted = get_booted()?;
    booted
        .core
        .kv_store
        .delete(PEER_ADDR_KV_KEY)
        .await
        .map_err(|e| format!("解除配对失败: {e}"))
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
