//! C-12: 命令面板最近使用域（palette recents）——双端单源 KV。
//!
//! 工单（TA 2026-11-20 07:19）:桌面 localStorage 自持 `aurora.recentCommands`
//! 与移动独立不同步——上移 core KV 单键后双端同源（后续置顶排序两端口径
//! 统一的地基）。
//!
//! 存储模式同 `task_store`（ADR-003 域例外:无 seal/attachments 依赖,
//! `&dyn KVStore` 直注;KV 直存 JSON 单键 `palette:recent_commands`）。
//!
//! 语义（工单契约）:
//! - 值 = `[{id, ts, count?}]` serde 蛇形（`id`/`ts`/`count` 均单词——
//!   序列化即契约,TS 镜像 `shared/types/src/palette.ts` include_str 互证）
//! - 上限 50 条 LRU;同 id 重复 track → 计数合并（count+1）+ ts 刷新 +
//!   移至队首（去重不膨胀）
//! - 坏 JSON 自愈:读解析失败 → 空表（UI 偏好域非权威数据,损坏重置;
//!   下次 track 覆写即恢复——与 task_store fail-closed 不同口径,域性质
//!   使然:偏好丢失可自愈,任务数据丢失不可）
//!
//! 写路径纪律:本域为 UI 偏好镜像,无事件发布、无 blocks/链接派生
//! （WritePath apply_note_change 链不适用——非笔记内容变更;总闸 3 条
//! 面向笔记写链路,本域 KV 偏好直写即全语义）。

use serde::{Deserialize, Serialize};

use crate::traits::kv_store::KVStore;
use crate::Error;

/// KV 单键（工单字面）。
pub const RECENT_COMMANDS_KEY: &str = "palette:recent_commands";

/// LRU 上限（工单字面:50 条）。
pub const MAX_RECENT_COMMANDS: usize = 50;

/// 最近使用条目（serde 蛇形——TS 镜像 `shared/types/src/palette.ts`）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentCommand {
    /// 命令/笔记面板项 id（如 `note.open` / `recent:<id>` / 设置项 id）。
    pub id: String,
    /// 最近一次执行时刻（epoch 毫秒——JS `Date.now()` 对齐）。
    pub ts: i64,
    /// 累计执行次数（同 id track 合并;首记 =1）。
    pub count: u32,
}

/// 读取最近使用列表（空键/坏 JSON → 空表自愈）。
pub async fn get_recent_commands(kv: &dyn KVStore) -> Result<Vec<RecentCommand>, Error> {
    let raw = kv.get(RECENT_COMMANDS_KEY).await?;
    let Some(bytes) = raw else {
        return Ok(Vec::new());
    };
    match serde_json::from_slice::<Vec<RecentCommand>>(&bytes) {
        Ok(list) => Ok(list),
        Err(e) => {
            // 偏好域自愈:损坏重置为空（不向调用方报错——下次 track 覆写
            // 即恢复;tracing 留痕供诊断）。
            tracing::warn!(key = RECENT_COMMANDS_KEY, error = %e, "recent_commands 坏 JSON 自愈为空表");
            Ok(Vec::new())
        }
    }
}

