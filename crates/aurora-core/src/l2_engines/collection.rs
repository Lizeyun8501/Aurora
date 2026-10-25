//! G1-S1：Collection 数据域 — 数据库视图（Notion database 心智）。
//!
//! 复用 [`PropertyEngine`]/[`PropertyDefinition`]（零重造——属性 schema
//! 直接引用，8 种类型含 Relation/Formula）；行=笔记绑定（v1 行锚定
//! note_id —— Notion「行=页」同构）；属性值落 PropertySet（block_id=
//! note_id 复用键空间）。
//!
//! 存储：KVStore `collection:{id}` → Collection、`collection:index` →
//! id 列表、`collection:rows:{id}` → 行绑定集、`props:{note_id}` →
//! PropertySet。内存读模型 + kv 持久化（TaskProjection 同模式）。
//!
//! 视图（表格/看板/筛选）是读端关注点——本域只供数据与行集，视图
//! 状态不落库（v1 前端本地态）。

use crate::l2_engines::property::{PropertyDefinition, PropertyEngine, PropertySet};
use crate::traits::kv_store::KVStore;
use crate::Error;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 集合实体（属性 schema 的容器——行集另存）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collection {
    /// `col:{uuid}`
    pub id: String,
    pub title: String,
    /// 集合属性 schema（全量覆盖编辑——PropertyDefinition 直引用）
    pub prop_defs: Vec<PropertyDefinition>,
    /// Unix 毫秒
    pub created_at: i64,
}

/// 索引键 / 行绑定键（kv key 规约——含 user 段挂点说明：v1 单段
/// `collection:*`；多用户段接 DK-47 per-user 前缀时迁移）。
pub const INDEX_KEY: &str = "collection:index";
pub fn collection_key(id: &str) -> String {
    format!("collection:{id}")
}
pub fn rows_key(id: &str) -> String {
    format!("collection:rows:{id}")
}
pub fn props_key(note_id: &str) -> String {
    format!("props:{note_id}")
}

/// 集合存储（AppCore 持有——命令面唯一入口）。
pub struct CollectionStore {
    kv: Arc<dyn KVStore>,
    engine: PropertyEngine,
    /// 内存读模型（id → Collection；启动懒加载）
    cache: RwLock<std::collections::HashMap<String, Collection>>,
}

