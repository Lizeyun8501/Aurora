//! DK-09 导入 command 层（Bravo request `bravo-request-import-desktop-wiring`
//! + `bravo-request-migration-wizard-ui` 裁决落地 · 2026-09-21）。
//!
//! # 设计
//! - ctx 与笔记 command 同源装配（APP_STATE/VAULT_STATE/ATTACH_STATE），
//!   附件经 `attach_to_note` 唯一入口（seal 密封 / 内容寻址去重）。
//! - 预扫 `plan_import` 返回 `ImportPlan`（serde 直通，前端向导预览）。
//! - 三个导入 command 支持可选 `manifest_dir`（防重）与 `only`（选择性）。
//! - progress 桥接（2026-09-25）：可选 `on_progress: Channel<ProgressEvent>`，
//!   内核 mpsc sender → Tauri IPC Channel 转发（import 结束后 flush）。

use aurora_core::write_path::WriteContext;
use aurora_import::{
    import_enex, import_markdown_dir, import_opml_file, plan_enex_file, plan_markdown_dir,
    plan_opml_file, EnexImportOptions, ImportOptions, OpmlImportOptions, ProgressEvent,
};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::ipc::Channel;

use crate::{APP_STATE, ATTACH_STATE, VAULT_STATE};

fn get_core() -> Result<Arc<aurora_core::app_core::AppCore>, String> {
    APP_STATE
        .lock()
        .expect("APP_STATE mutex poisoned")
        .clone()
        .ok_or_else(|| "应用尚未初始化".into())
}

/// 导入用 WriteContext（与笔记 command 同源；seal 对 = vault DEK at-rest，
/// 形态与 `isomorphic_write_path::ctx_for` desktop 分支一致）。
/// DK-09 渲染接线起提升为 `pub(crate)`：附件读取 command 同源复用。
pub(crate) fn import_ctx() -> Result<WriteContext, String> {
    let core = get_core()?;
    let vault_opt = VAULT_STATE
        .lock()
        .expect("VAULT_STATE mutex poisoned")
        .clone();
    let seal = vault_opt.map(|vault| {
        let crypto = core.crypto.clone();
        let crypto2 = crypto.clone();
        let vault2 = vault.clone();
        aurora_core::write_path::SealPair {
            seal: Box::new(move |b: &[u8]| {
                vault
                    .encrypt(crypto.as_ref(), b)
                    .map_err(|e| aurora_core::Error::Internal(e.to_string()))
            }),
            unseal: Box::new(move |b: &[u8]| {
                vault2
                    .decrypt(crypto2.as_ref(), b)
                    .map_err(|e| aurora_core::Error::Internal(e.to_string()))
            }),
        }
    });
    Ok(WriteContext {
        core: core.clone(),
        blocks: None, // 导入不派生块（与导入器测试语义一致；正文块由打开笔记时重建）
        seal,
        content_cipher: None, // 导入为明文写入（ENC_NONE），加密级别由用户后续设置
        attachments: ATTACH_STATE
            .lock()
            .expect("ATTACH_STATE mutex poisoned")
            .clone(),
    })
}

/// 预扫（只读）：`kind` ∈ "markdown" | "enex" | "opml"。
#[tauri::command]
pub async fn cmd_plan_import(
    source: String,
    kind: String,
) -> Result<aurora_import::ImportPlan, String> {
    let path = PathBuf::from(&source);
    let plan = match kind.as_str() {
        "markdown" => {
            let root = path.clone();
            tokio::task::spawn_blocking(move || plan_markdown_dir(&root, &ImportOptions::default()))
                .await
                .map_err(|e| e.to_string())?
        }
        "enex" => tokio::task::spawn_blocking(move || plan_enex_file(&path))
            .await
            .map_err(|e| e.to_string())?,
        "opml" => tokio::task::spawn_blocking(move || plan_opml_file(&path))
            .await
            .map_err(|e| e.to_string())?,
        other => return Err(format!("未知导入类型: {other}")),
    };
    plan.map_err(|e| format!("预扫失败 {}: {}", e.path.display(), e.reason))
}

fn manifest(dir: Option<String>) -> Option<PathBuf> {
    dir.map(PathBuf::from)
}

/// progress 桥接：内核 `options.progress`（mpsc UnboundedSender）→ Tauri IPC
/// `Channel<ProgressEvent>`。转发任务在 sender 全部 drop（import 结束）后收尾；
/// 调用方须在 import 返回后 `JoinHandle::await`，保证最后一批事件 flush。
/// 注：Channel 不可包 Option（Option 的 CommandArg blanket 要求 Deserialize），
/// 故 command 参数非 Optional，前端在 tauri 环境下必传。
fn progress_bridge(
    chan: Channel<ProgressEvent>,
) -> (
    tokio::sync::mpsc::UnboundedSender<ProgressEvent>,
    tokio::task::JoinHandle<()>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<ProgressEvent>();
    let forward = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            // Channel::send 失败（前端已关闭）不中断导入——进度是尽力通知。
            let _ = chan.send(ev);
        }
    });
    (tx, forward)
}

/// Markdown 目录导入（可选防重 / 选择性 / 进度）。
#[tauri::command]
pub async fn cmd_import_markdown_dir(
    dir: String,
    manifest_dir: Option<String>,
    only: Option<Vec<String>>,
    on_progress: Channel<ProgressEvent>,
) -> Result<aurora_import::ImportReport, String> {
    let ctx = import_ctx()?;
    let (progress, forward) = progress_bridge(on_progress);
    let options = ImportOptions {
        manifest_dir: manifest(manifest_dir),
        only: only.unwrap_or_default(),
        progress: Some(progress),
        ..Default::default()
    };
    let report = import_markdown_dir(&ctx, &PathBuf::from(&dir), &options)
        .await
        .map_err(|e| format!("导入失败 {}: {}", e.path.display(), e.reason));
    let _ = forward.await;
    report
}

/// ENEX 导入（`attachments_dir` = 资源 sidecar 目录；None → 占位链接）。
#[tauri::command]
pub async fn cmd_import_enex(
    file: String,
    manifest_dir: Option<String>,
    only: Option<Vec<String>>,
    attachments_dir: Option<String>,
    on_progress: Channel<ProgressEvent>,
) -> Result<aurora_import::ImportReport, String> {
    let ctx = import_ctx()?;
    let (progress, forward) = progress_bridge(on_progress);
    let options = EnexImportOptions {
        manifest_dir: manifest(manifest_dir),
        only: only.unwrap_or_default(),
        attachments_dir: attachments_dir.map(PathBuf::from),
        progress: Some(progress),
        ..Default::default()
    };
    let report = import_enex(&ctx, &PathBuf::from(&file), &options)
        .await
        .map_err(|e| format!("导入失败 {}: {}", e.path.display(), e.reason));
    let _ = forward.await;
    report
}

/// OPML 导入。
#[tauri::command]
pub async fn cmd_import_opml(
    file: String,
    manifest_dir: Option<String>,
    only: Option<Vec<String>>,
    on_progress: Channel<ProgressEvent>,
) -> Result<aurora_import::ImportReport, String> {
    let ctx = import_ctx()?;
    let (progress, forward) = progress_bridge(on_progress);
    let options = OpmlImportOptions {
        manifest_dir: manifest(manifest_dir),
        only: only.unwrap_or_default(),
        progress: Some(progress),
        ..Default::default()
    };
    let report = import_opml_file(&ctx, &PathBuf::from(&file), &options)
        .await
        .map_err(|e| format!("导入失败 {}: {}", e.path.display(), e.reason));
    let _ = forward.await;
    report
}
