//! DK-14e.1 示范插件 · 被动监听型模板：plugin-auditlog-guest。
//!
//! 绑定自 `../aurora-plugin/wit/aurora-plugin.wit`（单一事实源）生成。
//!
//! 行为约定（docs/plugin-log-conventions.md）：handle-event 全量记录
//! `[audit] ts=N event=<event> payload_len=<len>`（N=调用计数，从 1 递增）；
//! 零业务逻辑旁路插件——第三方起步的最简模板。
//!
//! 工程结构同 hello 模板：逻辑在 [`logic`]（host 可测），绑定 cfg 隔离；
//! 调用计数用静态原子量（wasm 单线程语义下安全；wasm 实例生命周期内累计）。

pub mod logic {
    use std::sync::atomic::{AtomicU64, Ordering};

    static CALLS: AtomicU64 = AtomicU64::new(0);

    /// 取下一个序号（1 起）。
    pub fn next_ts() -> u64 {
        CALLS.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 审计行（单行、固定前缀——测试可断言）。
    pub fn audit_line(ts: u64, event: &str, payload: &str) -> String {
        format!(
            "[audit] ts={ts} event={event} payload_len={}",
            payload.len()
        )
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn ts_increments() {
            let a = next_ts();
            let b = next_ts();
            assert_eq!(b, a + 1);
        }

        #[test]
        fn audit_line_format() {
            let line = audit_line(3, "note.created", "hello");
            assert_eq!(line, "[audit] ts=3 event=note.created payload_len=5");
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    wit_bindgen::generate!({
        path: "../aurora-plugin/wit",
        world: "plugin",
    });

    struct AuditlogGuest;

    impl exports::aurora::plugin::host::Guest for AuditlogGuest {
        fn init(_capability: &exports::aurora::plugin::host::CapabilityToken) -> Result<(), ()> {
            eprintln!("[audit] init ok");
            Ok(())
        }

        fn handle_event(
            _capability: &exports::aurora::plugin::host::CapabilityToken,
            event: String,
            payload: String,
        ) -> Result<(), ()> {
            let ts = super::logic::next_ts();
            eprintln!("{}", super::logic::audit_line(ts, &event, &payload));
            Ok(())
        }
    }

    export!(AuditlogGuest);
}
