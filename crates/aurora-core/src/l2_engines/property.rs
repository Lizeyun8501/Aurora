//! 属性引擎 (Property Engine)
//!
//! 管理块的元数据属性系统，支持自定义属性与类型系统。
//! 基础类型：Text / Number / Date / Checkbox / Select / MultiSelect / Relation / Formula
//! 类型校验：基于 JSON Schema 的运行时校验
//! 索引策略：热点属性自动建立 SQLite 索引，冷属性按需查询

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use parking_lot::RwLock;
use tracing::info;

/// 属性类型系统
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PropertyType {
    Text,
    Number,
    Date,
    Checkbox,
    Select(Vec<SelectOption>),
    MultiSelect(Vec<SelectOption>),
    Relation { target_collection: String },
    Formula { expression: String },
}

/// Select 选项
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectOption {
    pub value: String,
    pub color: Option<String>,
}

/// 属性定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyDefinition {
    pub id: String,
    pub name: String,
    pub prop_type: PropertyType,
    pub required: bool,
    pub default_value: Option<serde_json::Value>,
    pub indexed: bool,
    pub description: Option<String>,
}

/// 属性值
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyValue {
    pub definition_id: String,
    pub value: serde_json::Value,
}

/// 属性集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertySet {
    pub block_id: String,
    pub values: HashMap<String, PropertyValue>,
}

/// 校验结果
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<String>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self { valid: true, errors: vec![] }
    }

    pub fn invalid(msg: impl Into<String>) -> Self {
        Self { valid: false, errors: vec![msg.into()] }
    }

    pub fn add_error(&mut self, msg: impl Into<String>) {
        self.valid = false;
        self.errors.push(msg.into());
    }
}

/// 属性引擎
pub struct PropertyEngine {
    definitions: Arc<RwLock<HashMap<String, PropertyDefinition>>>,
    hot_properties: Arc<RwLock<HashSet<String>>>,
    access_counts: Arc<RwLock<HashMap<String, u64>>>,
}

