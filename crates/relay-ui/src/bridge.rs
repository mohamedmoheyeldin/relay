//! Thin wrapper over the Tauri JavaScript API (`window.__TAURI__`, enabled with `withGlobalTauri`).

use relay_protocol::UiEvent;
use serde_json::Value;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_namespace = ["__TAURI__", "core"], js_name = invoke)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = ["__TAURI__", "event"], js_name = listen)]
    async fn tauri_listen(
        event: &str,
        handler: &Closure<dyn FnMut(JsValue)>,
    ) -> Result<JsValue, JsValue>;
}

/// Name of the event the backend uses for [`UiEvent`]s (payload is a JSON string).
pub const EVENT_NAME: &str = "relay-event";

fn describe(err: JsValue) -> String {
    err.as_string()
        .or_else(|| {
            js_sys::Reflect::get(&err, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
        })
        .unwrap_or_else(|| "unknown error".to_string())
}

/// Call a backend command. Commands return their result as a JSON string (or nothing).
pub async fn invoke(cmd: &str, args: Value) -> Result<Option<String>, String> {
    let js_args = js_sys::JSON::parse(&args.to_string()).map_err(describe)?;
    let result = tauri_invoke(cmd, js_args).await.map_err(describe)?;
    Ok(result.as_string())
}

/// Subscribe to backend events for the lifetime of the page.
pub async fn listen_events(mut on_event: impl FnMut(UiEvent) + 'static) {
    let callback = Closure::<dyn FnMut(JsValue)>::new(move |event: JsValue| {
        let payload = js_sys::Reflect::get(&event, &"payload".into())
            .ok()
            .and_then(|p| p.as_string());
        if let Some(json) = payload {
            match serde_json::from_str::<UiEvent>(&json) {
                Ok(parsed) => on_event(parsed),
                Err(e) => web_sys::console::error_1(&format!("bad event: {e}").into()),
            }
        }
    });
    if tauri_listen(EVENT_NAME, &callback).await.is_err() {
        web_sys::console::error_1(&"could not subscribe to backend events".into());
    }
    callback.forget();
}
