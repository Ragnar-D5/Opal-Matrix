use icondata as i;
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos_icons::Icon as LIcon;
use phosphor_leptos::{Icon, IconWeight, IconWeightData, PENCIL_SIMPLE};
use serde_json::json;
use web_sys::KeyboardEvent;

use crate::app::call_tauri;
use crate::components::user_profile::MemberProfileExt;
use crate::components::FloatingTile;
use crate::state::{AppState, ProfileStore};

#[derive(Clone, PartialEq)]
enum SettingsIcon {
    IconData(i::Icon),
    Phosphor(&'static IconWeightData),
}

#[derive(Clone)]
struct SettingsSection {
    title: &'static str,
    id: &'static str,
    icon: SettingsIcon,
    render_fn: fn() -> AnyView,
}

impl PartialEq for SettingsSection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

const SETTINGS_SECTIONS: &[SettingsSection] = &[
    SettingsSection {
        title: "Appearance",
        id: "appearance",
        icon: SettingsIcon::IconData(i::BsPalette),
        render_fn: || {
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
            view! {
                <div class="flex items-center justify-between p-4 bg-neutral-800/40 rounded-xl border border-neutral-800/80 w-full">
                    <div class="flex flex-col space-y-1 pr-4">
                        <label for="scaling-slider" class="text-sm font-semibold text-bright">
                            "UI scale"
                        </label>
                        <span class="text-xs text-muted font-mono">
                            {move || {
                                format!("{:.2}x", 0.5 + (slider_value.get() as f64 / 100.0) * 1.5)
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
                                let val = event_target_value(&ev).parse::<i32>().unwrap_or(0);
                                set_slider_value.set(val);
                                let mapped_val = 0.5 + (val as f64 / 100.0) * 1.5;
                                slider_action.dispatch_local(mapped_val);
                            }
                            class="w-full h-2 bg-neutral-700 rounded-lg appearance-none cursor-pointer accent-indigo-500"
                        />
                    </div>
                </div>
            }.into_any()
        },
    },
    SettingsSection {
        title: "Audio",
        id: "audio",
        icon: SettingsIcon::Phosphor(phosphor_leptos::HEADPHONES),
        render_fn: || ().into_any(),
    },
    SettingsSection {
        title: "Input",
        id: "input",
        icon: SettingsIcon::Phosphor(phosphor_leptos::MICROPHONE),
        render_fn: || ().into_any(),
    },
    SettingsSection {
        title: "About",
        id: "about",
        icon: SettingsIcon::Phosphor(phosphor_leptos::X),
        render_fn: || ().into_any(),
    },
];

const PROFILE_SECTION: SettingsSection = SettingsSection {
    title: "Profile",
    id: "profile",
    icon: SettingsIcon::Phosphor(PENCIL_SIMPLE),
    render_fn: || ().into_any(),
};

#[component]
pub fn SettingsIcon(#[prop(into, optional)] class: String) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let sections_dict = SETTINGS_SECTIONS
        .iter()
        .map(|section| (section.id, section.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    let (is_open, set_is_open) = signal(false);

    window_event_listener(leptos::ev::keydown, move |ev: KeyboardEvent| {
        if is_open.try_get_untracked().unwrap_or(false) && ev.key() == "Escape" {
            set_is_open.set(false);
        }
    });

    let selected_section = RwSignal::new(PROFILE_SECTION.id);

    let user_sig = Memo::new(move |_| {
        let user_id = state.user_id.get();
        store.get_user_profile(&user_id)
    });

    let current_section = Memo::new(move |_| {
        sections_dict
            .get(&selected_section.get())
            .unwrap_or(&PROFILE_SECTION)
            .clone()
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
                        class="opacity-100 text-bright w-300 h-full max-h-[95vh] min-h-[50vh] flex flex-row overflow-hidden z-50 !bg-(--opaque-tile-bg-color)"
                    >
                        <div class="border-r border-(--tile-border-color) w-50 h-full flex flex-col gap-1">
                            <div
                                class="flex items-center p-2 border-b border-(--tile-border-color) gap-2 cursor-pointer group"
                                class=(
                                    "bg-(--ui-solid-hover-bg)",
                                    move || selected_section.get() == PROFILE_SECTION.id,
                                )
                                on:click=move |_| selected_section.set(PROFILE_SECTION.id)
                            >
                                <div class="border border-transparent group-hover:border-(--tile-border-color) flex flex-row rounded-[10px] p-1 items-center justify-center flex-1">
                                    {move || user_sig.get().get().render_icon("40px")}
                                    <div class="flex flex-col p-2 rounded-[10px]">
                                        {move || user_sig.get().get().render_name_no_popup("16px")}
                                        <span class="text-xs text-muted flex flex-row pr-2">
                                            <Icon
                                                icon=PENCIL_SIMPLE
                                                size="12px"
                                                weight=IconWeight::Fill
                                            />
                                            "Edit profiles"
                                        </span>
                                    </div>
                                </div>
                            </div>
                            <For
                                each=move || SETTINGS_SECTIONS.iter().cloned()
                                key=|s| s.id
                                children=move |section| {
                                    view! {
                                        <button
                                            class="flex flex-0 items-center gap-3 text-left text-dim hover:text-normal mx-2 rounded-[10px] cursor-pointer px-2 py-1 border border-transparent hover:border-(--tile-border-color)"
                                            class=(
                                                "bg-(--ui-solid-hover-bg)",
                                                move || section.id == selected_section.get(),
                                            )
                                            class=(
                                                "text-normal",
                                                move || section.id == selected_section.get(),
                                            )
                                            on:click=move |_| selected_section.set(section.id)
                                        >
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
                                            }}
                                            <span>{section.title}</span>
                                        </button>
                                    }
                                }
                            />
                        </div>
                        <div class="flex-1 overflow-y-auto">
                            {move || current_section.get().render_fn}
                        </div>
                    </FloatingTile>
                </div>
            </Portal>
        </Show>
    }
}
