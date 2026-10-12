//! DK-14d.3 示例 guest：plugin-hello-guest（wasm32-wasip2 cdylib）。
//!
//! 绑定自 `../aurora-plugin/wit/aurora-plugin.wit`（单一事实源）生成——
//! world `plugin` export host 接口。最小件：init/handle-event 恒返回
//! result::ok，init 打 log 到 stderr 验证调用链（14e 起再上业务功能）。
//!
//! 构建目标：wasm32-wasip2（CI wasm-guest job；产物不入库）。

wit_bindgen::generate!({
    path: "../aurora-plugin/wit",
    world: "plugin",
});

/// guest 实现体（world 单例；wit-bindgen 0.57 Guest trait 方法无 self）。
struct PluginHello;

impl exports::aurora::plugin::host::Guest for PluginHello {
    fn init(capability: String) -> Result<(), ()> {
        eprintln!("[plugin-hello-guest] init called, capability={capability:?}");
        Ok(())
    }

    fn handle_event(capability: String, event: String, payload: String) -> Result<(), ()> {
        eprintln!(
            "[plugin-hello-guest] handle-event: event={event:?} payload_len={} capability={capability:?}",
            payload.len()
        );
        Ok(())
    }
}

export!(PluginHello);