/// 记一次命令命中（LRU 合并同 id 计数+刷新 ts+移队首;超限截尾）。
///
/// 返回合并后的全量列表（调用方可选消费——桌面 v1 忽略返回值走 get 拉取）。
pub async fn track_recent_command(
    kv: &dyn KVStore,
    id: &str,
    ts: i64,
) -> Result<Vec<RecentCommand>, Error> {
    let mut list = get_recent_commands(kv).await?;
    // 同 id 合并:移除旧位（保留 count 累加）,队首插新。
    let prev_count = list
        .iter()
        .find(|r| r.id == id)
        .map(|r| r.count)
        .unwrap_or(0);
    list.retain(|r| r.id != id);
    list.insert(
        0,
        RecentCommand {
            id: id.to_string(),
            ts,
            count: prev_count + 1,
        },
    );
    // LRU 截尾（上限 50——工单字面）。
    list.truncate(MAX_RECENT_COMMANDS);
    // 序列化理论不可达（Vec<RecentCommand> 恒可序列化）——防御分支
    // 走 A04（数据校验类;Domain.message 静态串惯例,细节入 stderr 诊断）
    let bytes = serde_json::to_vec(&list).map_err(|_| Error::Domain {
        code: crate::error_codes::ErrorCode::A04,
        message: "最近使用记录序列化失败",
    })?;
    kv.set(RECENT_COMMANDS_KEY, &bytes).await?;
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l1_infrastructure::storage_engine::MemoryKVStore;
    use std::sync::Arc;

    #[tokio::test]
    async fn track_and_get_roundtrip() {
        let kv = Arc::new(MemoryKVStore::default());
        // 空键 → 空表
        assert!(get_recent_commands(kv.as_ref()).await.unwrap().is_empty());
        // 首 track:count=1
        let out = track_recent_command(kv.as_ref(), "note.open", 100)
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "note.open");
        assert_eq!(out[0].count, 1);
        // 读回一致（KV 持久面）
        let got = get_recent_commands(kv.as_ref()).await.unwrap();
        assert_eq!(got, out);
    }

    #[tokio::test]
    async fn same_id_merges_count_and_refreshes_ts() {
        let kv = Arc::new(MemoryKVStore::default());
        track_recent_command(kv.as_ref(), "a", 100).await.unwrap();
        track_recent_command(kv.as_ref(), "b", 200).await.unwrap();
        // 再 track a:count 2+ts 300+移队首（b 后移）
        let out = track_recent_command(kv.as_ref(), "a", 300).await.unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "a");
        assert_eq!(out[0].count, 2);
        assert_eq!(out[0].ts, 300);
        assert_eq!(out[1].id, "b"); // 去重不膨胀（a 不占两位）
    }

    #[tokio::test]
    async fn lru_evicts_beyond_50() {
        let kv = Arc::new(MemoryKVStore::default());
        for i in 0..(MAX_RECENT_COMMANDS + 7) {
            track_recent_command(kv.as_ref(), &format!("cmd-{i}"), i as i64)
                .await
                .unwrap();
        }
        let got = get_recent_commands(kv.as_ref()).await.unwrap();
        assert_eq!(got.len(), MAX_RECENT_COMMANDS);
        // 队首=最新;最早 7 条被淘汰（cmd-0..cmd-6 不在）
        assert_eq!(got[0].id, format!("cmd-{}", MAX_RECENT_COMMANDS + 6));
        assert!(!got.iter().any(|r| r.id == "cmd-6"));
        assert!(got.iter().any(|r| r.id == "cmd-7"));
    }

    #[tokio::test]
    async fn corrupt_json_self_heals_to_empty() {
        let kv = Arc::new(MemoryKVStore::default());
        kv.set(RECENT_COMMANDS_KEY, b"{not-json").await.unwrap();
        let got = get_recent_commands(kv.as_ref()).await.unwrap();
        assert!(got.is_empty());
        // 自愈后 track 可恢复写入
        let out = track_recent_command(kv.as_ref(), "x", 1).await.unwrap();
        assert_eq!(out.len(), 1);
    }

    /// C-12 契约测试:Rust 权威字段 ↔ TS 镜像（`shared/types/src/palette.ts`）
    /// include_str 互证——变体漂移即红（DK-15a capability 同款惯例）。
    #[test]
    fn ts_mirror_contract() {
        const TS: &str = include_str!("../../../shared/types/src/palette.ts");
        for field in ["id", "ts", "count"] {
            assert!(
                TS.contains(&format!("{field}:")),
                "TS RecentCommand 镜像应含 Rust 权威字段 `{field}`（防漂移）"
            );
        }
        // 键名契约（palette:recent_commands——TS 镜像常量同步）
        assert!(
            TS.contains("palette:recent_commands"),
            "TS 镜像应含 KV 键常量（双端单键一致）"
        );
        // 上限契约（50——双端截断口径一致）
        assert!(
            TS.contains("RECENT_COMMANDS_MAX = 50"),
            "TS 镜像应含 LRU 上限常量 50（双端口径统一）"
        );
    }
}
