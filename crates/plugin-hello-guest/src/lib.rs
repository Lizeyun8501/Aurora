//! DK-14e.1 示范插件 · 分支响应型模板：plugin-hello-guest。
//!
//! 绑定自 `../aurora-plugin/wit/aurora-plugin.wit`（单一事实源）生成——
//! world `plugin` export host 接口。
//!
//! 行为约定（日志前缀见 docs/plugin-log-conventions.md）：
//! - init：校验 capability 非空，空则打 `[hello] ERROR ...` 到 stderr
//! - handle-event：`note.created` 打摘要行；未知 event 打 `[hello] WARN ...`
//!
//! 工程结构（第三方模板关键点）：业务逻辑全部在 [`logic`]（纯 Rust，
//! host/wasm 双面编译，`cargo test -p plugin-hello-guest` 可跑单测）；
//! WIT 绑定与导出仅在 wasm32 target 编译（`cfg(target_arch)` 隔离）。

pub mod logic {
    /// 分支响应型纯逻辑（无 WIT 依赖——宿主侧单测覆盖）。
    pub fn init_check(capability: &str) -> Result<(), String> {
        if capability.is_empty() {
            Err("capability must be non-empty".into())
        } else {
            Ok(())
        }
    }

    /// handle-event 的分支输出行（单行、固定前缀——测试可断言）。
    pub fn event_line(capability: &str, event: &str, payload: &str) -> String {
        if event == "note.created" {
            format!(
                "[hello] note.created capability={capability:?} payload_len={} payload_preview={:?}",
                payload.len(),
                payload_preview(payload)
            )
        } else {
            format!("[hello] WARN unknown event={event:?} capability={capability:?}")
        }
    }

    /// payload 摘要（前 20 字符——避免 stderr 噪音）。
    fn payload_preview(payload: &str) -> String {
        payload.chars().take(20).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn init_rejects_empty_capability() {
            assert!(init_check("").is_err());
            assert!(init_check("notes.read").is_ok());
        }

        #[test]
        fn event_line_note_created_has_summary() {
            let line = event_line("notes.read", "note.created", "hello world");
            assert!(line.starts_with("[hello] note.created"));
            assert!(line.contains("payload_len=11"));
            assert!(line.contains("payload_preview=\"hello world\""));
        }

        #[test]
        fn event_line_unknown_warns() {
            let line = event_line("notes.read", "note.updated", "x");
            assert!(line.starts_with("[hello] WARN unknown"));
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
        fn init(capability: String) -> Result<(), ()> {
            match super::logic::init_check(&capability) {
                Ok(()) => {
                    eprintln!("[hello] init ok capability={capability:?}");
                    Ok(())
                }
                Err(reason) => {
                    eprintln!("[hello] ERROR init rejected: {reason} capability={capability:?}");
                    Ok(())
                }
            }
        }

        fn handle_event(capability: String, event: String, payload: String) -> Result<(), ()> {
            eprintln!("{}", super::logic::event_line(&capability, &event, &payload));
            Ok(())
        }
    }

    export!(PluginHello);
}
