//! G1-S2：Collection 命令面 — 数据库视图 CRUD + 行绑定 + 属性单元格。
//!
//! 数据域：core `CollectionStore`（S1——复用 PropertyEngine 校验）；
//! 表格渲染数据面 `cmd_collection_rows` 返回行集+属性值全量（v1 无
//! 分页——集合行数=笔记级，量级温和）；schema 编辑=全量覆盖语义
//! （对齐 cmd_set_note_tags 惯例）。

use crate::get_core;
use aurora_core::l2_engines::collection::Collection;
use aurora_core::l2_engines::property::PropertyDefinition;
use serde_json::{json, Value};

/// 新建集合（空 schema——列头后续编辑）。
#[tauri::command]
pub async fn cmd_collection_create(title: String) -> Result<Value, String> {
    let core = get_core()?;
    let col = core
        .collections
        .create(&title)
        .await
        .map_err(|e| e.to_string())?;
    Ok(collection_json(&col))
}

/// 全量列表（index 驱动）。
#[tauri::command]
pub async fn cmd_collection_list() -> Result<Vec<Value>, String> {
    let core = get_core()?;
    let cols = core.collections.list().await.map_err(|e| e.to_string())?;
    Ok(cols.iter().map(collection_json).collect())
}

/// 读单个（schema 编辑面）。
#[tauri::command]
pub async fn cmd_collection_get(id: String) -> Result<Option<Value>, String> {
    let core = get_core()?;
    Ok(core
        .collections
        .get(&id)
        .await
        .map_err(|e| e.to_string())?
        .map(|c| collection_json(&c)))
}

/// 删除集合（级联行绑定键；行属性值留档不删——笔记域数据）。
#[tauri::command]
pub async fn cmd_collection_delete(id: String) -> Result<(), String> {
    let core = get_core()?;
    core.collections
        .delete(&id)
        .await
        .map_err(|e| e.to_string())
}

/// schema 全量覆盖（列头编辑——PropertyDefinition 数组直传）。
#[tauri::command]
pub async fn cmd_collection_set_schema(
    id: String,
    prop_defs: Vec<PropertyDefinition>,
) -> Result<(), String> {
    let core = get_core()?;
    core.collections
        .set_schema(&id, prop_defs)
        .await
        .map_err(|e| e.to_string())
}

/// 绑行（笔记进集合——v1 行=笔记）。
#[tauri::command]
pub async fn cmd_collection_add_row(id: String, note_id: String) -> Result<(), String> {
    let core = get_core()?;
    core.collections
        .add_row(&id, &note_id)
        .await
        .map_err(|e| e.to_string())
}

/// 解绑行。
#[tauri::command]
pub async fn cmd_collection_remove_row(id: String, note_id: String) -> Result<(), String> {
    let core = get_core()?;
    core.collections
        .remove_row(&id, &note_id)
        .await
        .map_err(|e| e.to_string())
}

/// 单元格写（经 PropertyEngine.validate 校验——非法值拒绝）。
#[tauri::command]
pub async fn cmd_collection_set_cell(
    note_id: String,
    def_id: String,
    value: Value,
) -> Result<(), String> {
    let core = get_core()?;
    core.collections
        .set_cell(&note_id, &def_id, value)
        .await
        .map_err(|e| e.to_string())
}

/// 表格渲染数据面：行集+每行属性值+笔记标题（恒明文通路补全）。
#[tauri::command]
pub async fn cmd_collection_rows(id: String) -> Result<Vec<Value>, String> {
    let core = get_core()?;
    let rows = core
        .collections
        .rows(&id)
        .await
        .map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(rows.len());
    for note_id in rows {
        let title = match core.kv_store.get(&format!("note:{}", note_id)).await {
            // &get_vault()? 内联形式的推断顺序坑：& 反推 ? 的 Ok 类型为
            // LocalDekVault 致 Arc→LocalDekVault From 失败（E0308）——显式
            // 解 Arc（&*）绕开；todo/tag 的先绑定写法不受此坑影响。
            Ok(Some(bytes)) => crate::unwrap_note_bytes(&core, &*get_vault()?, &bytes)
                .ok()
                .and_then(|j| j.get("title").and_then(|t| t.as_str()).map(String::from))
                .unwrap_or_else(|| "未命名".into()),
            _ => "未命名".into(),
        };
        // rows() 现签名=Vec<note_id>（G1 后 core 收敛）——行属性经 get_props 单独取。
        let values = match core.collections.get_props(&note_id).await {
            Ok(set) => json!(set.values),
            Err(_) => json!({}), // 行属性读失败=空表渲染（诚实降级，不阻断行集）
        };
        out.push(json!({
            "note_id": note_id,
            "title": title,
            "values": values,
        }));
    }
    Ok(out)
}

use crate::get_vault;

fn collection_json(col: &Collection) -> Value {
    json!({
        "id": col.id,
        "title": col.title,
        "prop_defs": col.prop_defs,
        "created_at": col.created_at,
    })
}
