//! 领域服务层 5 子层架构映射（V19 §10.2.2 / DEF-003）
//!
//! V19 将原扁平的领域服务模块重组为 5 个职责子层，解决模块膨胀与
//! 依赖方向不明问题。本模块以**重新导出**方式建立子层视图，不移动
//! 既有文件，保持向后兼容；新增模块应按子层归属放置并在此登记。
//!
//! # 子层划分与依赖规则
//!
//! ```text
//! ┌─ integration（开放集成层）─────────────────────────┐
//! │   import_export · （预留: agent_gateway / plugin / │
//! │   template / external_sync — 当前在独立 crate 中）  │
//! ├─ productivity（效能管理层）────────────────────────┤
//! │   gtd_system · today_view · capture_matrix        │
//! ├─ intelligence（智能服务层）────────────────────────┤
//! │   ai_system · ocr_service                         │
//! ├─ knowledge（知识网络层）───────────────────────────┤
//! │   knowledge_network · （查询引擎见 L2 query.rs）    │
//! ├─ core_data（核心数据层）───────────────────────────┤
//! │   content_editor · asset_library · system_settings│
//! └───────────────────────────────────────────────────┘
//! ```
//!
//! **依赖规则（V19 强制）**：
//! - 上层可依赖下层，下层**禁止**依赖上层；
//! - 同层模块间禁止直接调用，须经 [`crate::event_bus`] 解耦；
//! - 密码学能力为跨层服务，所有子层经
//!   [`crate::traits::crypto_provider::CryptoProvider`] 访问，
//!   禁止直接依赖 ring / aes-gcm 等具体实现。

/// 核心数据层：文档编辑、存储、素材与配置。
///
/// 仅允许依赖基础设施层（L1/L2），不得依赖任何其他子层。
pub mod core_data {
    pub use crate::l3_domain::asset_library;
    pub use crate::l3_domain::content_editor;
    pub use crate::l3_domain::system_settings;
}

/// 知识网络层：双链、搜索、自然语言查询。
///
/// 依赖方向：仅可依赖 [`core_data`]。
pub mod knowledge {
    pub use crate::l3_domain::knowledge_network;
}

/// 智能服务层：AI 推理、智能体、OCR。
///
/// 依赖方向：仅可依赖 [`knowledge`] 及以下。
pub mod intelligence {
    pub use crate::l3_domain::ai_system;
    pub use crate::l3_domain::ocr_service;
}

/// 效能管理层：GTD 工作流、今日视图、捕获矩阵。
///
/// 依赖方向：仅可依赖 [`intelligence`] 及以下。
pub mod productivity {
    pub use crate::l3_domain::capture_matrix;
    pub use crate::l3_domain::gtd_system;
    pub use crate::l3_domain::today_view;
}

/// 开放集成层：导入导出、Agent 接入、外部同步、插件、模板。
///
/// 依赖方向：仅可依赖 [`productivity`] 及以下。
/// 注：AgentGateway / PluginSystem / ExternalSyncHub 当前以独立 crate
/// （`aurora-ai` / `aurora-plugin` / `aurora-sync`）实现，属本子层逻辑归属。
pub mod integration {
    pub use crate::l3_domain::import_export;
}
