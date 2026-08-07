//! 系统设置（System Settings）
//!
//! 实现设置分层存储（system → user → workspace）、主题系统、快捷键配置。
//!
//! # 简化说明
//! - 设置存储用内存 `HashMap` 模拟 SQLite，不真正落盘。
//! - 主题系统输出 CSS 变量形式的设计令牌（design tokens）。
//! - 快捷键冲突检测基于「平台 + 键位」精确匹配。

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, warn};

/// 分层设置映射： (layer, workspace_id) → LayerSettings。
type LayerMap = HashMap<(SettingsLayer, Option<String>), LayerSettings>;
/// 迁移函数：接收 SettingsStore 引用执行迁移。
type MigrationFn = Box<dyn Fn(&SettingsStore)>;

// ============================================================================
// SubTask 3.6.1: 设置分层存储
// ============================================================================

/// 设置层级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SettingsLayer {
    /// 系统级（默认值）
    System,
    /// 用户级（跨工作区）
    User,
    /// 工作区级（特定 workspace）
    Workspace,
}

impl SettingsLayer {
    /// 优先级数值（越大优先级越高）
    pub fn priority(&self) -> u8 {
        match self {
            SettingsLayer::System => 1,
            SettingsLayer::User => 2,
            SettingsLayer::Workspace => 3,
        }
    }
}

/// 设置版本（用于迁移）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsVersion {
    pub schema_version: u32,
    pub migrated_at: chrono::DateTime<chrono::Utc>,
    pub migrations_applied: Vec<String>,
}

impl Default for SettingsVersion {
    fn default() -> Self {
        Self {
            schema_version: 1,
            migrated_at: chrono::Utc::now(),
            migrations_applied: Vec::new(),
        }
    }
}

/// 一层设置数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerSettings {
    pub layer: SettingsLayer,
    /// workspace_id（仅 Workspace 层有意义）
    pub workspace_id: Option<String>,
    pub values: HashMap<String, serde_json::Value>,
}

impl LayerSettings {
    pub fn new(layer: SettingsLayer) -> Self {
        Self {
            layer,
            workspace_id: None,
            values: HashMap::new(),
        }
    }

    pub fn for_workspace(workspace_id: impl Into<String>) -> Self {
        Self {
            layer: SettingsLayer::Workspace,
            workspace_id: Some(workspace_id.into()),
            values: HashMap::new(),
        }
    }
}

