use leptos::prelude::*;
use leptos::task::spawn_local;
use send_wrapper::SendWrapper;
use serde::de::DeserializeOwned;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use log::error;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
}

pub fn use_tauri_event<T>(event_name: &str) -> ReadSignal<Option<T>>
where
    T: DeserializeOwned + Clone + Send + Sync + 'static,
{
    let (payload_signal, set_payload_signal) = signal(None::<T>);
    let event_name_owned = event_name.to_string();

    Effect::new(move |_| {
        let name = event_name_owned.clone();
        let owner = Owner::current().expect("Leptos owner required");

        let future = SendWrapper::new(async move {
            let closure = Closure::wrap(Box::new(move |js_val: JsValue| {
                let payload_js = match js_sys::Reflect::get(&js_val, &JsValue::from_str("payload"))
                {
                    Ok(v) => v,
                    Err(e) => {
                        error!("Failed to read Tauri event payload field: {:?}", e);
                        return;
                    }
                };

                match serde_wasm_bindgen::from_value::<T>(payload_js.clone()) {
                    Ok(payload) => set_payload_signal.set(Some(payload)),
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
