//! L3 领域服务层（Domain Service Layer）
//!
//! 包含内容编辑、知识网络、GTD效能、AI智能等P0领域服务模块，
//! 以及导入导出、素材库、系统设置、TodayView、OCR服务等P1领域服务模块。

pub mod content_editor;
pub mod knowledge_network;
pub mod gtd_system;
pub mod ai_system;

pub mod import_export;
pub mod asset_library;
pub mod system_settings;
pub mod today_view;
pub mod ocr_service;
pub mod capture_matrix;

/// V19 DEF-003 五子层架构视图（core_data / knowledge / intelligence /
/// productivity / integration），含依赖方向约束说明。
pub mod sublayers;
