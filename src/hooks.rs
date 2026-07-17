use leptos::prelude::*;
use leptos::task::spawn_local;
use send_wrapper::SendWrapper;
use serde::de::DeserializeOwned;
use shared::api::events::TauriEvent;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use log::error;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    pub type Channel;

    #[wasm_bindgen(constructor, js_namespace = ["window", "__TAURI__", "core"])]
    fn new() -> Channel;

    #[wasm_bindgen(method, setter)]
    fn set_onmessage(this: &Channel, cb: &Closure<dyn FnMut(JsValue)>);

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    pub fn invoke(cmd: &str, args: JsValue) -> js_sys::Promise;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "core"], js_name = convertFileSrc)]
    pub fn convertFileSrc(path: &str) -> String;

    #[wasm_bindgen(js_namespace = ["__TAURI__", "opener"])]
    pub fn openUrl(url: &str) -> js_sys::Promise;
}

impl Clone for Channel {
    fn clone(&self) -> Self {
        // Clones the JS handle, not the channel itself: both values refer to
        // the same underlying `__TAURI__.core.Channel` object.
        JsValue::clone(self).unchecked_into()
    }
}

#[derive(serde::Serialize)]
struct IpcCallLog {
    cmd: String,
    request_bytes: usize,
    response_bytes: usize,
    ok: bool,
}

fn js_value_byte_len(value: &JsValue) -> usize {
    match js_sys::JSON::stringify(value) {
        Ok(s) => {
            let s: String = s.into();
            s.len()
        }
        Err(_) => 0,
    }
}

pub async fn call_tauri(cmd: &str, args: JsValue) -> Result<JsValue, JsValue> {
    let request_bytes = js_value_byte_len(&args);
    let result = wasm_bindgen_futures::JsFuture::from(invoke(cmd, args)).await;

    // Avoid recursively logging the traffic-logging call itself.
    if cmd != "log_ipc_call" {
        let log = IpcCallLog {
            cmd: cmd.to_string(),
            request_bytes,
            response_bytes: result.as_ref().map(js_value_byte_len).unwrap_or(0),
            ok: result.is_ok(),
        };
        spawn_local(async move {
            if let Ok(args) = serde_wasm_bindgen::to_value(&log) {
                let _ = call_tauri("log_ipc_call", args).await;
            }
        });
    }

    result
}

pub async fn call_tauri_no_args(cmd: &str) -> Result<JsValue, JsValue> {
    call_tauri(cmd, JsValue::NULL).await
}

/// Calls a Tauri command, injecting a `Channel` into the `args` object under
/// `channel_key` (which must match the backend command's parameter name, in
/// snake_case) before invoking.
pub async fn call_tauri_with_channel(
    cmd: &str,
    args: JsValue,
    channel: &Channel,
) -> Result<JsValue, JsValue> {
    // Args serialized via `serde_wasm_bindgen::to_value(&json!(..))` come out as
    // an ES `Map`, and Tauri's IPC layer only keeps its *entries* — a property
    // added with `Reflect::set` would be silently dropped.
    if let Some(map) = args.dyn_ref::<js_sys::Map>() {
        map.set(&JsValue::from_str("channel"), channel.as_ref());
    } else {
        js_sys::Reflect::set(&args, &JsValue::from_str("channel"), channel.as_ref())?;
    }

    call_tauri(cmd, args).await
}

pub fn use_tauri_event_named<T>(event_name: &str) -> ReadSignal<Option<T>>
where
    T: DeserializeOwned + Clone + Send + Sync + 'static + PartialEq,
{
    let (payload_signal, set_payload_signal) = signal(None::<T>);
    let event_name_owned = event_name.to_string();

    Effect::new(move |_| {
        let name = event_name_owned.clone();
        let owner = Owner::current().expect("Leptos owner required");

        let future = SendWrapper::new(async move {
            let closure = Closure::wrap(Box::new(move |js_val: JsValue| {
                let previous = payload_signal.get_untracked();
                let previous_ref = previous.as_ref();

                let payload_js = match js_sys::Reflect::get(&js_val, &JsValue::from_str("payload"))
                {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Failed to read Tauri event payload field: {:?}", e);
                        return;
                    }
                };

                let js_string: String = match js_sys::JSON::stringify(&payload_js) {
                    Ok(s) => s.into(),
                    Err(e) => {
                        error!("Failed to stringify Tauri event payload: {:?}", e);
                        return;
                    }
                };

                match serde_json::from_str::<T>(&js_string) {
                    Ok(payload) => {
                        if Some(&payload) != previous_ref {
                            set_payload_signal.set(Some(payload));
                        }
                    }
                    Err(e) => {
                        error!(
                            "Failed to deserialize Tauri event payload: {}; {:?}",
                            e, payload_js
                        );
                    }
                }
            }) as Box<dyn FnMut(JsValue)>);

            let handler_fn = closure.as_ref().unchecked_ref::<js_sys::Function>();

            let unlisten_js = listen(&name, handler_fn).await;

            let safe_unlisten = SendWrapper::new(unlisten_js);
            let safe_closure = SendWrapper::new(closure);

            owner.with(move || {
                on_cleanup(move || {
                    let unlisten_js = safe_unlisten.take();
                    let _dropped_closure = safe_closure.take();

                    if let Some(unlisten_fn) = unlisten_js.dyn_ref::<js_sys::Function>() {
                        let _ = unlisten_fn.call0(&JsValue::NULL);
                    }
                });
            });
        });

        spawn_local(future);
    });

    payload_signal
}

pub fn use_tauri_event_option<T>() -> ReadSignal<Option<T>>
where
    T: TauriEvent + DeserializeOwned + Clone + Send + Sync + 'static,
{
    use_tauri_event_named::<T>(&T::name())
}

pub fn setup_update_effect<T>(signal: ReadSignal<Option<T>>, closure: impl Fn(T) + 'static)
where
    T: TauriEvent + DeserializeOwned + Clone + Send + Sync + 'static,
{
    Effect::new(move |_| {
        if let Some(payload) = signal.get() {
            closure(payload);
        }
    });
}

pub fn use_tauri_channel<T>(events: RwSignal<T>) -> Channel
where
    T: DeserializeOwned + Sync + Send + 'static + Clone + PartialEq,
{
    let channel = Channel::new();

    let cb = Closure::wrap(Box::new(move |val: JsValue| {
        let js_string = match js_sys::JSON::stringify(&val) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to stringify channel payload: {:?}", e);
                return;
            }
        };

        let Some(js_string) = js_string.as_string() else {
            error!("Channel payload did not stringify to JSON: {:?}", val);
            return;
        };

        match serde_json::from_str::<T>(&js_string) {
            Ok(payload) => {
                events.set(payload);
            }
            Err(e) => {
                error!("Failed to deserialize channel payload: {}; {:?}", e, val);
            }
        }
    }) as Box<dyn FnMut(JsValue)>);

    channel.set_onmessage(&cb);

    let safe_cb = SendWrapper::new(cb);

    // Drop the closure when the Leptos component unmounts to prevent memory leaks
    on_cleanup(move || {
        drop(safe_cb);
    });

    channel
}
