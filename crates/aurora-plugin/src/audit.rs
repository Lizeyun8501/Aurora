//! 结构化插件审计日志（DK-审计面：16/17 审计产生面的可查询化）。
//!
//! 现状勘定（2026-10-16）：wasm_runtime 18 处 `eprintln!("[audit-host] …")`
//! ——stderr 即弃，无结构化存储、不可查询、不可持久化。本模块落：
//!
//! 1. [`AuditEntry`]：单条审计（ts/plugin/event/result/reason/detail）——
//!    serde Serialize（tauri DTO 直投影）。
//! 2. [`AuditRing`]：定容环形缓冲（内存态先行，落盘随同步卡——与 DK-17
//!    shares 图谱"内存 KV 先行"同一节奏）。满则丢最旧（防宿主内存被
//!    恶意插件刷爆——容量即安全边界）。
//! 3. 双写纪律：产生面 **eprintln 保留**（stderr 可观测/CI 抓取）+
//!    ring.push（可查询）。查询经 `WasmRuntime::audit_recent`。
//!
//! 领地：独立文件（installer/fetcher 归属他卡并行，零碰撞）。

use aurora_core::traits::kv_store::KVStore;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

/// 单条审计记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// RFC3339 UTC 时间戳。
    pub ts: String,
    /// 插件 id（宿主面事件如 share/revoke 以 from 为主语，仍填 plugin 维度）。
    pub plugin: String,
    /// 事件名（init / handle-event / share / revoke / useroutput / log /
    /// read-doc / write-doc / storage-deny / data-deny …）。
    pub event: String,
    /// 结果（ok / denied / error）。
    pub result: String,
    /// 拒绝原因或语义标记（no-token / useroutput-denied / share-read …）。
    pub reason: String,
    /// 补充明细（doc_id / 目标插件 / 字节数等，自由文本）。
    pub detail: String,
}

/// 定容环形审计缓冲。
pub struct AuditRing {
    cap: usize,
    inner: Mutex<VecDeque<AuditEntry>>,
    /// 未落盘计数（push 递增/persist 成功清零——省写：无新行不重写快照）。
    dirty: std::sync::atomic::AtomicUsize,
}