impl PropertyEngine {
    pub fn new() -> Self {
        Self {
            definitions: Arc::new(RwLock::new(HashMap::new())),
            hot_properties: Arc::new(RwLock::new(HashSet::new())),
            access_counts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new property definition
    pub fn register(&self, def: PropertyDefinition) {
        let id = def.id.clone();
        if def.indexed {
            self.hot_properties.write().insert(id.clone());
        }
        self.definitions.write().insert(id, def);
    }

    /// Unregister a property
    pub fn unregister(&self, def_id: &str) {
        self.definitions.write().remove(def_id);
        self.hot_properties.write().remove(def_id);
        self.access_counts.write().remove(def_id);
    }

    /// Get a property definition
    pub fn get(&self, def_id: &str) -> Option<PropertyDefinition> {
        self.definitions.read().get(def_id).cloned()
    }

    /// List all property definitions
    pub fn list(&self) -> Vec<PropertyDefinition> {
        self.definitions.read().values().cloned().collect()
    }

    /// Validate a property value against its definition
    pub fn validate(&self, def_id: &str, value: &serde_json::Value) -> ValidationResult {
        let defs = self.definitions.read();
        let def = match defs.get(def_id) {
            Some(d) => d,
            None => return ValidationResult::invalid(format!("Property '{}' not found", def_id)),
        };

        // Check required
        if def.required && value.is_null() {
            return ValidationResult::invalid(format!("Property '{}' is required", def.name));
        }

        // Type-specific validation
        self.validate_type(&def.prop_type, value, &def.name)
    }

    fn validate_type(&self, prop_type: &PropertyType, value: &serde_json::Value, name: &str) -> ValidationResult {
        let mut result = ValidationResult::valid();

        match prop_type {
            PropertyType::Text => {
                if !value.is_null() && !value.is_string() {
                    result.add_error(format!("Property '{}' expects string, got {:?}", name, value));
                }
            }
            PropertyType::Number => {
                if !value.is_null() && !value.is_number() {
                    result.add_error(format!("Property '{}' expects number, got {:?}", name, value));
                }
            }
            PropertyType::Date => {
                if !value.is_null() {
                    match value {
                        serde_json::Value::String(s) => {
                            if chrono::DateTime::parse_from_rfc3339(s).is_err()
                                && chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_err() {
                                result.add_error(format!("Property '{}' invalid date format", name));
                            }
                        }
                        serde_json::Value::Number(n) => {
                            if n.as_i64().is_none() {
                                result.add_error(format!("Property '{}' expects integer timestamp", name));
                            }
                        }
                        _ => result.add_error(format!("Property '{}' expects date string or timestamp", name)),
                    }
                }
            }
            PropertyType::Checkbox => {
                if !value.is_null() && !value.is_boolean() {
                    result.add_error(format!("Property '{}' expects boolean", name));
                }
            }
            PropertyType::Select(options) => {
                if !value.is_null() {
                    match value.as_str() {
                        Some(val) => {
                            if !options.iter().any(|opt| opt.value == val) {
                                result.add_error(format!("Property '{}' invalid option '{}'", name, val));
                            }
                        }
                        None => result.add_error(format!("Property '{}' expects string", name)),
                    }
                }
            }
            PropertyType::MultiSelect(options) => {
                if !value.is_null() {
                    match value.as_array() {
                        Some(arr) => {
                            for item in arr {
                                if let Some(s) = item.as_str() {
                                    if !options.iter().any(|opt| opt.value == s) {
                                        result.add_error(format!("Property '{}' invalid option '{}'", name, s));
                                    }
                                } else {
                                    result.add_error(format!("Property '{}' expects array of strings", name));
                                }
                            }
                        }
                        None => result.add_error(format!("Property '{}' expects array", name)),
                    }
                }
            }
            PropertyType::Relation { target_collection } => {
                if !value.is_null() {
                    if !value.is_string() && !value.is_array() {
                        result.add_error(format!("Property '{}' expects string or array of IDs", name));
                    }
                    // Could validate that referenced items exist in target_collection
                    let _ = target_collection;
                }
            }
            PropertyType::Formula { expression } => {
                // Formula values are computed, not validated directly
                // In a full implementation, we'd evaluate the expression
                if !value.is_null() {
                    info!("Formula property '{}' value: {:?}", name, value);
                }
                let _ = expression;
            }
        }

        result
    }

    /// Set a property value on a block
    pub fn set_property(&self, prop_set: &mut PropertySet, def_id: &str, value: serde_json::Value) -> ValidationResult {
        let validation = self.validate(def_id, &value);
        if validation.valid {
            prop_set.values.insert(def_id.to_string(), PropertyValue {
                definition_id: def_id.to_string(),
                value,
            });

            // Track access for hot/cold classification
            *self.access_counts.write().entry(def_id.to_string()).or_insert(0) += 1;
            self.update_hot_cold(def_id);
        }
        validation
    }

    /// Get a property value from a block
    pub fn get_property<'a>(&self, prop_set: &'a PropertySet, def_id: &str) -> Option<&'a serde_json::Value> {
        *self.access_counts.write().entry(def_id.to_string()).or_insert(0) += 1;
        self.update_hot_cold(def_id);
        prop_set.values.get(def_id).map(|pv| &pv.value)
    }

    /// Remove a property from a block
    pub fn remove_property(&self, prop_set: &mut PropertySet, def_id: &str) {
        prop_set.values.remove(def_id);
    }

    /// Get hot properties (frequently accessed)
    pub fn get_hot_properties(&self) -> Vec<String> {
        self.hot_properties.read().iter().cloned().collect()
    }

    /// Update hot/cold classification based on access frequency
    fn update_hot_cold(&self, def_id: &str) {
        let counts = self.access_counts.read();
        let count = counts.get(def_id).copied().unwrap_or(0);
        drop(counts);

        let threshold = 10; // Properties accessed >10 times become hot
        if count > threshold {
            self.hot_properties.write().insert(def_id.to_string());
        }
    }

    /// Check if a property should be indexed (hot = should index)
    pub fn should_index(&self, def_id: &str) -> bool {
        self.hot_properties.read().contains(def_id)
    }
}

