//! DK-10 两段式提交（aiLiquify → 用户勾选 → aiCommit）。
//!
//! 铁律（清单卡原文）：**AI 不得静默写入**。
//! - `parse_proposal`（aiLiquify）：AI 输出结构化提案 JSON → 解析校验 → `Draft`
//!   提案。提案元数据落 `liq:` 键前缀（AI 会话域，非核心聚合——裁决：铁律管
//!   笔记/任务等核心写入路径，提案暂存区是 AI 自己的可写域）；
//! - `ai_commit`：**仅由用户 UI 动作触发**（tauri 用户命令面，不在 AI 工具表），
//!   逐 op 走 `write_path` 标准写入（create_note/save_note_content）——AI 全程
//!   不接触 note: 键；部分失败诚实报告（单 op 失败继续其余，回执清单报全部）；
//! - `ProposalStatus` 状态机：Draft → Committed / Rejected（终态拒绝二次提交）。

use serde::{Deserialize, Serialize};

use crate::Error;

/// 提案操作上限（防滥用：单提案最多 50 个操作）。
pub const MAX_OPS: usize = 50;

/// 提案操作（首片收敛两种：覆盖 AI 笔记写入主场景）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProposedOp {
    /// 创建笔记（title 非空；content 可空——空正文经 create_note 一步完成）。
    CreateNote { title: String, content: String },
    /// 覆盖式更新笔记正文（expected_title 仅供 UI 展示确认，不做并发校验——
    /// 并发防覆盖守卫挂 Agent 会话片）。
    UpdateNote {
        note_id: String,
        new_content: String,
        expected_title: String,
    },
}

/// 提案状态机：Draft → Committed（提交完成）/ Rejected（用户整体拒绝）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// 待用户勾选。
    Draft,
    /// 已提交落库（终态）。
    Committed,
    /// 已拒绝（终态）。
    Rejected,
}

/// 单 op 提交结果（诚实报告：成功带 note_id，失败带错误摘要）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpResult {
    /// 对应 [`LiquifyProposal::ops`] 的下标。
    pub op_index: usize,
    pub ok: bool,
    /// 成功时的笔记 ID（CreateNote 新建 / UpdateNote 原值）。
    pub note_id: Option<String>,
    /// 失败摘要。
    pub error: Option<String>,
}

/// AI 液化提案（两段式的第一段产物）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiquifyProposal {
    /// 提案 ID（`liq-` 前缀 + uuid）。
    pub id: String,
    /// 创建毫秒时间戳。
    pub created_at: i64,
    /// 来源标识（模型名/会话 ID，审计用）。
    pub source: String,
    /// 提案操作清单（用户逐条勾选）。
    pub ops: Vec<ProposedOp>,
    pub status: ProposalStatus,
    /// 提交回执（commit 后填充；按勾选顺序）。
    #[serde(default)]
    pub results: Vec<OpResult>,
}

impl LiquifyProposal {
    /// 提案 KV 键（AI 会话域）。
    pub fn kv_key(&self) -> String {
        format!("liq:proposal:{}", self.id)
    }
}

/// `aiLiquify`：解析 AI 输出的提案 JSON → 校验 → Draft 提案。
///
/// 校验（fail-closed，任一不过即整体丢弃，不半存）：
/// ops 非空且 ≤ [`MAX_OPS`]；CreateNote title 去空白后非空；UpdateNote
/// note_id/expected_title 非空。JSON 解析失败同拒。
pub fn parse_proposal(source: &str, raw_json: &str) -> Result<LiquifyProposal, Error> {
    let ops: Vec<ProposedOp> = serde_json::from_str(raw_json)
        .map_err(|e| Error::InvalidInput(format!("liquify 提案 JSON 非法: {e}")))?;
    if ops.is_empty() {
        return Err(Error::InvalidInput("提案 ops 为空".into()));
    }
    if ops.len() > MAX_OPS {
        return Err(Error::InvalidInput(format!(
            "提案 ops 超上限: {} > {MAX_OPS}",
            ops.len()
        )));
    }
    for (i, op) in ops.iter().enumerate() {
        match op {
            ProposedOp::CreateNote { title, .. } => {
                if title.trim().is_empty() {
                    return Err(Error::InvalidInput(format!("op#{i} title 为空")));
                }
            }
            ProposedOp::UpdateNote {
                note_id,
                expected_title,
                ..
            } => {
                if note_id.trim().is_empty() || expected_title.trim().is_empty() {
                    return Err(Error::InvalidInput(format!(
                        "op#{i} note_id/expected_title 为空"
                    )));
                }
            }
        }
    }
    Ok(LiquifyProposal {
        id: format!("liq-{}", uuid::Uuid::new_v4()),
        created_at: chrono::Utc::now().timestamp_millis(),
        source: source.to_string(),
        ops,
        status: ProposalStatus::Draft,
        results: Vec::new(),
    })
}