/// 设置存储（分层 + 合并 + 版本迁移）
pub struct SettingsStore {
    /// (layer, workspace_id) → LayerSettings
    layers: Arc<RwLock<LayerMap>>,
    version: Arc<RwLock<SettingsVersion>>,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsStore {
    pub fn new() -> Self {
        let mut layers = HashMap::new();
        // 系统层默认存在
        layers.insert(
            (SettingsLayer::System, None),
            LayerSettings::new(SettingsLayer::System),
        );
        Self {
            layers: Arc::new(RwLock::new(layers)),
            version: Arc::new(RwLock::new(SettingsVersion::default())),
        }
    }

    /// 写入某层的某个键
    pub fn set(
        &self,
        layer: SettingsLayer,
        workspace_id: Option<&str>,
        key: &str,
        value: serde_json::Value,
    ) {
        let ws = workspace_id.map(|s| s.to_string());
        let mut layers = self.layers.write();
        let entry = layers
            .entry((layer, ws.clone()))
            .or_insert_with(|| LayerSettings {
                layer,
                workspace_id: ws,
                values: HashMap::new(),
            });
        entry.values.insert(key.to_string(), value);
        debug!(?layer, key, "setting updated");
    }

    /// 读取某层某键（不合并）
    pub fn get_layer_value(
        &self,
        layer: SettingsLayer,
        workspace_id: Option<&str>,
        key: &str,
    ) -> Option<serde_json::Value> {
        let ws = workspace_id.map(|s| s.to_string());
        self.layers
            .read()
            .get(&(layer, ws))
            .and_then(|l| l.values.get(key).cloned())
    }

    /// 合并读取：Workspace > User > System
    pub fn get_effective(
        &self,
        workspace_id: Option<&str>,
        key: &str,
    ) -> Option<serde_json::Value> {
        let layers = self.layers.read();
        // 优先级从高到低
        if let Some(ws) = workspace_id {
            if let Some(l) = layers.get(&(SettingsLayer::Workspace, Some(ws.to_string()))) {
                if let Some(v) = l.values.get(key) {
                    return Some(v.clone());
                }
            }
        }
        if let Some(l) = layers.get(&(SettingsLayer::User, None)) {
            if let Some(v) = l.values.get(key) {
                return Some(v.clone());
            }
        }
        if let Some(l) = layers.get(&(SettingsLayer::System, None)) {
            if let Some(v) = l.values.get(key) {
                return Some(v.clone());
            }
        }
        None
    }

    /// 合并读取整层（所有键）
    pub fn get_effective_all(
        &self,
        workspace_id: Option<&str>,
    ) -> HashMap<String, serde_json::Value> {
        let layers = self.layers.read();
        let mut merged: HashMap<String, serde_json::Value> = HashMap::new();
        // 从低到高合并
        if let Some(l) = layers.get(&(SettingsLayer::System, None)) {
            for (k, v) in &l.values {
                merged.insert(k.clone(), v.clone());
            }
        }
        if let Some(l) = layers.get(&(SettingsLayer::User, None)) {
            for (k, v) in &l.values {
                merged.insert(k.clone(), v.clone());
            }
        }
        if let Some(ws) = workspace_id {
            if let Some(l) = layers.get(&(SettingsLayer::Workspace, Some(ws.to_string()))) {
                for (k, v) in &l.values {
                    merged.insert(k.clone(), v.clone());
                }
            }
        }
        merged
    }

    /// 删除某层某键
    pub fn unset(&self, layer: SettingsLayer, workspace_id: Option<&str>, key: &str) -> bool {
        let ws = workspace_id.map(|s| s.to_string());
        let mut layers = self.layers.write();
        if let Some(l) = layers.get_mut(&(layer, ws)) {
            l.values.remove(key).is_some()
        } else {
            false
        }
    }

    /// 迁移：应用一组迁移函数（按顺序执行，记录已应用）
    pub fn migrate(&self, migrations: Vec<(String, MigrationFn)>) {
        let mut version = self.version.write();
        for (name, f) in migrations {
            if version.migrations_applied.contains(&name) {
                warn!(migration = %name, "already applied, skipping");
                continue;
            }
            drop(version); // 释放锁以允许迁移函数访问 store
            f(self);
            version = self.version.write();
            version.migrations_applied.push(name.clone());
            version.schema_version += 1;
            version.migrated_at = chrono::Utc::now();
            debug!(migration = %name, "applied");
        }
    }

    pub fn version(&self) -> SettingsVersion {
        self.version.read().clone()
    }
}

// ============================================================================
// SubTask 3.6.2: 主题系统
// ============================================================================

/// 主题模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Light,
    Dark,
    Sepia,
    HighContrast,
    /// 跟随系统
    Auto,
}

/// 主题定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub mode: ThemeMode,
    pub tokens: DesignTokens,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    pub fn light() -> Self {
        Self {
            name: "Light".to_string(),
            mode: ThemeMode::Light,
            tokens: DesignTokens::light(),
        }
    }

    pub fn dark() -> Self {
        Self {
            name: "Dark".to_string(),
            mode: ThemeMode::Dark,
            tokens: DesignTokens::dark(),
        }
    }

    pub fn sepia() -> Self {
        Self {
            name: "Sepia".to_string(),
            mode: ThemeMode::Sepia,
            tokens: DesignTokens::sepia(),
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            name: "HighContrast".to_string(),
            mode: ThemeMode::HighContrast,
            tokens: DesignTokens::high_contrast(),
        }
    }
}

