use icondata as i;
use leptos::portal::Portal;
use leptos::prelude::*;
use leptos_icons::Icon as LIcon;
use phosphor_leptos::{
    Icon, IconWeight, IconWeightData, CAMERA, CARET_DOWN, PAINT_BRUSH, PENCIL_SIMPLE,
};
use serde_json::json;
use web_sys::{HtmlButtonElement, KeyboardEvent};

use crate::app::call_tauri;
use crate::components::presence::PresenceBadge;
use crate::components::user_profile::{render_url_icon, MemberProfileExt};
use crate::components::{CloseButton, FloatingTile};
use crate::state::{AppState, ProfileSignal, ProfileStore};

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

        <Show when=move || is_open.get() fallback=|| ()>
            <Portal>
                <div
                    on:click=move |_| set_is_open.set(false)
                    on:wheel=|e| e.stop_propagation()
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
                                        <span
                                            class="text-xs group-hover:underline flex flex-row pr-2"
                                            class=(
                                                "text-normal",
                                                move || selected_section.get() == PROFILE_SECTION.id,
                                            )
                                            class=(
                                                "text-muted",
                                                move || selected_section.get() != PROFILE_SECTION.id,
                                            )
                                        >
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
                        <div class="w-full h-full flex flex-col">
                            <div class="w-full h-(--header-height) shrink-0">
                                <span class="text-xl font-bold text-normal p-4 block border-b border-(--tile-border-color)">
                                    {move || current_section.get().title}
                                </span>
                                <CloseButton
                                    on_click=move |_| set_is_open.set(false)
                                    size="20px"
                                    inset="12px"
                                />
                            </div>
                            <div class="p-4 flex-1 min-h-0 overflow-auto">
                                {move || current_section.get().render_fn}
                            </div>
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

    let store_icon = store.clone();
    let store_fields = store.clone();
    let uid_icon = user_id.clone();
    let uid_fields = user_id.clone();

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

    let room_resource = Memo::new(move |_| state.get_rooms());
    let is_dropdown_open = RwSignal::new(false);
    let search_text: RwSignal<String> = RwSignal::new(String::new());
    let is_focused = RwSignal::new(false);

    let picker_or_info = {
        move || {
            if in_room_selection.get() {
                view! {
                    <span class="p-1 text-normal">"Select a room:"</span>

                    <div class="relative">
                        <div
                            class="relative h-7 flex items-center w-48 border rounded-(--ui-border-radius) overflow-hidden"
                            class=("border-(--accent-color)", move || is_dropdown_open.get())
                            class=("border-(--tile-border-color)", move || !is_dropdown_open.get())
                        >
                            <input
                                type="text"
                                class="flex-1 min-w-0 px-2 bg-transparent text-left text-sm text-normal outline-none"
                                placeholder=move || {
                                    if selected_room.get().is_some() { "" } else { "Select a room..." }
                                }
                                prop:value=move || search_text.get()
                                on:input=move |ev| {
                                    search_text.set(event_target_value(&ev));
                                    is_dropdown_open.set(true);
                                }
                                on:focus=move |_| {
                                    is_focused.set(true);
                                    search_text.set(String::new());
                                    is_dropdown_open.set(true);
                                }
                                on:blur=move |_| is_focused.set(false)
                            />
                            <button
                                class="shrink-0 flex items-center pr-1 cursor-pointer"
                                tabindex="-1"
                                on:click=move |_| is_dropdown_open.update(|open| *open = !*open)
                            >
                                <Icon icon=CARET_DOWN size="16px" color="var(--text-muted)" />
                            </button>
                            <Show when=move || !is_focused.get() && selected_room.get().is_some()>
                                <div class="absolute inset-0 flex items-center gap-2 px-2 pointer-events-none bg-(--opaque-tile-bg-color) overflow-hidden">
                                    {move || {
                                        let selected_id = selected_room.get();
                                        room_resource
                                            .get()
                                            .into_iter()
                                            .find(|room| Some(room.room_id.clone()) == selected_id)
                                            .map(|room| {
                                                view! {
                                                    {render_url_icon(
                                                        room.avatar_url(),
                                                        room.get_name(),
                                                        "16px",
                                                        room.color.clone(),
                                                        "[25%]",
                                                    )}
                                                    <span class="truncate text-sm text-normal">
                                                        {room.name.clone()}
                                                    </span>
                                                }
                                            })
                                    }}
                                </div>
                            </Show>
                        </div>

                        <Show when=move || is_dropdown_open.get()>
                            <div
                                class="fixed inset-0 z-40"
                                on:click=move |_| is_dropdown_open.set(false)
                            />
                            <div class="absolute top-full left-0 mt-1 w-48 max-h-60 overflow-y-auto bg-(--ui-solid-bg) border border-(--tile-border-color) rounded-(--ui-border-radius) shadow-lg z-50 py-1 flex flex-col">
                                <For
                                    each=move || {
                                        let search = search_text.get().to_lowercase();
                                        room_resource
                                            .get()
                                            .into_iter()
                                            .filter(move |room| {
                                                search.is_empty()
                                                    || room
                                                        .get_name()
                                                        .to_lowercase()
                                                        .contains(&search)
                                                    || room.room_id.to_lowercase().contains(&search)
                                            })
                                            .collect::<Vec<_>>()
                                    }
                                    key=|room| room.room_id.clone()
                                    children=move |room| {
                                        let room_id = room.room_id.clone();
                                        let is_selected =
                                            Some(room_id.clone()) == selected_room.get();
                                        view! {
                                            <button
                                                class="flex items-center w-full gap-2 px-3 py-2 text-left transition-colors hover:bg-(--overlay-bg-color)"
                                                class=("bg-(--overlay-bg-color)", is_selected)
                                                on:click=move |_| {
                                                    selected_room.set(Some(room_id.clone()));
                                                    search_text.set(String::new());
                                                    is_dropdown_open.set(false);
                                                }
                                            >
                                                {render_url_icon(
                                                    room.avatar_url(),
                                                    room.get_name(),
                                                    "16px",
                                                    room.color,
                                                    "[25%]",
                                                )}
                                                <span class="truncate text-normal">
                                                    {room.name.clone()}
                                                </span>
                                            </button>
                                        }
                                    }
                                />
                                <Show when=move || {
                                    let search = search_text.get().to_lowercase();
                                    !search.is_empty()
                                        && room_resource.get().into_iter().all(|room| {
                                            !room.get_name().to_lowercase().contains(&search)
                                                && !room.room_id.to_lowercase().contains(&search)
                                        })
                                }>
                                    <span class="px-3 py-2 text-sm text-muted italic">
                                        "No rooms found"
                                    </span>
                                </Show>
                            </div>
                        </Show>
                    </div>
                }.into_any()
            } else {
                view! {
                    <div class="text-sm text-muted italic">
                        "Settings that apply to all rooms and conversations."
                    </div>
                }
                .into_any()
            }
        }
    };

    let displayname_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    let bannercolor_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    let namecolor_ref: NodeRef<leptos::html::Input> = NodeRef::new();
    let hue_bar_ref: NodeRef<leptos::html::Div> = NodeRef::new();

    let displayname_val = RwSignal::new(String::new());
    let bannercolor_val = RwSignal::new(String::new());
    let namecolor_val = RwSignal::new(String::new());

    Effect::new(move |_| {
        let (dn, bc, nc) = match store_fields.get_profile_signal(selected_room.get(), &uid_fields) {
            ProfileSignal::User(sig) => {
                let p = sig.get();
                (
                    p.display_name.clone().unwrap_or_default(),
                    p.banner_color().to_css_hex(),
                    format!("{}", p.name_color().to_hsla()[0].round() as u32),
                )
            }
            ProfileSignal::Member(sig) => {
                let p = sig.get();
                (
                    p.profile.display_name.clone().unwrap_or_default(),
                    p.banner_color().to_css_hex(),
                    format!("{}", p.name_color().to_hsla()[0].round() as u32),
                )
            }
        };
        displayname_val.set(dn);
        bannercolor_val.set(bc);
        namecolor_val.set(nc);
    });

    let save_displayname = Action::new_local(move |(name, room_id): &(String, Option<String>)| {
        let name = name.clone();
        let room_id = room_id.clone();
        async move {
            call_tauri(
                "set_display_name",
                serde_wasm_bindgen::to_value(&json!({ "display_name": name, "room_id": room_id }))
                    .unwrap(),
            )
            .await
        }
    });

    let save_bannercolor = Action::new_local(move |(color, room_id): &(String, Option<String>)| {
        let color = color.clone();
        let room_id = room_id.clone();
        async move {
            call_tauri(
                "set_banner_color",
                serde_wasm_bindgen::to_value(&json!({ "color": color, "room_id": room_id }))
                    .unwrap(),
            )
            .await
        }
    });

    let save_namecolor = Action::new_local(move |(hue, room_id): &(String, Option<String>)| {
        let hue: f64 = hue.parse().unwrap_or(0.0);
        let room_id = room_id.clone();
        async move {
            call_tauri(
                "set_name_color",
                serde_wasm_bindgen::to_value(&json!({ "hue": hue, "room_id": room_id })).unwrap(),
            )
            .await
        }
    });

    view! {
        <div class="flex flex-col h-full">
            <div class="relative flex flex-row w-full gap-5 px-5 pt-5">
                <button
                    node_ref=global_btn
                    class="font-medium hover:text-normal cursor-pointer"
                    class=("text-(--accent-color)", move || !in_room_selection.get())
                    class=("text-dim", move || in_room_selection.get())
                    on:click=move |_| in_room_selection.set(false)
                >
                    "Global"
                </button>
                <button
                    node_ref=room_btn
                    class="font-medium hover:text-normal cursor-pointer"
                    class=("text-(--accent-color)", move || in_room_selection.get())
                    class=("text-dim", move || !in_room_selection.get())
                    on:click=move |_| in_room_selection.set(true)
                >
                    "Room-specific"
                </button>

                <div
                    class="absolute bottom-0 h-[2px] rounded-full bg-(--accent-color)"
                    class=("transition-all", move || has_measured.get())
                    class=("duration-100", move || has_measured.get())
                    class=("ease-in-out", move || has_measured.get())
                    style=pill_style
                />
            </div>
            <div class="border-b border-(--tile-border-color) p-2 h-13 flex flex-row items-center gap-4">
                {picker_or_info}
            </div>
            <div class="flex flex-row gap-5 m-5 items-start">
                <div class="w-56 shrink-0 border border-(--tile-border-color) flex flex-col rounded-(--ui-border-radius) overflow-hidden">
                    <div class="relative">
                        <div
                            class="w-full cursor-pointer group relative overflow-hidden"
                            style=move || {
                                format!(
                                    "height: {banner_height}px; background-color: {}; {banner_mask}",
                                    bannercolor_val.get(),
                                )
                            }
                            on:click=move |_| {
                                if let Some(el) = bannercolor_ref.get() {
                                    let _ = el.focus();
                                }
                            }
                        >
                            <div class="absolute inset-0 bg-(--overlay-bg-color) opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center pointer-events-none text-bright">
                                <Icon icon=PAINT_BRUSH size="24px" weight=IconWeight::Light />
                            </div>
                        </div>

                        <div
                            class="absolute group cursor-pointer"
                            style=format!("top: {icon_top}px; left: {left_offset}px;")
                            on:click=move |_| {
                                if let Some(el) = namecolor_ref.get() {
                                    let _ = el.focus();
                                }
                            }
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
                        <div class="flex flex-row">
                            <div
                                class="text-base font-bold text-bright cursor-pointer"
                                on:click=move |_| {
                                    if let Some(el) = displayname_ref.get() {
                                        let _ = el.focus();
                                    }
                                }
                            >
                                <span
                                    class="font-bold"
                                    style=move || format!(
                                        "font-size: 16px; color: hsl({}deg, 90%, 70%);",
                                        namecolor_val.get()
                                    )
                                >
                                    {move || displayname_val.get()}
                                </span>
                            </div>
                            <button class="ml-2 text-muted hover:text-normal" on:click=move |_| {
                                if let Some(el) = namecolor_ref.get() {
                                    let _ = el.focus();
                                }
                            }>
                                <Icon icon=PAINT_BRUSH size="12px" weight=IconWeight::Thin />
                            </button>
                        </div>
                        <div class="text-xs text-muted truncate">{uid_display}</div>
                    </div>
                </div>

                <div class="flex flex-col gap-4 flex-1 pt-1">
                    <div class="flex flex-col gap-1">
                        <label class="text-xs text-muted font-medium">"Displayname"</label>
                        <div class="flex gap-2">
                            <input
                                node_ref=displayname_ref
                                type="text"
                                class="flex-1 min-w-0 px-2 py-1 bg-transparent border border-(--tile-border-color) rounded-(--ui-border-radius) text-sm text-normal outline-none focus:border-(--accent-color)"
                                prop:value=move || displayname_val.get()
                                on:input=move |ev| displayname_val.set(event_target_value(&ev))
                            />
                            <button
                                class="shrink-0 px-3 py-1 text-sm bg-(--accent-color) text-white rounded-(--ui-border-radius) hover:brightness-110 cursor-pointer"
                                on:click=move |_| {
                                    save_displayname
                                        .dispatch_local((displayname_val.get(), selected_room.get()));
                                }
                            >
                                "Save"
                            </button>
                        </div>
                    </div>

                    <div class="flex flex-col gap-1">
                        <label class="text-xs text-muted font-medium">"Bannercolor"</label>
                        <div class="flex gap-2">
                            <input
                                node_ref=bannercolor_ref
                                type="text"
                                class="flex-1 min-w-0 px-2 py-1 bg-transparent border border-(--tile-border-color) rounded-(--ui-border-radius) text-sm text-normal outline-none focus:border-(--accent-color)"
                                prop:value=move || bannercolor_val.get()
                                on:input=move |ev| bannercolor_val.set(event_target_value(&ev))
                            />
                            <button
                                class="shrink-0 px-3 py-1 text-sm bg-(--accent-color) text-white rounded-(--ui-border-radius) hover:brightness-110 cursor-pointer"
                                on:click=move |_| {
                                    save_bannercolor
                                        .dispatch_local((bannercolor_val.get(), selected_room.get()));
                                }
                            >
                                "Save"
                            </button>
                        </div>
                    </div>

                    <div class="flex flex-col gap-1">
                        <label class="text-xs text-muted font-medium">"Namecolor"</label>
                        <div class="flex gap-2">
                            <input
                                node_ref=namecolor_ref
                                type="number"
                                min="0"
                                max="360"
                                class="flex-1 min-w-0 px-2 py-1 bg-transparent border border-(--tile-border-color) rounded-(--ui-border-radius) text-sm text-normal outline-none focus:border-(--accent-color)"
                                prop:value=move || namecolor_val.get()
                                on:input=move |ev| namecolor_val.set(event_target_value(&ev))
                            />
                            <button
                                class="shrink-0 px-3 py-1 text-sm bg-(--accent-color) text-white rounded-(--ui-border-radius) hover:brightness-110 cursor-pointer"
                                on:click=move |_| {
                                    save_namecolor
                                        .dispatch_local((namecolor_val.get(), selected_room.get()));
                                }
                            >
                                "Save"
                            </button>
                        </div>
                        <div
                            node_ref=hue_bar_ref
                            class="relative h-3 rounded-full cursor-crosshair select-none touch-none"
                            style="background: linear-gradient(to right in hsl increasing hue, hsl(0,90%,70%), hsl(359 ,90%,70%))"
                            on:pointerdown=move |ev| {
                                ev.prevent_default();
                                if let Some(el) = hue_bar_ref.get() {
                                    let _ = el.set_pointer_capture(ev.pointer_id());
                                    let rect = el.get_bounding_client_rect();
                                    let x = (ev.client_x() as f64 - rect.left()).clamp(0.0, rect.width());
                                    namecolor_val.set(format!("{}", (x / rect.width() * 360.0).round() as u32));
                                }
                            }
                            on:pointermove=move |ev| {
                                if ev.buttons() == 0 { return; }
                                if let Some(el) = hue_bar_ref.get() {
                                    let rect = el.get_bounding_client_rect();
                                    let x = (ev.client_x() as f64 - rect.left()).clamp(0.0, rect.width());
                                    namecolor_val.set(format!("{}", (x / rect.width() * 360.0).round() as u32));
                                }
                            }
                        >
                            <div
                                class="absolute top-1/2 -translate-y-1/2 w-3 h-3 rounded-full border-2 border-white pointer-events-none"
                                style=move || {
                                    let hue: f64 = namecolor_val.get().parse().unwrap_or(0.0);
                                    format!(
                                        "left: clamp(0px, calc({}% - 6px), calc(100% - 12px)); background-color: hsl({}deg, 90%, 70%); box-shadow: 0 0 0 1.5px black, 0 2px 6px rgba(0,0,0,0.5);",
                                        hue / 360.0 * 100.0,
                                        hue as u32,
                                    )
                                }
                            />
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
    .into_any()
}
