use icondata as i;
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos_icons::Icon as LIcon;
use phosphor_leptos::{Icon, IconWeight, IconWeightData};
use serde_json::json;
use web_sys::KeyboardEvent;

use crate::app::call_tauri;
use crate::components::FloatingTile;

#[derive(Clone)]
enum SettingsIcon {
    IconData(i::Icon),
    Phosphor(&'static IconWeightData),
}

#[derive(Clone)]
struct SettingsSection {
    title: &'static str,
    id: &'static str,
    icon: SettingsIcon,
}

const SETTINGS_SECTIONS: &[SettingsSection] = &[
    SettingsSection {
        title: "Appearance",
        id: "appearance",
        icon: SettingsIcon::IconData(i::BsPalette),
    },
    SettingsSection {
        title: "Audio",
        id: "audio",
        icon: SettingsIcon::Phosphor(phosphor_leptos::HEADPHONES),
    },
    SettingsSection {
        title: "Input",
        id: "input",
        icon: SettingsIcon::Phosphor(phosphor_leptos::MICROPHONE),
    },
    SettingsSection {
        title: "About",
        id: "about",
        icon: SettingsIcon::Phosphor(phosphor_leptos::X),
    },
];

#[component]
pub fn SettingsIcon(#[prop(into, optional)] class: String) -> impl IntoView {
    let (is_open, set_is_open) = signal(false);

    let (slider_value, set_slider_value) = signal(50);
    let slider_action = Action::new_local(|new_value: &f64| {
        let val_clone = *new_value;
        async move {
            call_tauri(
                "change_screen_scaling",
                serde_wasm_bindgen::to_value(&json!({"scale_factor": val_clone})).unwrap(),
            )
            .await
        }
    });
    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if is_open.try_get_untracked().unwrap_or(false) && ev.key() == "Escape" {
            set_is_open.set(false);
        }
    });

    view! {
        <button
            on:click=move |_| set_is_open.update(|v| *v = !*v)
            class=format!(
                "text-muted hover:text-bright cursor-pointer transition-transform duration-300 ease-in-out hover:rotate-[90deg] {class}",
            )
        >
            <LIcon icon=i::BsGearWideConnected height="20px" />
        </button>

        <Show when=move || is_open.get() fallback=|| view! { "" }>
            <Portal>
                <div
                    on:click=move |_| set_is_open.set(false)
                    class="fixed inset-0 z-40 bg-(--overlay-bg-color) backdrop-blur-sm flex items-center justify-center p-6 md:p-12"
                >
                    <FloatingTile
                        on:click=move |e| e.stop_propagation()
                        class="opacity-100 text-bright w-400 h-full max-h-[95vh] min-h-[50vh] flex flex-row overflow-hidden z-50 !bg-(--opaque-tile-bg-color)"
                    >
                        <div class="border-r border-(--tile-border-color) w-80 h-full">
                            <For
                                each=move || SETTINGS_SECTIONS.iter().cloned()
                                key=|s| s.id
                                children=move |section| {
                                    view! {
                                        <button class="flex items-center gap-3 p-2 w-full text-left text-dim hover:bg-(--ui-solid-hover-bg) hover:text-normal">
                                            {match section.icon {
                                                SettingsIcon::IconData(icon_data) => {
                                                    view! { <LIcon icon=icon_data height="18px" /> }.into_any()
                                                }
                                                SettingsIcon::Phosphor(phosphor_icon) => {
                                                    view! {
                                                        <Icon
                                                            icon=phosphor_icon
                                                            size="18px"
                                                            weight=IconWeight::Fill
                                                        />
                                                    }
                                                        .into_any()
                                                }
                                            }} <span>{section.title}</span>
                                        </button>
                                    }
                                }
                            />
                        </div>
                        <div class="flex-1 overflow-y-auto">
                            <div class="flex items-center justify-between p-4 bg-neutral-800/40 rounded-xl border border-neutral-800/80 w-full">
                                <div class="flex flex-col space-y-1 pr-4">
                                    <label
                                        for="scaling-slider"
                                        class="text-sm font-semibold text-bright"
                                    >
                                        "UI scale"
                                    </label>
                                    <span class="text-xs text-muted font-mono">
                                        {move || {
                                            format!(
                                                "{:.2}x",
                                                0.5 + (slider_value.get() as f64 / 100.0) * 1.5,
                                            )
                                        }}
                                    </span>
                                </div>

                                <div class="w-full max-w-xs md:max-w-md">
                                    <input
                                        id="scaling-slider"
                                        type="range"
                                        min="0"
                                        max="100"
                                        prop:value=move || slider_value.get()

                                        on:input=move |ev| {
                                            let val = event_target_value(&ev)
                                                .parse::<i32>()
                                                .unwrap_or(0);
                                            set_slider_value.set(val);
                                            let mapped_val = 0.5 + (val as f64 / 100.0) * 1.5;
                                            slider_action.dispatch_local(mapped_val);
                                        }
                                        class="w-full h-2 bg-neutral-700 rounded-lg appearance-none cursor-pointer accent-indigo-500"
                                    />
                                </div>
                            </div>
                        </div>
                    </FloatingTile>
                </div>
            </Portal>
        </Show>
    }
}