/// 设计令牌（CSS 变量形式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignTokens {
    pub bg_primary: String,
    pub bg_secondary: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub accent: String,
    pub border: String,
    pub font_size_base: String,
}

impl DesignTokens {
    pub fn light() -> Self {
        Self {
            bg_primary: "#ffffff".into(),
            bg_secondary: "#f5f5f5".into(),
            text_primary: "#1a1a1a".into(),
            text_secondary: "#666666".into(),
            accent: "#0066cc".into(),
            border: "#e0e0e0".into(),
            font_size_base: "16px".into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            bg_primary: "#1a1a1a".into(),
            bg_secondary: "#2a2a2a".into(),
            text_primary: "#f5f5f5".into(),
            text_secondary: "#a0a0a0".into(),
            accent: "#4d9fff".into(),
            border: "#3a3a3a".into(),
            font_size_base: "16px".into(),
        }
    }

    pub fn sepia() -> Self {
        Self {
            bg_primary: "#f4ecd8".into(),
            bg_secondary: "#e8dec5".into(),
            text_primary: "#5b4636".into(),
            text_secondary: "#8a7a66".into(),
            accent: "#9c6b3f".into(),
            border: "#d4c4a8".into(),
            font_size_base: "16px".into(),
        }
    }

    pub fn high_contrast() -> Self {
        Self {
            bg_primary: "#000000".into(),
            bg_secondary: "#1a1a1a".into(),
            text_primary: "#ffffff".into(),
            text_secondary: "#ffff00".into(),
            accent: "#ffff00".into(),
            border: "#ffffff".into(),
            font_size_base: "18px".into(),
        }
    }

    /// 合并：以 self 为基底，用 override 中非空字段覆盖
    pub fn merge(&self, override_tokens: &DesignTokens) -> DesignTokens {
        DesignTokens {
            bg_primary: if override_tokens.bg_primary.is_empty() {
                self.bg_primary.clone()
            } else {
                override_tokens.bg_primary.clone()
            },
            bg_secondary: if override_tokens.bg_secondary.is_empty() {
                self.bg_secondary.clone()
            } else {
                override_tokens.bg_secondary.clone()
            },
            text_primary: if override_tokens.text_primary.is_empty() {
                self.text_primary.clone()
            } else {
                override_tokens.text_primary.clone()
            },
            text_secondary: if override_tokens.text_secondary.is_empty() {
                self.text_secondary.clone()
            } else {
                override_tokens.text_secondary.clone()
            },
            accent: if override_tokens.accent.is_empty() {
                self.accent.clone()
            } else {
                override_tokens.accent.clone()
            },
            border: if override_tokens.border.is_empty() {
                self.border.clone()
            } else {
                override_tokens.border.clone()
            },
            font_size_base: if override_tokens.font_size_base.is_empty() {
                self.font_size_base.clone()
            } else {
                override_tokens.font_size_base.clone()
            },
        }
    }

    /// 序列化为 CSS 变量字符串
    pub fn to_css_variables(&self) -> String {
        format!(
            ":root {{\n  --bg-primary: {};\n  --bg-secondary: {};\n  --text-primary: {};\n  --text-secondary: {};\n  --accent: {};\n  --border: {};\n  --font-size-base: {};\n}}",
            self.bg_primary,
            self.bg_secondary,
            self.text_primary,
            self.text_secondary,
            self.accent,
            self.border,
            self.font_size_base
        )
    }
}

/// 主题管理器
pub struct ThemeManager {
    current: Arc<RwLock<Theme>>,
    /// 用户自定义令牌覆盖
    overrides: Arc<RwLock<DesignTokens>>,
}

impl Default for ThemeManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ThemeManager {
    pub fn new() -> Self {
        Self {
            current: Arc::new(RwLock::new(Theme::light())),
            overrides: Arc::new(RwLock::new(DesignTokens {
                bg_primary: String::new(),
                bg_secondary: String::new(),
                text_primary: String::new(),
                text_secondary: String::new(),
                accent: String::new(),
                border: String::new(),
                font_size_base: String::new(),
            })),
        }
    }

