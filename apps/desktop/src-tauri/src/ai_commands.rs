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
    let bytes = serde_json::to_vec(&proposal).map_err(|e| e.to_string())?;
    core.kv_store
        .put(&key, bytes)
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
        .put(
            &key,
            serde_json::to_vec(&proposal).map_err(|e| e.to_string())?,
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
        .put(
            &key,
            serde_json::to_vec(&proposal).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())
}
