use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn invoke_js(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[derive(Serialize)]
struct NoArgs {}

/// Call a Tauri command with an args struct, decoding the JSON result as `T`.
pub async fn invoke<T: DeserializeOwned>(cmd: &str, args: impl Serialize) -> Result<T, String> {
    let args = serde_wasm_bindgen::to_value(&args).map_err(|e| e.to_string())?;
    let result = invoke_js(cmd, args).await.map_err(|e| format!("{e:?}"))?;
    serde_wasm_bindgen::from_value(result).map_err(|e| e.to_string())
}

/// Like [`invoke`], for commands that take no arguments.
pub async fn invoke0<T: DeserializeOwned>(cmd: &str) -> Result<T, String> {
    invoke(cmd, NoArgs {}).await
}