    pub fn set_mode(&self, mode: ThemeMode) {
        let theme = match mode {
            ThemeMode::Light => Theme::light(),
            ThemeMode::Dark => Theme::dark(),
            ThemeMode::Sepia => Theme::sepia(),
            ThemeMode::HighContrast => Theme::high_contrast(),
            ThemeMode::Auto => Theme::light(), // 简化：Auto 默认 Light
        };
        *self.current.write() = theme;
    }

    pub fn set_override(&self, tokens: DesignTokens) {
        *self.overrides.write() = tokens;
    }

    /// 获取当前主题（应用 override 后的合并结果）
    pub fn current_theme(&self) -> Theme {
        let cur = self.current.read().clone();
        let ov = self.overrides.read().clone();
        Theme {
            name: cur.name,
            mode: cur.mode,
            tokens: cur.tokens.merge(&ov),
        }
    }

    pub fn css_variables(&self) -> String {
        self.current_theme().tokens.to_css_variables()
    }
}

// ============================================================================
// SubTask 3.6.3: 快捷键配置
// ============================================================================

/// 平台
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    Mac,
    Windows,
    Linux,
}

impl Platform {
    /// 返回该平台的修饰键显示名（Mac 显示 ⌘，其他显示 Ctrl）
    pub fn primary_modifier(&self) -> &'static str {
        match self {
            Platform::Mac => "Cmd",
            Platform::Windows | Platform::Linux => "Ctrl",
        }
    }
}

/// 快捷键作用域
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutScope {
    /// 全局快捷键
    Global,
    /// 编辑器快捷键
    Editor,
}

/// 快捷键绑定
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ShortcutBinding {
    pub scope: ShortcutScope,
    /// 修饰键列表（如 ["Ctrl", "Shift"]）
    pub modifiers: Vec<String>,
    /// 主键（如 "K", "Enter"）
    pub key: String,
}

impl ShortcutBinding {
    pub fn new(scope: ShortcutScope, key: impl Into<String>, modifiers: Vec<String>) -> Self {
        Self {
            scope,
            modifiers,
            key: key.into(),
        }
    }

    /// 平台适配：Mac 上 Ctrl → Cmd
    pub fn for_platform(&self, platform: Platform) -> Self {
        let modifiers = self
            .modifiers
            .iter()
            .map(|m| {
                if platform == Platform::Mac && m == "Ctrl" {
                    "Cmd".to_string()
                } else if platform != Platform::Mac && m == "Cmd" {
                    "Ctrl".to_string()
                } else {
                    m.clone()
                }
            })
            .collect();
        Self {
            scope: self.scope,
            modifiers,
            key: self.key.clone(),
        }
    }

    /// 规范化键签名（用于冲突检测）
    pub fn signature(&self) -> String {
        let mut mods = self.modifiers.clone();
        mods.sort();
        format!(
            "{:?}|{}|{}",
            self.scope,
            mods.join("+"),
            self.key.to_lowercase()
        )
    }
}

/// 快捷键定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Shortcut {
    pub id: String,
    pub name: String,
    pub description: String,
    pub binding: ShortcutBinding,
    pub platform: Platform,
}

/// 冲突检测结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShortcutConflict {
    pub signature: String,
    pub shortcuts: Vec<String>,
}

/// 快捷键管理器
pub struct ShortcutManager {
    shortcuts: Arc<RwLock<Vec<Shortcut>>>,
}