impl Default for PropertyEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_text_def(id: &str, required: bool) -> PropertyDefinition {
        PropertyDefinition {
            id: id.to_string(),
            name: format!("Property {}", id),
            prop_type: PropertyType::Text,
            required,
            default_value: None,
            indexed: false,
            description: None,
        }
    }

    fn make_select_def(id: &str) -> PropertyDefinition {
        PropertyDefinition {
            id: id.to_string(),
            name: "Status".to_string(),
            prop_type: PropertyType::Select(vec![
                SelectOption { value: "todo".to_string(), color: Some("red".to_string()) },
                SelectOption { value: "doing".to_string(), color: Some("yellow".to_string()) },
                SelectOption { value: "done".to_string(), color: Some("green".to_string()) },
            ]),
            required: true,
            default_value: Some(serde_json::json!("todo")),
            indexed: true,
            description: None,
        }
    }

    #[test]
    fn test_validate_text_ok() {
        let engine = PropertyEngine::new();
        engine.register(make_text_def("p1", false));
        let result = engine.validate("p1", &serde_json::json!("Hello"));
        assert!(result.valid);
    }

    #[test]
    fn test_validate_text_fail() {
        let engine = PropertyEngine::new();
        engine.register(make_text_def("p1", false));
        let result = engine.validate("p1", &serde_json::json!(42));
        assert!(!result.valid);
    }

    #[test]
    fn test_required_property() {
        let engine = PropertyEngine::new();
        engine.register(make_text_def("p1", true));
        let result = engine.validate("p1", &serde_json::Value::Null);
        assert!(!result.valid);
    }

    #[test]
    fn test_select_valid_option() {
        let engine = PropertyEngine::new();
        engine.register(make_select_def("status"));
        let result = engine.validate("status", &serde_json::json!("todo"));
        assert!(result.valid);
    }

    #[test]
    fn test_select_invalid_option() {
        let engine = PropertyEngine::new();
        engine.register(make_select_def("status"));
        let result = engine.validate("status", &serde_json::json!("invalid"));
        assert!(!result.valid);
    }

    #[test]
    fn test_set_and_get_property() {
        let engine = PropertyEngine::new();
        engine.register(make_text_def("p1", false));

        let mut prop_set = PropertySet {
            block_id: "block-1".to_string(),
            values: HashMap::new(),
        };

        let result = engine.set_property(&mut prop_set, "p1", serde_json::json!("test value"));
        assert!(result.valid);

        let value = engine.get_property(&prop_set, "p1");
        assert_eq!(value, Some(&serde_json::json!("test value")));
    }

    #[test]
    fn test_set_invalid_property_rejected() {
        let engine = PropertyEngine::new();
        engine.register(make_text_def("p1", false));

        let mut prop_set = PropertySet {
            block_id: "block-1".to_string(),
            values: HashMap::new(),
        };

        let result = engine.set_property(&mut prop_set, "p1", serde_json::json!(123));
        assert!(!result.valid);
        assert!(prop_set.values.is_empty());
    }

    #[test]
    fn test_hot_property_tracking() {
        let engine = PropertyEngine::new();
        engine.register(make_text_def("p1", false));

        let prop_set = PropertySet {
            block_id: "block-1".to_string(),
            values: HashMap::new(),
        };

        // Access >10 times to make it hot
        for _ in 0..15 {
            engine.get_property(&prop_set, "p1");
        }

        assert!(engine.should_index("p1"));
    }

    #[test]
    fn test_indexed_on_registration() {
        let engine = PropertyEngine::new();
        engine.register(make_select_def("status"));
        assert!(engine.should_index("status"));
    }

    #[test]
    fn test_multi_select_validation() {
        let engine = PropertyEngine::new();
        engine.register(PropertyDefinition {
            id: "tags".to_string(),
            name: "Tags".to_string(),
            prop_type: PropertyType::MultiSelect(vec![
                SelectOption { value: "rust".to_string(), color: None },
                SelectOption { value: "ai".to_string(), color: None },
            ]),
            required: false,
            default_value: None,
            indexed: false,
            description: None,
        });

        assert!(engine.validate("tags", &serde_json::json!(["rust", "ai"])).valid);
        assert!(!engine.validate("tags", &serde_json::json!(["rust", "invalid"])).valid);
    }
}
