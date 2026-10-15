//! DK-14c.1 分支响应型模板：plugin-hello-guest（合约 0.3.0）。
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

    use aurora::plugin::capability as cap;

    struct PluginHello;

    /// A-5：init 传入的 self-id（handle-event 前缀派生源——wasm 单线程，
    /// OnceLock 安全）
    static SELF_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

    impl exports::aurora::plugin::host::Guest for PluginHello {
        fn init(
            capability: &exports::aurora::plugin::host::CapabilityToken,
            plugin_id: String,
        ) -> Result<(), ()> {
            eprintln!("[hello] init ok plugin_id={plugin_id}");
            let _ = SELF_ID.set(plugin_id.clone());
            // DK-14c.2：首个 guest-invoked 能力消费（结构化日志）——
            // 宿主转发 tracing::info!（带 plugin_id）；stderr 侧信道保留
            // 对照；err 传播（越权→trap→invoke 层可断言）
            aurora::plugin::capability::log(capability, "info", "hello init via capability.log");
            // DK-14c.3：数据面自验（write→read 回读→query 键检索）——
            // 共享 KV 语义（同 runtime 全插件可见）见文档+README 披露
            // DK-14c.4：原生类型（option/result/list）——哨兵协议撤
            if let Err(e) = cap::write_doc(
                capability,
                &format!("{plugin_id}:greeting"),
                r#"{"from":"hello"}"#,
            ) {
                panic!("write-doc failed: {e}");
            }
            let back = cap::read_doc(capability, &format!("{plugin_id}:greeting"));
            if back.is_none() {
                panic!("read-doc read-back none");
            }
            // DK-14c.4 回归自验：空内容文档（option 语义——空串可表示）
            // + 含换行 doc_id（list 类型保证——无分隔协议歧义）
            if let Err(e) = cap::write_doc(capability, &format!("{plugin_id}:note-empty"), "") {
                panic!("write-doc empty failed: {e}");
            }
            let empty_back = cap::read_doc(capability, &format!("{plugin_id}:note-empty"));
            match empty_back {
                Some(v) if v.is_empty() => {}
                other => panic!("read-doc empty expected Some(\"\"), got {other:?}"),
            }
            if let Err(e) = cap::write_doc(capability, &format!("{plugin_id}:line\nbreak"), "nl") {
                panic!("write-doc newline failed: {e}");
            }
            let nl_keys = cap::query_docs(capability, "line\n");
            if !nl_keys.iter().any(|k| k.contains('\n')) {
                panic!("query newline key missing: {nl_keys:?}");
            }
            let keys = cap::query_docs(capability, "greeting");
            let key_count = keys.len();
            aurora::plugin::capability::log(
                capability,
                "info",
                &format!("hello data ok keys={key_count}"),
            );
            Ok(())
        }

        fn handle_event(
            capability: &exports::aurora::plugin::host::CapabilityToken,
            event: String,
            payload: String,
        ) -> Result<(), ()> {
            let plugin_id = SELF_ID.get().map(|s| s.as_str()).unwrap_or("unknown");
            eprintln!("{}", super::logic::event_line(&event, &payload));
            // DK-17 L2：read-back 探针——读宿主编排给的任意 doc_id（share
            // 豁免/namespace 拒的 guest 面断言载体；跨前缀未授权→trap）
            if event == "read-back" {
                match cap::read_doc(capability, &payload) {
                    Some(v) => {
                        if let Err(e) =
                            cap::print(capability, &format!("read-back {payload} = {v}"))
                        {
                            panic!("print failed: {e}");
                        }
                        return Ok(());
                    }
                    None => panic!("read-back {payload} = <none>"),
                }
            }
            // DK-17 L2：write-back 探针——payload="doc_id|value"，写指定键
            // （跨前缀写→guard 拒 trap；share 只读语义的 guest 面断言载体）
            if let Some((target, value)) = payload.split_once('|') {
                if event == "write-back" {
                    cap::write_doc(capability, target, value)
                        .unwrap_or_else(|e| panic!("write-back {target} failed: {e}"));
                    return Ok(());
                }
            }
            // DK-14c.3：事件数据落 KV（e2e 容量例经此注入 >1MiB payload）
            if let Err(e) =
                cap::write_doc(capability, &format!("{plugin_id}:event-{event}"), &payload)
            {
                panic!("write-doc failed: {e}");
            }
            // DK-15b：用户可见输出（受控通道演示——与 log 诊断流分界）
            if let Err(e) = cap::print(capability, &format!("hello event={event}")) {
                panic!("print failed: {e}");
            }
            // DK-16 L1：query 键集（本插件前缀过滤后）落 KV——e2e 隔离断言载体
            let keys = cap::query_docs(capability, "");
            let list = keys.join(",");
            let _ = cap::write_doc(capability, &format!("{plugin_id}:query-keys"), &list);
            aurora::plugin::capability::log(capability, "info", &format!("hello event={event}"));
            Ok(())
        }
    }

    export!(PluginHello);
}