impl Default for ShortcutManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ShortcutManager {
    pub fn new() -> Self {
        Self {
            shortcuts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn register(&self, shortcut: Shortcut) {
        self.shortcuts.write().push(shortcut);
    }

    pub fn list(&self) -> Vec<Shortcut> {
        self.shortcuts.read().clone()
    }

    pub fn find(&self, id: &str) -> Option<Shortcut> {
        self.shortcuts.read().iter().find(|s| s.id == id).cloned()
    }

    pub fn update_binding(&self, id: &str, binding: ShortcutBinding) -> bool {
        let mut shortcuts = self.shortcuts.write();
        if let Some(s) = shortcuts.iter_mut().find(|s| s.id == id) {
            s.binding = binding;
            true
        } else {
            false
        }
    }

    /// 检测所有冲突（同平台 + 同签名）
    pub fn detect_conflicts(&self) -> Vec<ShortcutConflict> {
        let shortcuts = self.shortcuts.read();
        let mut by_sig: HashMap<(Platform, String), Vec<String>> = HashMap::new();
        for s in shortcuts.iter() {
            by_sig
                .entry((s.platform, s.binding.signature()))
                .or_default()
                .push(s.id.clone());
        }
        by_sig
            .into_iter()
            .filter(|(_, ids)| ids.len() > 1)
            .map(|((_, sig), ids)| ShortcutConflict {
                signature: sig,
                shortcuts: ids,
            })
            .collect()
    }

    /// 获取某平台下的快捷键列表（应用平台适配）
    pub fn for_platform(&self, platform: Platform) -> Vec<Shortcut> {
        self.shortcuts
            .read()
            .iter()
            .filter(|s| s.platform == platform)
            .map(|s| {
                let mut copy = s.clone();
                copy.binding = copy.binding.for_platform(platform);
                copy
            })
            .collect()
    }
}

// ============================================================================
// 顶层系统设置
// ============================================================================

/// AI 推理设置（V19 §7.2 功能依赖懒加载 + §13.1 混合推理架构）
///
/// 本地 Ollama 作为 LocalFirst 默认，本地不可用时降级到云端 OpenAI 兼容 API。
/// `cloud_*` 字段任一缺失则不启用云端 Provider（仅本地）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AISettings {
    /// 本地 Ollama HTTP 基地址（默认 `http://localhost:11434`）。
    pub ollama_base_url: String,
    /// 本地 Ollama 模型名（首次使用前需 `ollama pull <model>`）。
    pub ollama_model: String,
    /// 云端 OpenAI 兼容 API 基地址（如 `https://api.openai.com/v1`）。缺省则不启用云端。
    pub cloud_base_url: Option<String>,
    /// 云端 API Key（Bearer）。缺省则不启用云端。
    pub cloud_api_key: Option<String>,
    /// 云端模型名（如 `gpt-4o-mini`）。缺省则不启用云端。
    pub cloud_model: Option<String>,
    /// 推理策略：LocalFirst / CloudOnly / Auto（复用 `ai_system::InferenceStrategy`）。
    #[serde(default = "default_inference_strategy")]
    pub strategy: crate::l3_domain::ai_system::InferenceStrategy,
}

fn default_inference_strategy() -> crate::l3_domain::ai_system::InferenceStrategy {
    crate::l3_domain::ai_system::InferenceStrategy::LocalFirst
}

impl Default for AISettings {
    fn default() -> Self {
        Self {
            ollama_base_url: "http://localhost:11434".into(),
            ollama_model: "llama3.2".into(),
            cloud_base_url: None,
            cloud_api_key: None,
            cloud_model: None,
            strategy: crate::l3_domain::ai_system::InferenceStrategy::LocalFirst,
        }
    }
}

impl AISettings {
    /// 云端 Provider 所需的三元组是否齐备（base_url + key + model 均存在）。
    pub fn cloud_configured(&self) -> bool {
        self.cloud_base_url.is_some() && self.cloud_api_key.is_some() && self.cloud_model.is_some()
    }
}

/// 系统设置顶层聚合
pub struct SystemSettings {
    pub store: SettingsStore,
    pub theme: ThemeManager,
    pub shortcuts: ShortcutManager,
    /// AI 推理设置（V19 §7.2 / §13.1）。
    pub ai: AISettings,
}

impl Default for SystemSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemSettings {
    pub fn new() -> Self {
        Self {
            store: SettingsStore::new(),
            theme: ThemeManager::new(),
            shortcuts: ShortcutManager::new(),
            ai: AISettings::default(),
        }
    }
}

/// 设置 schema 描述（用于前端展示与校验）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSchema {
    pub key: String,
    pub description: String,
    pub default_value: serde_json::Value,
    pub layer: SettingsLayer,
    pub editable: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Layered storage ---

