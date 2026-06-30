use leptos::prelude::*;
use leptos::task::spawn_local;
use send_wrapper::SendWrapper;
use serde::de::DeserializeOwned;
use shared::api::events::TauriEvent;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

use log::error;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
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

pub fn use_tauri_event<T>() -> ReadSignal<Option<T>>
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