impl CollectionStore {
    pub fn new(kv: Arc<dyn KVStore>) -> Self {
        Self {
            kv,
            engine: PropertyEngine::new(),
            cache: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// 建（uuid 幂等由调用方语义保证——重名允许，id 唯一）。
    pub async fn create(&self, title: &str) -> Result<Collection, Error> {
        let col = Collection {
            id: format!("col:{}", uuid::Uuid::new_v4()),
            title: title.to_string(),
            prop_defs: Vec::new(),
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        self.save(&col).await?;
        Ok(col)
    }

    /// 全量列表（index 驱动——cache 未命中逐条拉）。
    pub async fn list(&self) -> Result<Vec<Collection>, Error> {
        let ids = self.load_index().await?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(c) = self.load_one(&id).await? {
                out.push(c);
            }
        }
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    }

    /// 读单。
    pub async fn get(&self, id: &str) -> Result<Option<Collection>, Error> {
        self.load_one(id).await
    }

    /// schema 全量覆盖编辑（含校验注册——PropertyEngine 同步 register）。
    pub async fn set_schema(
        &self,
        id: &str,
        prop_defs: Vec<PropertyDefinition>,
    ) -> Result<(), Error> {
        let col = self
            .load_one(id)
            .await?
            .ok_or_else(|| Error::InvalidInput(format!("collection not found: {id}")))?;
        let mut col = col;
        for def in &prop_defs {
            self.engine.register(def.clone());
        }
        col.prop_defs = prop_defs;
        self.save(&col).await?;
        Ok(())
    }

    /// 删（级联：index+实体+行绑定键）。
    pub async fn delete(&self, id: &str) -> Result<(), Error> {
        let mut ids = self.load_index().await?;
        if ids.iter().any(|x| x == id) {
            ids.retain(|x| x != id);
            self.save_index(&ids).await?;
        }
        self.kv.delete(&collection_key(id)).await?;
        self.kv.delete(&rows_key(id)).await?;
        self.cache.write().await.remove(id);
        Ok(())
    }

    /// 绑行（行=笔记——幂等）。
    pub async fn add_row(&self, id: &str, note_id: &str) -> Result<(), Error> {
        self.ensure_exists(id).await?;
        let mut rows = self.load_rows(id).await?;
        if !rows.iter().any(|r| r == note_id) {
            rows.push(note_id.to_string());
            self.save_rows(id, &rows).await?;
        }
        Ok(())
    }

    /// 摘行。
    pub async fn remove_row(&self, id: &str, note_id: &str) -> Result<(), Error> {
        let mut rows = self.load_rows(id).await?;
        rows.retain(|r| r != note_id);
        self.save_rows(id, &rows).await?;
        Ok(())
    }

    /// 行集（表格渲染数据面——行序即绑定序）。
    pub async fn rows(&self, id: &str) -> Result<Vec<String>, Error> {
        self.load_rows(id).await
    }

    /// 写单元格（经 PropertyEngine 校验——类型不符拒绝）。
    pub async fn set_cell(
        &self,
        note_id: &str,
        def_id: &str,
        value: serde_json::Value,
    ) -> Result<(), Error> {
        let result = self.engine.validate(def_id, &value);
        if !result.valid {
            return Err(Error::InvalidInput(format!(
                "property validate: {}",
                result.errors.join("; ")
            )));
        }
        let mut set = self.load_props(note_id).await?;
        let result = self.engine.set_property(&mut set, def_id, value);
        if !result.valid {
            return Err(Error::InvalidInput(format!(
                "property set: {}",
                result.errors.join("; ")
            )));
        }
        self.save_props(note_id, &set).await
    }

    /// 读某行全部属性值。
    pub async fn get_props(&self, note_id: &str) -> Result<PropertySet, Error> {
        self.load_props(note_id).await
    }

    // ── 内部：kv 读写 ──

    async fn ensure_exists(&self, id: &str) -> Result<(), Error> {
        if self.load_one(id).await?.is_some() {
            Ok(())
        } else {
            Err(Error::InvalidInput(format!("collection not found: {id}")))
        }
    }

    async fn load_index(&self) -> Result<Vec<String>, Error> {
        match self.kv.get(INDEX_KEY).await? {
            Some(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            None => Ok(Vec::new()),
        }
    }

    async fn save_index(&self, ids: &[String]) -> Result<(), Error> {
        self.kv
            .set(
                INDEX_KEY,
                &serde_json::to_vec(ids).map_err(Error::Serialization)?,
            )
            .await
    }

    async fn load_one(&self, id: &str) -> Result<Option<Collection>, Error> {
        if let Some(c) = self.cache.read().await.get(id) {
            return Ok(Some(c.clone()));
        }
        match self.kv.get(&collection_key(id)).await? {
            Some(bytes) => {
                let col: Collection =
                    serde_json::from_slice(&bytes).map_err(Error::Serialization)?;
                for def in &col.prop_defs {
                    self.engine.register(def.clone());
                }
                self.cache.write().await.insert(id.to_string(), col.clone());
                Ok(Some(col))
            }
            None => Ok(None),
        }
    }

    async fn save(&self, col: &Collection) -> Result<(), Error> {
        let mut ids = self.load_index().await?;
        if !ids.iter().any(|x| x == &col.id) {
            ids.push(col.id.clone());
            self.save_index(&ids).await?;
        }
        self.kv
            .set(
                &collection_key(&col.id),
                &serde_json::to_vec(col).map_err(Error::Serialization)?,
            )
            .await?;
        self.cache.write().await.insert(col.id.clone(), col.clone());
        Ok(())
    }

    async fn load_rows(&self, id: &str) -> Result<Vec<String>, Error> {
        match self.kv.get(&rows_key(id)).await? {
            Some(bytes) => Ok(serde_json::from_slice(&bytes).unwrap_or_default()),
            None => Ok(Vec::new()),
        }
    }

    async fn save_rows(&self, id: &str, rows: &[String]) -> Result<(), Error> {
        self.kv
            .set(
                &rows_key(id),
                &serde_json::to_vec(rows).map_err(Error::Serialization)?,
            )
            .await
    }

    async fn load_props(&self, note_id: &str) -> Result<PropertySet, Error> {
        match self.kv.get(&props_key(note_id)).await? {
            Some(bytes) => Ok(
                serde_json::from_slice(&bytes).unwrap_or_else(|_| PropertySet {
                    block_id: note_id.to_string(),
                    values: Default::default(),
                }),
            ),
            None => Ok(PropertySet {
                block_id: note_id.to_string(),
                values: Default::default(),
            }),
        }
    }

    async fn save_props(&self, note_id: &str, set: &PropertySet) -> Result<(), Error> {
        self.kv
            .set(
                &props_key(note_id),
                &serde_json::to_vec(set).map_err(Error::Serialization)?,
            )
            .await
    }
}