    #[test]
    fn test_settings_layer_priority() {
        assert!(SettingsLayer::Workspace.priority() > SettingsLayer::User.priority());
        assert!(SettingsLayer::User.priority() > SettingsLayer::System.priority());
    }

    #[test]
    fn test_settings_set_get_layer() {
        let store = SettingsStore::new();
        store.set(SettingsLayer::System, None, "lang", serde_json::json!("en"));
        assert_eq!(
            store.get_layer_value(SettingsLayer::System, None, "lang"),
            Some(serde_json::json!("en"))
        );
    }

    #[test]
    fn test_settings_effective_resolution() {
        let store = SettingsStore::new();
        store.set(
            SettingsLayer::System,
            None,
            "theme",
            serde_json::json!("light"),
        );
        store.set(
            SettingsLayer::User,
            None,
            "theme",
            serde_json::json!("dark"),
        );
        // User 覆盖 System
        assert_eq!(
            store.get_effective(None, "theme"),
            Some(serde_json::json!("dark"))
        );
    }

    #[test]
    fn test_settings_workspace_overrides_user() {
        let store = SettingsStore::new();
        store.set(SettingsLayer::User, None, "lang", serde_json::json!("en"));
        store.set(
            SettingsLayer::Workspace,
            Some("ws1"),
            "lang",
            serde_json::json!("zh"),
        );
        assert_eq!(
            store.get_effective(Some("ws1"), "lang"),
            Some(serde_json::json!("zh"))
        );
        // 不同 workspace 不受影响
        assert_eq!(
            store.get_effective(Some("ws2"), "lang"),
            Some(serde_json::json!("en"))
        );
    }

    #[test]
    fn test_settings_effective_all_merge() {
        let store = SettingsStore::new();
        store.set(SettingsLayer::System, None, "a", serde_json::json!(1));
        store.set(SettingsLayer::User, None, "b", serde_json::json!(2));
        store.set(
            SettingsLayer::Workspace,
            Some("ws1"),
            "c",
            serde_json::json!(3),
        );
        let merged = store.get_effective_all(Some("ws1"));
        assert_eq!(merged.get("a"), Some(&serde_json::json!(1)));
        assert_eq!(merged.get("b"), Some(&serde_json::json!(2)));
        assert_eq!(merged.get("c"), Some(&serde_json::json!(3)));
    }

    #[test]
    fn test_settings_unset() {
        let store = SettingsStore::new();
        store.set(SettingsLayer::User, None, "k", serde_json::json!("v"));
        assert!(store.unset(SettingsLayer::User, None, "k"));
        assert!(store
            .get_layer_value(SettingsLayer::User, None, "k")
            .is_none());
    }

    #[test]
    fn test_settings_migration() {
        let store = SettingsStore::new();
        let initial = store.version().schema_version;
        store.migrate(vec![
            (
                "m1".to_string(),
                Box::new(|s: &SettingsStore| {
                    s.set(
                        SettingsLayer::System,
                        None,
                        "migrated",
                        serde_json::json!(true),
                    );
                }) as Box<dyn Fn(&SettingsStore)>,
            ),
            (
                "m2".to_string(),
                Box::new(|s: &SettingsStore| {
                    s.set(
                        SettingsLayer::System,
                        None,
                        "migrated2",
                        serde_json::json!(true),
                    );
                }) as Box<dyn Fn(&SettingsStore)>,
            ),
        ]);
        let v = store.version();
        assert_eq!(v.schema_version, initial + 2);
        assert!(v.migrations_applied.contains(&"m1".to_string()));
        assert!(v.migrations_applied.contains(&"m2".to_string()));
        assert_eq!(
            store.get_layer_value(SettingsLayer::System, None, "migrated"),
            Some(serde_json::json!(true))
        );
    }

