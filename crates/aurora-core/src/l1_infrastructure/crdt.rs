//! CRDT 引擎 (基于 Loro)
//!
//! 提供无冲突复制数据类型 (CRDT) 能力，支持多端协同编辑与离线合并。
//! 底层使用 [Loro](https://loro.dev) 实现，确保协议长期稳定。

/// Loro CRDT 文档类型再导出。
///
/// 供 `traits::CrdtEngine` Trait 的 `create_document` 返回值使用。
/// `loro` 已作为 workspace 依赖加入，此处直接再导出真实类型。
pub use loro::LoroDoc;

/// CRDT 文档句柄占位类型。
///
/// 实际实现将在后续任务中封装 Loro 的 `LoroDoc` 等能力。
pub struct CrdtDoc;
