//! DK-10 切片 3：AI 云策略 command 层（desktop UI 开关后端面）。
//!
//! 读写经 `BootedApp` 同源语义：KV `ws-policy:{id}` 权威（崩溃安全，重启
//! 预载恢复）+ `WorkspaceConfigResolver` 内存直写即时生效。workspace_id
//! 缺省 = `AI_DEFAULT_WORKSPACE_ID`（单工作区口径；多工作区立项后由前端
//! 传实际 id，命令签名不变）。

use aurora_ai::policy::WorkspacePolicy;
use aurora_bootstrap::AI_DEFAULT_WORKSPACE_ID;

use crate::sync_commands::get_booted;

fn ws_or_default(workspace_id: Option<String>) -> String {
    workspace_id.unwrap_or_else(|| AI_DEFAULT_WORKSPACE_ID.into())
}

/// 查询工作区 AI 云策略（"allow" | "deny"）。
#[tauri::command]
pub async fn cmd_get_ai_cloud_policy(workspace_id: Option<String>) -> Result<String, String> {
    let ws = ws_or_default(workspace_id);
    let booted = get_booted()?;
    Ok(match booted.ai_cloud_policy(&ws) {
        WorkspacePolicy::AllowCloud => "allow",
        WorkspacePolicy::DenyCloud => "deny",
    }
    .into())
}

/// 设置工作区 AI 云策略（deny = true 禁止云端出网）。
#[tauri::command]
pub async fn cmd_set_ai_cloud_policy(
    workspace_id: Option<String>,
    deny: bool,
) -> Result<(), String> {
    let ws = ws_or_default(workspace_id);
    get_booted()?
        .set_ai_cloud_policy(&ws, deny)
        .map_err(|e| e.to_string())
}

// ============================================================================
// DK-10 两段式提交（aiLiquify → 用户勾选 → aiCommit）——铁律：AI 不得静默写入
// ============================================================================
//
// 裁决：`liq:` 键前缀为 AI 会话域（提案暂存区），liquify 写它不违反铁律；
// 真正的笔记落库只发生在 `cmd_ai_commit_liquify`（**用户 UI 显式触发**），
// 经 write_path 标准写入。aiCommit 不注册进 AI 工具表（装配纪律）。

use aurora_ai::liquify::{ai_commit, parse_proposal, LiquifyProposal, ProposalStatus};

/// aiLiquify：解析 AI 提案 JSON → Draft 提案（只写 AI 会话域 `liq:` 键）。
#[tauri::command]
pub async fn cmd_ai_liquify_proposal(source: String, raw_json: String) -> Result<String, String> {
    let core = crate::get_core()?;
    let proposal = parse_proposal(&source, &raw_json).map_err(|e| e.to_string())?;
    let key = format!("liq:proposal:{}", proposal.id);
    let bytes = &serde_json::to_vec(&proposal).map_err(|e| e.to_string())?;
    core.kv_store
        .set(&key, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::to_string(&proposal).map_err(|e| e.to_string())
}

/// 列出全部提案（回收站式按前缀扫描，AI 会话域）。
#[tauri::command]
pub async fn cmd_ai_list_liquify_proposals() -> Result<Vec<String>, String> {
    let core = crate::get_core()?;
    let rows = core
        .kv_store
        .scan_prefix("liq:proposal:")
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|(_, bytes)| serde_json::from_slice::<LiquifyProposal>(&bytes).ok())
        .map(|p| serde_json::to_string(&p).unwrap_or_default())
        .collect())
}