    // --- Theme ---

    #[test]
    fn test_theme_modes() {
        assert_eq!(Theme::light().mode, ThemeMode::Light);
        assert_eq!(Theme::dark().mode, ThemeMode::Dark);
        assert_eq!(Theme::sepia().mode, ThemeMode::Sepia);
        assert_eq!(Theme::high_contrast().mode, ThemeMode::HighContrast);
    }

    #[test]
    fn test_theme_manager_switch() {
        let tm = ThemeManager::new();
        tm.set_mode(ThemeMode::Dark);
        assert_eq!(tm.current_theme().mode, ThemeMode::Dark);
        tm.set_mode(ThemeMode::Sepia);
        assert_eq!(tm.current_theme().mode, ThemeMode::Sepia);
    }

    #[test]
    fn test_design_tokens_merge() {
        let base = DesignTokens::light();
        let override_tokens = DesignTokens {
            bg_primary: String::new(),
            bg_secondary: String::new(),
            text_primary: "#333333".into(),
            text_secondary: String::new(),
            accent: String::new(),
            border: String::new(),
            font_size_base: String::new(),
        };
        let merged = base.merge(&override_tokens);
        assert_eq!(merged.text_primary, "#333333");
        // 未覆盖的保持 base
        assert_eq!(merged.bg_primary, base.bg_primary);
    }

    #[test]
    fn test_css_variables_output() {
        let tokens = DesignTokens::dark();
        let css = tokens.to_css_variables();
        assert!(css.starts_with(":root {"));
        assert!(css.contains("--bg-primary: #1a1a1a;"));
        assert!(css.contains("--accent: #4d9fff;"));
    }

    #[test]
    fn test_theme_override_applied() {
        let tm = ThemeManager::new();
        tm.set_mode(ThemeMode::Light);
        tm.set_override(DesignTokens {
            bg_primary: "#custom".into(),
            bg_secondary: String::new(),
            text_primary: String::new(),
            text_secondary: String::new(),
            accent: String::new(),
            border: String::new(),
            font_size_base: String::new(),
        });
        let theme = tm.current_theme();
        assert_eq!(theme.tokens.bg_primary, "#custom");
        assert_eq!(
            theme.tokens.text_primary,
            DesignTokens::light().text_primary
        );
    }

    // --- Shortcuts ---

    #[test]
    fn test_platform_modifier() {
        assert_eq!(Platform::Mac.primary_modifier(), "Cmd");
        assert_eq!(Platform::Windows.primary_modifier(), "Ctrl");
        assert_eq!(Platform::Linux.primary_modifier(), "Ctrl");
    }