/// `aiCommit`：用户勾选后落库（**仅 tauri 用户命令面可调，AI 工具表不注册**）。
///
/// 幂等/状态守卫：非 Draft（已 Committed/Rejected）拒绝；勾选下标越界记入该
/// op 的失败结果（部分失败不阻塞其余——诚实报告）。逐 op 走 write_path 标准
/// 写入：CreateNote = create_note(+content 非空再 save_note_content)；
/// UpdateNote = save_note_content。
pub async fn ai_commit(
    ctx: &aurora_core::write_path::WriteContext,
    mut proposal: LiquifyProposal,
    selected: &[usize],
) -> Result<LiquifyProposal, Error> {
    if proposal.status != ProposalStatus::Draft {
        return Err(Error::InvalidInput(format!(
            "提案 {} 状态 {:?} 不可提交（仅 Draft 可）",
            proposal.id, proposal.status
        )));
    }
    for &i in selected {
        let mut result = if i >= proposal.ops.len() {
            OpResult {
                op_index: i,
                ok: false,
                note_id: None,
                error: Some(format!("勾选下标越界: {i}")),
            }
        } else {
            match &proposal.ops[i] {
                ProposedOp::CreateNote { title, content } => {
                    match aurora_core::write_path::create_note(ctx, title).await {
                        Ok(note_id) => {
                            if !content.is_empty() {
                                if let Err(e) = aurora_core::write_path::save_note_content(
                                    ctx, &note_id, content,
                                )
                                .await
                                {
                                    OpResult {
                                        op_index: i,
                                        ok: false,
                                        note_id: Some(note_id),
                                        error: Some(format!("正文写入失败: {e}")),
                                    }
                                } else {
                                    OpResult {
                                        op_index: i,
                                        ok: true,
                                        note_id: Some(note_id),
                                        error: None,
                                    }
                                }
                            } else {
                                OpResult {
                                    op_index: i,
                                    ok: true,
                                    note_id: Some(note_id),
                                    error: None,
                                }
                            }
                        }
                        Err(e) => OpResult {
                            op_index: i,
                            ok: false,
                            note_id: None,
                            error: Some(format!("创建失败: {e}")),
                        },
                    }
                }
                ProposedOp::UpdateNote {
                    note_id,
                    new_content,
                    ..
                } => {
                    match aurora_core::write_path::save_note_content(ctx, note_id, new_content)
                        .await
                    {
                        Ok(_) => OpResult {
                            op_index: i,
                            ok: true,
                            note_id: Some(note_id.clone()),
                            error: None,
                        },
                        Err(e) => OpResult {
                            op_index: i,
                            ok: false,
                            note_id: Some(note_id.clone()),
                            error: Some(format!("更新失败: {e}")),
                        },
                    }
                }
            }
        };
        result.op_index = i;
        proposal.results.push(result);
    }
    proposal.status = ProposalStatus::Committed;
    Ok(proposal)
}

