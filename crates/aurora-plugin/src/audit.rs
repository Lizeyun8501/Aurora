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

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;

/// 单条审计记录。
#[derive(Debug, Clone, Serialize)]
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
}

impl AuditRing {
    /// 缺省容量（1KiB 条——桌面运维足够，恶意刷写丢最旧可控）。
    pub const DEFAULT_CAP: usize = 1024;

    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            inner: Mutex::new(VecDeque::new()),
        }
    }

    /// 追加一条（满则丢最旧）。
    pub fn push(&self, entry: AuditEntry) {
        let mut q = self.inner.lock().expect("audit ring poisoned");
        if q.len() >= self.cap {
            q.pop_front();
        }
        q.push_back(entry);
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
}
