use icondata as i;
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos_icons::Icon as LIcon;
use phosphor_leptos::{Icon, IconWeight, IconWeightData, CAMERA, PAINT_BRUSH, PENCIL_SIMPLE};
use serde_json::json;
use web_sys::{HtmlButtonElement, KeyboardEvent};

use crate::app::call_tauri;
use crate::components::presence::PresenceBadge;
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
    render_fn: render_profile_section,
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
                                on:click=move |_| selected_section.set(PROFILE_SECTION.id)
                            >
                                <div
                                    class="border border-transparent group-hover:border-(--tile-border-color) flex flex-row rounded-[10px] p-1 items-center justify-center flex-1"
                                    class=(
                                        "bg-(--ui-solid-hover-bg)",
                                        move || selected_section.get() == PROFILE_SECTION.id,
                                    )
                                >
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

fn render_profile_section() -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let user_id = state.user_id.get_untracked();

    // RwSignal<Option<String>>: None = global profile, Some(room_id) = room-specific
    let selected_room: RwSignal<Option<String>> = RwSignal::new(None);

    let presence = store.get_presence(&user_id);

    let store_banner = store.clone();
    let store_icon = store.clone();
    let store_name = store.clone();
    let uid_banner = user_id.clone();
    let uid_icon = user_id.clone();
    let uid_name = user_id.clone();

    // Banner geometry constants
    let banner_height = 108.0_f64;
    let icon_size = 70.0_f64;
    let icon_radius = icon_size / 2.0;
    let ring_width = 6.0_f64;
    let left_offset = 16.0_f64;
    let cutout_radius = icon_radius + ring_width;
    let smooth_cutout_radius = cutout_radius + 0.5;
    let cx = left_offset + icon_radius;
    let cy = banner_height;
    let banner_mask = format!(
        "-webkit-mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
         mask-image: radial-gradient(circle at {cx}px {cy}px, transparent {cutout_radius}px, black {smooth_cutout_radius}px); \
         -webkit-mask-composite: destination-out; \
         mask-composite: exclude;",
    );
    let icon_top = banner_height - icon_radius;
    let icon_size_str = format!("{icon_size}px");
    let badge_size = (icon_size * 25.0 / 70.0) as f32;
    let uid_display = user_id.clone();

    let in_room_selection = RwSignal::new(false);

    let global_btn = NodeRef::new();
    let room_btn = NodeRef::new();

    let pill_left = RwSignal::new(0);
    let pill_width = RwSignal::new(0);
    let has_measured = RwSignal::new(false);

    Effect::new(move |_| {
        let is_room = in_room_selection.get();
        let target_node: Option<HtmlButtonElement> = if is_room {
            room_btn.get()
        } else {
            global_btn.get()
        };

        if let Some(el) = target_node {
            request_animation_frame(move || {
                pill_left.set(el.offset_left());
                pill_width.set(el.offset_width());

                if !has_measured.get_untracked() {
                    set_timeout(
                        move || has_measured.set(true),
                        std::time::Duration::from_millis(50),
                    );
                }
            });
        }
    });

    let pill_style = move || {
        let w = pill_width.get();
        let l = pill_left.get();
        if w > 0 {
            format!("left: {}px; width: {}px; opacity: 1;", l, w)
        } else {
            "opacity: 0;".to_string()
        }
    };

    view! {
        <div class="flex flex-col h-full">
            <div class="relative flex flex-row w-full gap-5 px-5 pt-5">
                <button
                    node_ref=global_btn
                    class="text-sm font-medium hover:text-normal"
                    class=("text-(--accent-color)", move || !in_room_selection.get())
                    class=("text-dim", move || in_room_selection.get())
                    on:click=move |_| in_room_selection.set(false)
                >
                    "Global"
                </button>
                <button
                    node_ref=room_btn
                    class="text-sm font-medium hover:text-normal"
                    class=("text-(--accent-color)", move || in_room_selection.get())
                    class=("text-dim", move || !in_room_selection.get())
                    on:click=move |_| in_room_selection.set(true)
                >
                    "Per Room"
                </button>

                <div
                    class="absolute bottom-0 h-[2px] rounded-full bg-(--accent-color)"
                    class=("transition-all", move || has_measured.get())
                    class=("duration-100", move || has_measured.get())
                    class=("ease-in-out", move || has_measured.get())
                    style=pill_style
                />
            </div>
            <div class="w-56 shrink-0 border border-(--tile-border-color) flex flex-col m-5 rounded-(--ui-border-radius) overflow-hidden">
                <div class="relative">
                    <div
                        class="w-full cursor-pointer group relative overflow-hidden"
                        style=move || {
                            format!(
                                "height: {banner_height}px; background-color: {}; {banner_mask}",
                                store_banner
                                    .get_profile_signal(selected_room.get(), &uid_banner)
                                    .banner_color(),
                            )
                        }
                        on:click=move |_| {}
                    >
                        <div class="absolute inset-0 bg-(--overlay-bg-color) opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center pointer-events-none text-bright">
                            <Icon icon=PAINT_BRUSH size="24px" weight=IconWeight::Light />
                        </div>
                    </div>

                    <div
                        class="absolute group cursor-pointer"
                        style=format!("top: {icon_top}px; left: {left_offset}px;")
                        on:click=move |_| {}
                    >
                        <PresenceBadge
                            presence=presence.clone()
                            size=badge_size
                            indicator_class="z-100"
                        >
                            {move || {
                                store_icon
                                    .get_profile_signal(selected_room.get(), &uid_icon)
                                    .icon(icon_size_str.clone())
                            }}
                        </PresenceBadge>
                        <div
                            class="absolute rounded-full bg-(--overlay-bg-color) opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center pointer-events-none text-bright"
                            style=format!(
                                "top: 0; left: 0; width: {icon_size}px; height: {icon_size}px;",
                            )
                        >
                            <Icon icon=CAMERA size="20px" />
                        </div>
                    </div>
                </div>

                <div class="px-4 pb-4" style=format!("padding-top: {icon_radius}px;")>
                    <div class="text-base font-bold text-bright">
                        {move || {
                            store_name
                                .get_profile_signal(selected_room.get(), &uid_name)
                                .name("16px".to_string())
                        }}
                    </div>
                    <div class="text-xs text-muted truncate">{uid_display}</div>
                </div>
            </div>
        </div>
    }
    .into_any()
}