impl AuditRing {
    /// 缺省容量（1KiB 条——桌面运维足够，恶意刷写丢最旧可控）。
    pub const DEFAULT_CAP: usize = 1024;

    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            inner: Mutex::new(VecDeque::new()),
            dirty: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 未落盘行数（0=快照与 ring 一致，persist 可跳写）。
    pub fn dirty(&self) -> usize {
        self.dirty.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// persist 成功后清零（audit::persist_audit 语义）。
    pub fn mark_clean(&self) {
        self.dirty.store(0, std::sync::atomic::Ordering::Relaxed);
    }

    /// 追加一条（满则丢最旧）。
    pub fn push(&self, entry: AuditEntry) {
        let mut q = self.inner.lock().expect("audit ring poisoned");
        if q.len() >= self.cap {
            q.pop_front();
        }
        q.push_back(entry);
        self.dirty
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// 按插入序（旧→新）返回最近 `limit` 条；`plugin` 为 Some 时按插件过滤。
    pub fn recent(&self, limit: usize, plugin: Option<&str>) -> Vec<AuditEntry> {
        let q = self.inner.lock().expect("audit ring poisoned");
        q.iter()
            .filter(|e| plugin.is_none_or(|p| e.plugin == p))
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
    }

    /// 快照（旧→新全量，供落盘序列化；cap 内最多 DEFAULT_CAP 条）。
    pub fn snapshot(&self) -> Vec<AuditEntry> {
        self.inner
            .lock()
            .expect("audit ring poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// 回灌历史（启动恢复——不重排，直接 append；总量仍受 cap 丢最旧约束）。
    pub fn extend_history(&self, entries: Vec<AuditEntry>) {
        let mut q = self.inner.lock().expect("audit ring poisoned");
        for e in entries {
            if q.len() >= self.cap {
                q.pop_front();
            }
            q.push_back(e);
        }
    }

    /// 当前条数（诊断用）。
    pub fn len(&self) -> usize {
        self.inner.lock().expect("audit ring poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for AuditRing {
    fn default() -> Self {
        Self::new(Self::DEFAULT_CAP)
    }
}

/// 审计快照 KV 键（单键 bincode——容量天然有界，无增量键膨胀面）。
pub const AUDIT_SNAPSHOT_KEY: &str = "plugins:audit:snapshot";

/// 落盘快照容量上限（条）——512≈ring 一半，重启恢复足够运维回看。
pub const SNAPSHOT_CAP: usize = 512;

/// 审计链落盘：ring 快照 → KV 单键 bincode（调用方决定时机——桌面
/// hourly tick flush；粒度=最多丢一个 flush 周期，stderr 双写仍有留痕）。
/// 返回落盘条数。
///
/// # Errors
/// KV 写失败 / 序列化失败。
pub async fn persist_audit(
    runtime: &crate::wasm_runtime::WasmRuntime,
    kv: &dyn KVStore,
) -> Result<usize, crate::Error> {
    let mut snap = runtime.audit_snapshot();
    if snap.len() > SNAPSHOT_CAP {
        snap.drain(..snap.len() - SNAPSHOT_CAP); // 丢最旧，保留最近 SNAPSHOT_CAP 条
    }
    if runtime.audit_dirty() == 0 {
        return Ok(0); // 无新行：快照与 ring 一致，跳写（省写优化）
    }
    let n = snap.len();
    let bytes = bincode::serialize(&snap)
        .map_err(|e| crate::Error::InvalidInput(format!("audit snapshot serialize: {e}")))?;
    kv.set(AUDIT_SNAPSHOT_KEY, &bytes)
        .await
        .map_err(|e| crate::Error::Kv(e.to_string()))?;
    runtime.audit_mark_clean();
    Ok(n)
}

/// 审计链启动恢复：KV 快照 → ring 回灌（不覆盖新产生行——append 语义，
/// cap 丢最旧兜底）。返回回灌条数（无快照=0）。
///
/// # Errors
/// KV 读失败（快照损坏视为无快照——warn 语义返回 0，不阻断启动）。
pub async fn restore_audit(
    runtime: &crate::wasm_runtime::WasmRuntime,
    kv: &dyn KVStore,
) -> Result<usize, crate::Error> {
    let Some(bytes) = kv
        .get(AUDIT_SNAPSHOT_KEY)
        .await
        .map_err(|e| crate::Error::Kv(e.to_string()))?
    else {
        return Ok(0);
    };
    let Ok(entries) = bincode::deserialize::<Vec<AuditEntry>>(&bytes) else {
        return Ok(0); // 快照损坏：视为无快照（fail-open 但无害——审计是运维面）
    };
    let n = entries.len();
    runtime.audit_restore(entries);
    Ok(n)
}

/// 便捷构造：从字段组一条（ts=now RFC3339）。
pub fn entry(plugin: &str, event: &str, result: &str, reason: &str, detail: &str) -> AuditEntry {
    AuditEntry {
        ts: chrono::Utc::now().to_rfc3339(),
        plugin: plugin.to_string(),
        event: event.to_string(),
        result: result.to_string(),
        reason: reason.to_string(),
        detail: detail.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_capacity_drops_oldest() {
        let ring = AuditRing::new(3);
        for i in 0..5 {
            ring.push(entry(&format!("p{i}"), "init", "ok", "-", "-"));
        }
        assert_eq!(ring.len(), 3);
        let all = ring.recent(10, None);
        // 最旧两条被丢弃
        assert_eq!(all[0].plugin, "p2");
        assert_eq!(all[2].plugin, "p4");
    }

    #[test]
    fn test_recent_limit_and_plugin_filter() {
        let ring = AuditRing::default();
        ring.push(entry("a", "init", "ok", "-", "-"));
        ring.push(entry("b", "share", "ok", "opt-in", "doc1"));
        ring.push(entry("a", "handle-event", "denied", "no-token", ""));
        ring.push(entry("a", "init", "ok", "-", "-"));
        // limit 截取（新→旧取后反转）
        assert_eq!(ring.recent(2, None).len(), 2);
        assert_eq!(ring.recent(2, None)[0].event, "handle-event");
        let only_a = ring.recent(10, Some("a"));
        assert_eq!(only_a.len(), 3);
        assert!(only_a.iter().all(|e| e.plugin == "a"));
        assert!(ring.recent(10, Some("ghost")).is_empty());
    }

    #[test]
    fn test_entry_serializable() {
        let e = entry("p1", "share", "ok", "opt-in", "doc1->p2");
        let json = serde_json::to_string(&e).expect("serialize ok");
        assert!(json.contains("\"event\":\"share\""));
        assert!(json.contains("doc1->p2"));
    }
    #[test]
    fn test_snapshot_roundtrip_and_restore() {
        let ring = AuditRing::new(8);
        ring.push(entry("a", "init", "ok", "-", ""));
        ring.push(entry("b", "share", "ok", "opt-in", "doc1"));
        let snap = ring.snapshot();
        assert_eq!(snap.len(), 2);
        let bytes = bincode::serialize(&snap).unwrap();
        let back: Vec<AuditEntry> = bincode::deserialize(&bytes).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].plugin, "a");
        assert_eq!(back[1].event, "share");
        // 回灌到新 ring（append 语义+cap 兜底——cap=2 时 push c 丢最旧 a）
        let ring2 = AuditRing::new(2);
        ring2.extend_history(back);
        ring2.push(entry("c", "init", "ok", "-", ""));
        assert_eq!(ring2.len(), 2);
        assert_eq!(ring2.snapshot()[0].plugin, "b"); // 丢最旧 a
    }

    #[tokio::test]
    async fn test_persist_restore_kv_roundtrip() {
        use std::collections::HashMap;
        use std::sync::Mutex;

        struct MemKv(Mutex<HashMap<String, Vec<u8>>>);
        #[async_trait::async_trait]
        impl KVStore for MemKv {
            async fn get(&self, k: &str) -> Result<Option<Vec<u8>>, aurora_core::Error> {
                Ok(self.0.lock().unwrap().get(k).cloned())
            }
            async fn set(&self, k: &str, v: &[u8]) -> Result<(), aurora_core::Error> {
                self.0.lock().unwrap().insert(k.to_string(), v.to_vec());
                Ok(())
            }
            async fn delete(&self, k: &str) -> Result<(), aurora_core::Error> {
                self.0.lock().unwrap().remove(k);
                Ok(())
            }
            async fn exists(&self, k: &str) -> Result<bool, aurora_core::Error> {
                Ok(self.0.lock().unwrap().contains_key(k))
            }
            async fn batch_get(
                &self,
                ks: &[&str],
            ) -> Result<Vec<Option<Vec<u8>>>, aurora_core::Error> {
                Ok(ks
                    .iter()
                    .map(|k| self.0.lock().unwrap().get(*k).cloned())
                    .collect())
            }
            async fn batch_set(&self, items: &[(&str, &[u8])]) -> Result<(), aurora_core::Error> {
                for (k, v) in items {
                    self.0.lock().unwrap().insert(k.to_string(), v.to_vec());
                }
                Ok(())
            }
            async fn scan_prefix(
                &self,
                p: &str,
            ) -> Result<Vec<(String, Vec<u8>)>, aurora_core::Error> {
                Ok(self
                    .0
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|(k, _)| k.starts_with(p))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect())
            }
        }
        let rt = crate::wasm_runtime::WasmRuntime::with_default_registry();
        rt.audit_log("a", "init", "ok", "-", "");
        rt.audit_log("b", "handle-event", "denied", "no-token", "");
        let kv = MemKv(Mutex::new(HashMap::new()));
        let n = persist_audit(&rt, &kv).await.unwrap();
        assert_eq!(n, 2);
        // 新 runtime 恢复
        let rt2 = crate::wasm_runtime::WasmRuntime::with_default_registry();
        let m = restore_audit(&rt2, &kv).await.unwrap();
        assert_eq!(m, 2);
        assert_eq!(rt2.audit_recent(10, None).len(), 2);
        // 损坏快照：fail-open 返回 0
        kv.0.lock()
            .unwrap()
            .insert(AUDIT_SNAPSHOT_KEY.to_string(), vec![0xff, 0xfe]);
        let rt3 = crate::wasm_runtime::WasmRuntime::with_default_registry();
        assert_eq!(restore_audit(&rt3, &kv).await.unwrap(), 0);
    }
}