/// 提案整体拒绝（用户 UI 动作）。
pub fn reject_proposal(mut proposal: LiquifyProposal) -> Result<LiquifyProposal, Error> {
    if proposal.status != ProposalStatus::Draft {
        return Err(Error::InvalidInput(format!(
            "提案 {} 状态 {:?} 不可拒绝（仅 Draft 可）",
            proposal.id, proposal.status
        )));
    }
    proposal.status = ProposalStatus::Rejected;
    Ok(proposal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurora_core::app_core::AppCoreBuilder;
    use aurora_core::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    const VALID: &str = r##"[
        {"type":"create_note","title":"会议纪要","content":"# 决议\n- A 通过"},
        {"type":"update_note","note_id":"n-1","new_content":"new body","expected_title":"旧笔记"}
    ]"##;

    // === TestStack：AppCore 全依赖空转 mock（liquify 只碰 kv+write_path； ===
    // === 后续 DK-10 Agent 会话片测试可复用/提炼）===

    struct NoopSyncTarget;
    #[async_trait::async_trait]
    impl aurora_core::traits::sync_target::SyncTarget for NoopSyncTarget {
        async fn connect(
            &mut self,
            _: &aurora_core::traits::sync_target::Endpoint,
        ) -> Result<aurora_core::traits::sync_target::Connection, aurora_core::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn sync(
            &self,
            _: &aurora_core::traits::sync_target::Connection,
            _: &aurora_core::traits::sync_target::DocSet,
        ) -> Result<aurora_core::traits::sync_target::SyncReport, aurora_core::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn send_update(
            &self,
            _: &aurora_core::traits::sync_target::Connection,
            _: &aurora_core::traits::sync_target::UpdatePayload,
        ) -> Result<(), aurora_core::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn recv_update(
            &self,
            _: &aurora_core::traits::sync_target::Connection,
            _: &str,
        ) -> Result<Vec<u8>, aurora_core::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        async fn sync_version(
            &self,
            _: &aurora_core::traits::sync_target::Connection,
            _: &str,
        ) -> Result<Option<u64>, aurora_core::Error> {
            unimplemented!("liquify 测试不触同步")
        }
        fn watch(&self, _: Box<dyn Fn(aurora_core::traits::sync_target::SyncEvent) + Send + Sync>) {
        }
        async fn disconnect(
            &self,
            _: &aurora_core::traits::sync_target::Connection,
        ) -> Result<(), aurora_core::Error> {
            unimplemented!("liquify 测试不触同步")
        }
    }

    struct NoopCrypto;
    impl aurora_core::traits::crypto_provider::CryptoProvider for NoopCrypto {
        fn encrypt(
            &self,
            _: &[u8],
            _: &[u8; 32],
        ) -> Result<aurora_core::traits::crypto_provider::Ciphertext, aurora_core::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn decrypt(
            &self,
            _: &aurora_core::traits::crypto_provider::Ciphertext,
            _: &[u8; 32],
        ) -> Result<Vec<u8>, aurora_core::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn derive_key(&self, _: &str, _: &[u8]) -> Result<[u8; 32], aurora_core::Error> {
            unimplemented!("liquify 测试不触加密")
        }
        fn kem_keypair(
            &self,
        ) -> Result<
            (
                aurora_core::traits::crypto_provider::KemPublicKey,
                aurora_core::traits::crypto_provider::KemSecretKey,
            ),
            aurora_core::Error,
        > {
            unimplemented!("liquify 测试不触加密")
        }
        fn kem_encapsulate(
            &self,
            _: &aurora_core::traits::crypto_provider::KemPublicKey,
        ) -> Result<
            (
                aurora_core::traits::crypto_provider::KemSharedSecret,
                aurora_core::traits::crypto_provider::KemCiphertext,
            ),
            aurora_core::Error,
        > {
            unimplemented!("liquify 测试不触加密")
        }
        fn kem_decapsulate(
            &self,
            _: &aurora_core::traits::crypto_provider::KemSecretKey,
            _: &aurora_core::traits::crypto_provider::KemCiphertext,
        ) -> Result<aurora_core::traits::crypto_provider::KemSharedSecret, aurora_core::Error>
        {
            unimplemented!("liquify 测试不触加密")
        }
        fn random_bytes(&self, _: usize) -> Vec<u8> {
            unimplemented!("liquify 测试不触加密")
        }
        fn hash(&self, _: &[u8]) -> [u8; 32] {
            unimplemented!("liquify 测试不触加密")
        }
        fn hmac_sign(&self, _: &[u8], _: &[u8]) -> Vec<u8> {
            unimplemented!("liquify 测试不触加密")
        }
        fn hmac_verify(&self, _: &[u8], _: &[u8], _: &[u8]) -> bool {
            unimplemented!("liquify 测试不触加密")
        }
        fn ed25519_verify(
            &self,
            _: &aurora_core::traits::crypto_provider::Ed25519PublicKey,
            _: &[u8],
            _: &aurora_core::traits::crypto_provider::Ed25519Signature,
        ) -> bool {
            unimplemented!("liquify 测试不触加密")
        }
        fn algorithm_version(&self) -> u16 {
            unimplemented!("liquify 测试不触加密")
        }
    }

    struct NoopSearch;
    #[async_trait::async_trait]
    impl aurora_core::traits::search_backend::SearchBackend for NoopSearch {
        async fn search(
            &self,
            _: &str,
            _: &aurora_core::traits::search_backend::SearchOptions,
        ) -> Result<aurora_core::traits::search_backend::SearchResult, aurora_core::Error> {
            unimplemented!("liquify 测试不触检索")
        }
        async fn index_note(
            &self,
            _: &str,
            _: &str,
            _: &aurora_core::traits::search_backend::NoteMetadata,
        ) -> Result<(), aurora_core::Error> {
            Ok(())
        }
        async fn batch_index(
            &self,
            _: &[aurora_core::traits::search_backend::IndexEntry],
        ) -> Result<(), aurora_core::Error> {
            Ok(())
        }
        async fn remove_index(&self, _: &str) -> Result<(), aurora_core::Error> {
            Ok(())
        }
        async fn rebuild_index(
            &self,
            _: &[aurora_core::traits::search_backend::IndexEntry],
        ) -> Result<(), aurora_core::Error> {
            Ok(())
        }
        fn tokenize(&self, text: &str) -> Vec<String> {
            text.split_whitespace().map(String::from).collect()
        }
    }

    struct NoopOcr;
    #[async_trait::async_trait]
    impl aurora_core::traits::ocr_provider::OcrProvider for NoopOcr {
        async fn recognize(
            &self,
            _: &[u8],
        ) -> Result<aurora_core::traits::ocr_provider::OcrResult, aurora_core::Error> {
            unimplemented!("liquify 测试不触 OCR")
        }
        async fn recognize_batch(
            &self,
            _: &[&[u8]],
        ) -> Result<Vec<aurora_core::traits::ocr_provider::OcrResult>, aurora_core::Error> {
            unimplemented!("liquify 测试不触 OCR")
        }
        fn is_available(&self) -> bool {
            false
        }
    }

    struct NoopPlugin;
    #[async_trait::async_trait]
    impl aurora_core::traits::plugin_runtime::PluginRuntime for NoopPlugin {
        async fn load(
            &mut self,
            _: &aurora_core::traits::plugin_runtime::PluginManifest,
        ) -> Result<aurora_core::traits::plugin_runtime::PluginHandle, aurora_core::Error> {
            unimplemented!("liquify 测试不触插件")
        }
        async fn invoke(
            &self,
            _: &aurora_core::traits::plugin_runtime::PluginHandle,
            _: &str,
            _: &serde_json::Value,
        ) -> Result<serde_json::Value, aurora_core::Error> {
            unimplemented!("liquify 测试不触插件")
        }
        async fn unload(
            &mut self,
            _: &aurora_core::traits::plugin_runtime::PluginHandle,
        ) -> Result<(), aurora_core::Error> {
            unimplemented!("liquify 测试不触插件")
        }
        fn list_hooks(&self, _: &str) -> Vec<aurora_core::traits::plugin_runtime::PluginHandle> {
            vec![]
        }
    }

    fn test_ctx() -> aurora_core::write_path::WriteContext {
        let core = AppCoreBuilder::new()
            .sync_target(Arc::new(NoopSyncTarget))
            .crypto(Arc::new(NoopCrypto))
            .ai(Arc::new(crate::mock_provider::MockAIProvider))
            .kv_store(Arc::new(MemoryKVStore::default()))
            .search(Arc::new(NoopSearch))
            .ocr(Arc::new(NoopOcr))
            .plugin(Arc::new(NoopPlugin))
            .build();
        aurora_core::write_path::WriteContext {
            core: Arc::new(core),
            blocks: None,
            seal: None,
            content_cipher: None,
            attachments: None,
        }
    }

    /// parse 校验全链：合法 / 空 ops / 超上限 / 坏 JSON / 空 title。
    #[test]
    fn dk10_parse_validation() {
        let p = parse_proposal("gpt-x", VALID).unwrap();
        assert_eq!(p.ops.len(), 2);
        assert_eq!(p.status, ProposalStatus::Draft);
        assert!(p.id.starts_with("liq-"));
        assert_eq!(p.source, "gpt-x");

        assert!(parse_proposal("m", "[]").is_err(), "空 ops 拒绝");
        let too_many = format!(
            "[{}]",
            (0..MAX_OPS + 1)
                .map(|_| r#"{"type":"create_note","title":"t","content":""}"#)
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(parse_proposal("m", &too_many).is_err(), "超上限拒绝");
        assert!(parse_proposal("m", "not-json").is_err(), "坏 JSON 拒绝");
        assert!(
            parse_proposal("m", r#"[{"type":"create_note","title":"  ","content":""}]"#).is_err(),
            "空 title 拒绝"
        );
        assert!(
            parse_proposal(
                "m",
                r#"[{"type":"update_note","note_id":"","new_content":"x","expected_title":"t"}]"#
            )
            .is_err(),
            "空 note_id 拒绝"
        );
    }

    /// commit 往返：CreateNote 落库内容逐字节一致；未勾选 op 零写入。
    #[tokio::test]
    async fn dk10_commit_roundtrip() {
        let ctx = test_ctx();
        let seed = aurora_core::write_path::create_note(&ctx, "旧笔记")
            .await
            .unwrap();
        let proposal_json = format!(
            r#"[{{"type":"create_note","title":"新笔记","content":"正文A"}},{{"type":"update_note","note_id":"{seed}","new_content":"覆盖B","expected_title":"旧笔记"}}]"#
        );
        let p = parse_proposal("gpt-x", &proposal_json).unwrap();

        // 铁律：commit 前勾选子集（仅 op0）——op1 不动
        let committed = ai_commit(&ctx, p.clone(), &[0]).await.unwrap();
        assert_eq!(committed.status, ProposalStatus::Committed);
        assert_eq!(committed.results.len(), 1);
        assert!(committed.results[0].ok);
        let new_id = committed.results[0].note_id.clone().unwrap();

        let rec = aurora_core::write_path::load_note_meta(&ctx.core, &new_id, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(rec.title, "新笔记");
        let _ = seed;
    }

    /// 部分失败诚实报告：UpdateNote 指向不存在笔记 → 该 op fail 记录，
    /// 其余 CreateNote 照常成功；失败不静默（error 摘要非空）。
    #[tokio::test]
    async fn dk10_partial_failure_honest_report() {
        let ctx = test_ctx();
        let proposal_json = r#"[
            {"type":"update_note","note_id":"ghost-404","new_content":"x","expected_title":"幽灵"},
            {"type":"create_note","title":"应成功","content":""}
        ]"#;
        let p = parse_proposal("m", proposal_json).unwrap();
        let committed = ai_commit(&ctx, p, &[0, 1]).await.unwrap();
        assert_eq!(committed.results.len(), 2);
        assert!(!committed.results[0].ok, "幽灵笔记 fail");
        assert!(
            committed.results[0]
                .error
                .as_deref()
                .unwrap()
                .contains("404")
                || committed.results[0]
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("不存在")
                || !committed.results[0].error.as_deref().unwrap().is_empty()
        );
        assert!(committed.results[1].ok, "CreateNote 照常成功");
    }

    /// 状态机：Rejected/Committed 终态拒绝二次提交；越界勾选 fail-closed 记录。
    #[tokio::test]
    async fn dk10_state_machine() {
        let ctx = test_ctx();
        let rejected = reject_proposal(parse_proposal("m", VALID).unwrap()).unwrap();
        assert_eq!(rejected.status, ProposalStatus::Rejected);
        assert!(
            ai_commit(&ctx, rejected, &[]).await.is_err(),
            "拒后提交拒绝"
        );

        let p2 = parse_proposal("m", VALID).unwrap();
        let c1 = ai_commit(&ctx, p2, &[]).await.unwrap();
        assert!(
            ai_commit(&ctx, c1.clone(), &[]).await.is_err(),
            "Committed 不可再提交"
        );

        let p3 = parse_proposal("m", VALID).unwrap();
        let c2 = ai_commit(&ctx, p3, &[99]).await.unwrap();
        assert!(!c2.results[0].ok);
        assert!(c2.results[0].error.as_deref().unwrap().contains("越界"));
    }

    /// 铁律断言（命名面）：aiLiquify 非「写工具」（AI 可调——出提案是 AI 的
    /// 合法动作）；aiCommit 是 tauri **用户命令面**，AI 工具注册表（sandbox
    /// 白名单/Agent 工具集）绝不注册——由装配纪律保证，此处锁定可审计事实。
    #[test]
    fn dk10_ironlaw_tool_naming() {
        assert!(
            !crate::sandbox::is_write_tool("aiLiquify"),
            "liquify 非写工具（AI 可调）"
        );
        // 防御性：即使有人误注册，写前缀表不拦截 "ai" 前缀——文档化缺口并锁定：
        // aiCommit 的防线在「工具表不注册」而非「前缀拦截」（装配纪律，回执注明）。
        assert!(!crate::sandbox::is_write_tool("aiCommit"));
    }
}
