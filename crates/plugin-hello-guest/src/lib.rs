//! DK-14c.1 分支响应型模板：plugin-hello-guest（合约 0.2.0）。
//!
//! 绑定自 `../aurora-plugin/wit/aurora-plugin.wit`（单一事实源）生成——
//! world `plugin` export host 接口（resource capability-token 版）。
//!
//! 行为约定（日志前缀见 docs/plugin-log-conventions.md）：
//! - init：记录启动（capability-token 为宿主发放的不透明句柄——DK-14c.1
//!   资源化，guest 不可解引用；0.1.0 时代的字符串内容校验随之退役）
//! - handle-event：`note.created` 打摘要行；未知 event 打 `[hello] WARN ...`
//!
//! 工程结构：业务逻辑在 [`logic`]（纯 Rust，host/wasm 双面编译，
//! `cargo test -p plugin-hello-guest` 可跑单测）；WIT 绑定与导出仅在
//! wasm32 target 编译（`cfg(target_arch)` 隔离）。

pub mod logic {
    /// handle-event 的分支输出行（单行、固定前缀——测试可断言）。
    /// capability-token 不透明不可打印——摘要行不含能力字段（DK-14c.1）。
    pub fn event_line(event: &str, payload: &str) -> String {
        if event == "note.created" {
            format!(
                "[hello] note.created payload_len={} payload_preview={:?}",
                payload.len(),
                payload_preview(payload)
            )
        } else {
            format!("[hello] WARN unknown event={event:?}")
        }
    }

    /// payload 摘要（前 20 字符——防止大 payload 刷日志）。
    pub fn payload_preview(s: &str) -> String {
        s.chars().take(20).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn event_line_note_created() {
            let line = event_line("note.created", "hello world");
            assert!(line.starts_with("[hello] note.created "));
            assert!(line.contains("payload_len=11"));
            assert!(line.contains("payload_preview=\"hello world\""));
        }

        #[test]
        fn event_line_unknown_warn() {
            let line = event_line("note.deleted", "x");
            assert!(line.starts_with("[hello] WARN unknown event=\"note.deleted\""));
        }

        #[test]
        fn preview_truncates_at_20_chars() {
            let p = payload_preview(&"a".repeat(100));
            assert_eq!(p.chars().count(), 20);
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    wit_bindgen::generate!({
        path: "../aurora-plugin/wit",
        world: "plugin",
    });

    struct PluginHello;

    impl exports::aurora::plugin::host::Guest for PluginHello {
        fn init(_capability: &exports::aurora::plugin::host::CapabilityToken) -> Result<(), ()> {
            eprintln!("[hello] init ok");
            Ok(())
        }

        fn handle_event(
            _capability: &exports::aurora::plugin::host::CapabilityToken,
            event: String,
            payload: String,
        ) -> Result<(), ()> {
            eprintln!("{}", super::logic::event_line(&event, &payload));
            Ok(())
        }
    }

    export!(PluginHello);
}