    #[test]
    fn test_shortcut_binding_for_platform_mac() {
        let b = ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]);
        let mac = b.for_platform(Platform::Mac);
        assert_eq!(mac.modifiers, vec!["Cmd".to_string()]);
        let win = b.for_platform(Platform::Windows);
        assert_eq!(win.modifiers, vec!["Ctrl".to_string()]);
    }

    #[test]
    fn test_shortcut_signature_normalized() {
        let b1 = ShortcutBinding::new(
            ShortcutScope::Editor,
            "K",
            vec!["Ctrl".into(), "Shift".into()],
        );
        let b2 = ShortcutBinding::new(
            ShortcutScope::Editor,
            "k",
            vec!["Shift".into(), "Ctrl".into()],
        );
        // 顺序不同、大小写不同 → 签名相同
        assert_eq!(b1.signature(), b2.signature());
    }

    #[test]
    fn test_shortcut_conflict_detection() {
        let sm = ShortcutManager::new();
        let b1 = ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]);
        let b2 = ShortcutBinding::new(ShortcutScope::Global, "k", vec!["Ctrl".into()]);
        sm.register(Shortcut {
            id: "s1".into(),
            name: "open".into(),
            description: "".into(),
            binding: b1,
            platform: Platform::Windows,
        });
        sm.register(Shortcut {
            id: "s2".into(),
            name: "search".into(),
            description: "".into(),
            binding: b2,
            platform: Platform::Windows,
        });
        let conflicts = sm.detect_conflicts();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].shortcuts.len(), 2);
    }

    #[test]
    fn test_shortcut_no_conflict_across_platforms() {
        let sm = ShortcutManager::new();
        let b = ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]);
        sm.register(Shortcut {
            id: "s1".into(),
            name: "open".into(),
            description: "".into(),
            binding: b.clone(),
            platform: Platform::Mac,
        });
        sm.register(Shortcut {
            id: "s2".into(),
            name: "open".into(),
            description: "".into(),
            binding: b,
            platform: Platform::Windows,
        });
        // 不同平台不冲突
        assert_eq!(sm.detect_conflicts().len(), 0);
    }

    #[test]
    fn test_shortcut_no_conflict_across_scopes() {
        let sm = ShortcutManager::new();
        let b = ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]);
        sm.register(Shortcut {
            id: "s1".into(),
            name: "global".into(),
            description: "".into(),
            binding: b.clone(),
            platform: Platform::Windows,
        });
        let b2 = ShortcutBinding::new(ShortcutScope::Editor, "K", vec!["Ctrl".into()]);
        sm.register(Shortcut {
            id: "s2".into(),
            name: "editor".into(),
            description: "".into(),
            binding: b2,
            platform: Platform::Windows,
        });
        assert_eq!(sm.detect_conflicts().len(), 0);
    }

    #[test]
    fn test_shortcut_update_binding() {
        let sm = ShortcutManager::new();
        sm.register(Shortcut {
            id: "s1".into(),
            name: "open".into(),
            description: "".into(),
            binding: ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]),
            platform: Platform::Windows,
        });
        let ok = sm.update_binding(
            "s1",
            ShortcutBinding::new(ShortcutScope::Global, "P", vec!["Ctrl".into()]),
        );
        assert!(ok);
        assert_eq!(sm.find("s1").unwrap().binding.key, "P");
    }

    #[test]
    fn test_shortcut_for_platform_filters() {
        let sm = ShortcutManager::new();
        sm.register(Shortcut {
            id: "s1".into(),
            name: "open".into(),
            description: "".into(),
            binding: ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]),
            platform: Platform::Mac,
        });
        sm.register(Shortcut {
            id: "s2".into(),
            name: "open".into(),
            description: "".into(),
            binding: ShortcutBinding::new(ShortcutScope::Global, "K", vec!["Ctrl".into()]),
            platform: Platform::Windows,
        });
        let mac = sm.for_platform(Platform::Mac);
        assert_eq!(mac.len(), 1);
        assert_eq!(mac[0].binding.modifiers, vec!["Cmd".to_string()]);
    }

    #[test]
    fn test_system_settings_aggregate() {
        let s = SystemSettings::new();
        s.store
            .set(SettingsLayer::User, None, "k", serde_json::json!("v"));
        s.theme.set_mode(ThemeMode::Dark);
        s.shortcuts.register(Shortcut {
            id: "s1".into(),
            name: "test".into(),
            description: "".into(),
            binding: ShortcutBinding::new(ShortcutScope::Global, "T", vec![]),
            platform: Platform::Linux,
        });
        assert_eq!(
            s.store.get_effective(None, "k"),
            Some(serde_json::json!("v"))
        );
        assert_eq!(s.theme.current_theme().mode, ThemeMode::Dark);
        assert_eq!(s.shortcuts.list().len(), 1);
    }
}