/// aiCommit：**用户勾选后**才落库——逐 op 走 write_path，部分失败诚实报告。
#[tauri::command]
pub async fn cmd_ai_commit_liquify(
    proposal_id: String,
    selected: Vec<usize>,
) -> Result<String, String> {
    let core = crate::get_core()?;
    let key = format!("liq:proposal:{proposal_id}");
    let bytes = core
        .kv_store
        .get(&key)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("proposal not found: {proposal_id}"))?;
    let mut proposal: LiquifyProposal =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;

    // owned ctx（同 cmd_delete_note 模式：'static boxed 闭包防 async 自借用）
    let vault = crate::get_vault()?;
    let blocks = crate::blocks_store();
    let crypto = core.crypto.clone();
    let vault_seal = vault.clone();
    let seal = move |b: &[u8]| {
        vault_seal
            .encrypt(crypto.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let crypto2 = core.crypto.clone();
    let vault_unseal = vault.clone();
    let unseal = move |b: &[u8]| {
        vault_unseal
            .decrypt(crypto2.as_ref(), b)
            .map_err(|e| aurora_core::Error::Internal(e.to_string()))
    };
    let ctx = aurora_core::write_path::WriteContext {
        core: core.clone(),
        blocks,
        seal: Some(aurora_core::write_path::SealPair {
            seal: Box::new(seal),
            unseal: Box::new(unseal),
        }),
        content_cipher: None, // TODO(DK-07): 桌面 UI 接 vault cipher
        attachments: None,    // 提案落库不涉附件（首片收敛）
    };

    let committed = ai_commit(&ctx, proposal.clone(), &selected)
        .await
        .map_err(|e| e.to_string())?;
    let _ = &committed.status;
    proposal.results = committed.results.clone();
    proposal.status = committed.status.clone();
    let out = serde_json::to_string(&proposal).map_err(|e| e.to_string())?;
    core.kv_store
        .set(
            &key,
            &serde_json::to_vec(&proposal).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// 丢弃提案（用户拒绝——终态 Rejected；AI 会话域状态更新）。
#[tauri::command]
pub async fn cmd_ai_reject_liquify_proposal(proposal_id: String) -> Result<(), String> {
    let core = crate::get_core()?;
    let key = format!("liq:proposal:{proposal_id}");
    let bytes = core
        .kv_store
        .get(&key)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("proposal not found: {proposal_id}"))?;
    let mut proposal: LiquifyProposal =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    proposal.status = ProposalStatus::Rejected;
    core.kv_store
        .set(
            &key,
            &serde_json::to_vec(&proposal).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())
}

// ============================================================================
// DK-10 切片 8：Agent 会话面板命令面（Kill-Switch 用户可达化）
// ============================================================================
//
// AgentSession 编排层（aurora-ai agent_session）的桌面装配：全局会话表 +
// 全局审计链（哈希链防篡改）。Kill-Switch 从引擎层提升为 UI 按钮——用户
// 可随时强杀会话（立即生效不可撤销，理由落审计链）。

use aurora_ai::agent_session::{AgentSession, AGENT_DEFAULT_DEADLINE_SECS};
use aurora_ai::sandbox::{AuditAction, AuditDecision, AuditEntry, AuditLog};
use std::collections::HashMap;
use std::time::Instant;

static AGENT_AUDIT: std::sync::OnceLock<Arc<AuditLog>> = std::sync::OnceLock::new();
static AGENT_SESSIONS: std::sync::OnceLock<Mutex<HashMap<String, Arc<AgentSession>>>> =
    std::sync::OnceLock::new();

fn agent_audit() -> Arc<AuditLog> {
    AGENT_AUDIT
        .get_or_init(|| Arc::new(AuditLog::new()))
        .clone()
}

fn agent_sessions() -> &'static Mutex<HashMap<String, Arc<AgentSession>>> {
    AGENT_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 创建 Agent 会话（15 分钟默认限时；注册进全局表+落审计）。
#[tauri::command]
pub async fn cmd_agent_session_create() -> Result<serde_json::Value, String> {
    let id = format!("agent-{}", chrono::Utc::now().timestamp_millis());
    let session = Arc::new(
        AgentSession::new(&id, agent_audit()).with_deadline_secs(AGENT_DEFAULT_DEADLINE_SECS),
    );
    agent_sessions()
        .lock()
        .map_err(|e| format!("sessions mutex poisoned: {e}"))?
        .insert(id.clone(), session);
    let _ = agent_audit().entries(); // 触发注册表初始化语义（审计在 AgentSession::new 内已落）
    Ok(serde_json::json!({ "id": id, "deadline_secs": AGENT_DEFAULT_DEADLINE_SECS }))
}

/// Kill-Switch：强杀会话（立即生效不可撤销；理由落审计链）。
#[tauri::command]
pub async fn cmd_agent_session_kill(session_id: String, reason: String) -> Result<(), String> {
    let sessions = agent_sessions()
        .lock()
        .map_err(|e| format!("sessions mutex poisoned: {e}"))?;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    session.kill(&reason);
    Ok(())
}

/// 会话状态（killed/expired/remaining_secs）。
#[tauri::command]
pub async fn cmd_agent_session_status(session_id: String) -> Result<serde_json::Value, String> {
    let sessions = agent_sessions()
        .lock()
        .map_err(|e| format!("sessions mutex poisoned: {e}"))?;
    let session = sessions
        .get(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;
    Ok(serde_json::json!({
        "id": session_id,
        "killed": session.is_killed(),
        "expired": session.is_expired(),
        "remaining_secs": session.remaining_secs(),
    }))
}

/// 最近审计条目（倒序 limit 条）+ 链完整性校验结果。
#[tauri::command]
pub async fn cmd_agent_audit_recent(limit: Option<usize>) -> Result<serde_json::Value, String> {
    let audit = agent_audit();
    let all = audit.entries();
    let n = limit.unwrap_or(50).min(500);
    let recent: Vec<&AuditEntry> = all.iter().rev().take(n).collect();
    let items: Vec<serde_json::Value> = recent
        .iter()
        .map(|e| {
            serde_json::json!({
                "timestamp": e.timestamp.to_rfc3339(),
                "action": e.action.as_str(),
                "decision": e.decision.as_str(),
                "tool_name": e.tool_name,
                "session_id": e.session_id,
                "detail": e.detail,
            })
        })
        .collect();
    Ok(
        serde_json::json!({ "entries": items, "chain_valid": audit.verify_chain(), "total": all.len() }),
    )
}

/// 手写审计条目（Kill-Switch 按钮外的运维动作——如导出审计快照标记）。
#[tauri::command]
pub async fn cmd_agent_audit_note(detail: String) -> Result<(), String> {
    let e = AuditEntry::new(AuditAction::Check, AuditDecision::Allow, "operator", detail);
    agent_audit().record(e);
    Ok(())
}
