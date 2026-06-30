use leptos::{ev, html::Button, prelude::*};
use leptos_use::use_event_listener;
use phosphor_leptos::{Icon, IconWeight, CARET_RIGHT, GEAR, HEADPHONES, MICROPHONE};
use wasm_bindgen::JsCast;

use crate::{
    state::AppState,
    tauri_functions::{set_input_device, set_output_device},
};

pub fn audio_device_popup(on_close: Callback<()>, input: bool) -> AnyView {
    let devices_open = RwSignal::new(false);
    let app_state: AppState = expect_context();
    let popup_ref = NodeRef::<Button>::new();

    let _ = use_event_listener(document(), ev::click, move |e| {
        let Some(popup) = popup_ref.get() else { return };
        let Some(target) = e.target() else { return };
        let Some(target_node) = target.dyn_ref::<web_sys::Node>() else {
            return;
        };
        if !popup.contains(Some(target_node)) {
            on_close.run(());
        }
    });

    let devices = move || {
        let devices = app_state.audio_devices.get();
        let active_id = devices.get_active_device(input).map(|d| d.id.clone());

        let all_devices = if input {
            &devices.input_devices
        } else {
            &devices.output_devices
        };

        let mut items: Vec<(String, String)> = all_devices
            .iter()
            .map(|d| (d.id.clone(), d.name.clone()))
            .collect();
        items.sort_by_key(|(id, _)| id.clone());

        if let Some(id) = devices.default_output_device_id.clone() {
            items.retain(|(item_id, _)| item_id != &id);
            items.insert(0, (id.clone(), "System Default".to_string()));
        } else if let Some(id) = devices.active_output_device_id.clone() {
            items.retain(|(item_id, _)| item_id != &id);
            items.insert(0, (id.clone(), "System Default".to_string()));
        }

        items
            .into_iter()
            .map(|(id, name)| {
                let is_active = active_id.as_ref() == Some(&id);
                let id_clone = id.clone();

                view! {
                    <button
                        class="w-full flex items-center gap-3 px-3 py-2.5 hover:bg-(--color-item-hover) cursor-pointer text-left"
                        on:click=move |_| {
                            if !id_clone.is_empty()
                                && let Err(e) = if input {
                                    set_input_device(id_clone.clone())
                                } else {
                                    set_output_device(id_clone.clone())
                                }
                            {
                                log::error!("Failed to set device: {:?}", e)
                            }
                        }
                    >
                        <span class="text-muted flex-shrink-0">
                            <Icon
                                icon=if input { MICROPHONE } else { HEADPHONES }
                                size="15px"
                                weight=IconWeight::Fill
                            />
                        </span>
                        <span class="text-sm text-bright truncate flex-1">{name}</span>
                        <div
                            class="flex-shrink-0 w-3.5 h-3.5 rounded-full border-2 flex items-center justify-center"
                            style=if is_active {
                                "border-color: var(--accent-color);"
                            } else {
                                "border-color: var(--offline-color);"
                            }
                        >
                            {is_active
                                .then(|| {
                                    view! {
                                        <div
                                            class="w-1.5 h-1.5 rounded-full"
                                            style="background-color: var(--online-color);"
                                        />
                                    }
                                })}
                        </div>
                    </button>
                }
            })
            .collect_view()
    };

    view! {
        <button
            node_ref=popup_ref
            class="absolute bottom-full left-0 flex flex-row items-start mb-[calc(var(--gap)+1px)] z-50 gap-(--gap)"
            on:mouseleave=move |_| {
                devices_open.set(false);
            }
        >
            <div class="bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--floating-border-radius) overflow-hidden w-[260px]">
                <button
                    class="w-full flex flex-col px-3 py-3 hover:bg-(--color-item-hover) cursor-pointer text-left"
                    on:mouseenter=move |_| {
                        devices_open.set(true);
                    }
                >
                    <div class="flex items-center justify-between w-full">
                        <span class="text-sm font-medium text-bright">"Input Device"</span>
                        <span class="text-muted">
                            <Icon icon=CARET_RIGHT size="14px" />
                        </span>
                    </div>
                    <span class="text-xs text-muted truncate mt-0.5">
                        {move || {
                            let devices = app_state.audio_devices.get();
                            if let Some(device) = &devices.get_active_device(input) {
                                devices
                                    .input_devices
                                    .iter()
                                    .find(|d| d.id == device.id)
                                    .map(|d| d.name.clone())
                                    .unwrap_or_else(|| "System Default".to_string())
                            } else {
                                "System Default".to_string()
                            }
                        }}
                    </span>
                </button>

                <div class="h-px bg-(--tile-border-color)" />

                <div class="flex flex-col px-3 py-3 opacity-50 cursor-not-allowed select-none">
                    <div class="flex items-center justify-between w-full">
                        <span class="text-sm font-medium text-bright">"Input Profile"</span>
                        <span class="text-muted">
                            <Icon icon=CARET_RIGHT size="14px" />
                        </span>
                    </div>
                    <span class="text-xs text-muted mt-0.5">"Voice Isolation"</span>
                </div>

                <div class="h-px bg-(--tile-border-color)" />

                // Input Volume
                <div class="px-3 py-3">
                    <span class="text-sm font-medium text-bright block mb-2">"Input Volume"</span>
                    <input
                        type="range"
                        min="0"
                        max="100"
                        value="80"
                        class="w-full cursor-pointer"
                        style="accent-color: var(--accent-color);"
                    />
                </div>

                <div class="h-px bg-(--tile-border-color)" />

                <div class="flex items-center justify-between px-3 py-3">
                    <span class="text-sm font-medium text-bright">"Voice Settings"</span>
                    <span class="text-muted hover:text-bright cursor-pointer">
                        <Icon icon=GEAR size="16px" />
                    </span>
                </div>
            </div>

            <Show when=move || devices_open.get()>
                <button class="bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--floating-border-radius) overflow-hidden w-[240px]">
                    {devices}
                </button>
            </Show>
        </button>
    }.into_any()
}
