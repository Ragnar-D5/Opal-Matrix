use leptos::prelude::*;
use phosphor_leptos::{CAMERA, CARET_DOWN, Icon, IconWeight, PAINT_BRUSH};
use ruma::OwnedRoomId;
use web_sys::HtmlButtonElement;

use shared::get_color;

use crate::tauri_functions::{save_banner_color, save_displayname, save_name_color};

use crate::components::presence::PresenceBadge;
use crate::components::user_profile::render_url_icon;
use crate::state::{AppState, ProfileSignal, ProfileStore};

pub fn render_profile_section() -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let Some(user_id) = state.user_id.get_untracked() else {
        return ().into_any();
    };

    let selected_room: RwSignal<Option<OwnedRoomId>> = RwSignal::new(None);

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
    let uid_display = user_id.clone().to_string();
    let uid_reset = user_id.clone().to_string();

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
                            class="relative h-7 flex items-center w-48 border rounded-ui overflow-hidden"
                            class=("border-(--accent-color)", move || is_dropdown_open.get())
                            class=("border-(--tile-border-color)", move || !is_dropdown_open.get())
                        >
                            <input
                                type="text"
                                class="flex-1 min-w-0 px-2 bg-transparent text-left text-sm text-normal outline-none"
                                placeholder=move || {
                                    if selected_room.get().is_some() {
                                        ""
                                    } else {
                                        "Select a room..."
                                    }
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
                                            .find(|room| { Some(room.room_id()) == selected_id })
                                            .map(|room| {
                                                view! {
                                                    {render_url_icon(
                                                        room.avatar_url(),
                                                        room.name(),
                                                        "16px",
                                                        room.color(),
                                                        "[25%]",
                                                    )}
                                                    <span class="truncate text-sm text-normal">
                                                        {room.name()}
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
                            <div class="absolute top-full left-0 mt-1 w-48 max-h-60 overflow-y-auto ui-solid-bg border border-(--tile-border-color) rounded-ui shadow-lg z-50 py-1 flex flex-col">
                                <For
                                    each=move || {
                                        let search = search_text.get().to_lowercase();
                                        room_resource
                                            .get()
                                            .into_iter()
                                            .filter(move |room| {
                                                search.is_empty()
                                                    || room.name().to_lowercase().contains(&search)
                                                    || room
                                                        .room_id()
                                                        .to_string()
                                                        .to_lowercase()
                                                        .contains(&search)
                                            })
                                            .collect::<Vec<_>>()
                                    }
                                    key=|room| room.room_id().to_string()
                                    children=move |room| {
                                        let room_id = room.room_id();
                                        let is_selected = Some(room_id.clone())
                                            == selected_room.get();
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
                                                    room.name(),
                                                    "16px",
                                                    room.color(),
                                                    "[25%]",
                                                )}
                                                <span class="truncate text-normal">{room.name()}</span>
                                            </button>
                                        }
                                    }
                                />
                                <Show when=move || {
                                    let search = search_text.get().to_lowercase();
                                    !search.is_empty()
                                        && room_resource
                                            .get()
                                            .into_iter()
                                            .all(|room| {
                                                !room.name().to_lowercase().contains(&search)
                                                    && !room
                                                        .room_id()
                                                        .to_string()
                                                        .to_lowercase()
                                                        .contains(&search)
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
    let namecolor_val = RwSignal::new(0);

    let strip_alpha = |hex: String| hex.chars().take(7).collect::<String>();

    Effect::new(move |_| {
        let (dn, bc, nc) = match store_fields.get_profile_signal(selected_room.get(), &uid_fields) {
            ProfileSignal::User(sig) => {
                let p = sig.get();
                (
                    p.display_name.clone().unwrap_or_default(),
                    strip_alpha(p.banner_color().to_css_hex()),
                    p.name_color().to_hsla()[0] as u32,
                )
            }
            ProfileSignal::Member(sig) => {
                let p = sig.get();
                (
                    p.profile.display_name.clone().unwrap_or_default(),
                    strip_alpha(p.banner_color().to_css_hex()),
                    p.name_color().to_hsla()[0] as u32,
                )
            }
        };
        displayname_val.set(dn);
        bannercolor_val.set(bc);
        namecolor_val.set(nc);
    });

    let fallback_color = Memo::new(move |_| get_color(user_id.as_ref()));

    view! {
        <div class="flex flex-col h-full">
            <div class="relative flex flex-row w-full gap-5 px-5">
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
                <div class="w-56 shrink-0 border border-(--tile-border-color) flex flex-col rounded-ui overflow-hidden">
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
                            <div class="absolute inset-0 bg-(--overlay-bg-color) opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center pointer-events-none text-bright [will-change:opacity]">
                                <Icon icon=PAINT_BRUSH size="24px" weight=IconWeight::Light />
                            </div>
                        </div>

                        <div
                            class="absolute group cursor-pointer [transform:translateZ(0)]"
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
                                class="absolute rounded-full bg-(--overlay-bg-color) opacity-0 group-hover:opacity-100 transition-opacity flex items-center justify-center pointer-events-none text-bright [will-change:opacity]"
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
                                    style=move || {
                                        format!(
                                            "font-size: 16px; color: hsl({}deg, 90%, 70%);",
                                            namecolor_val.get(),
                                        )
                                    }
                                >
                                    {move || displayname_val.get()}
                                </span>
                            </div>
                            <button
                                class="ml-2 text-muted hover:text-normal cursor-pointer"
                                on:click=move |_| {
                                    if let Some(el) = namecolor_ref.get() {
                                        let _ = el.focus();
                                    }
                                }
                            >
                                <Icon icon=PAINT_BRUSH size="18px" weight=IconWeight::Thin />
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
                                class="flex-1 min-w-0 px-2 py-1 bg-transparent border border-(--tile-border-color) rounded-ui text-sm text-normal outline-none focus:border-(--accent-color)"
                                prop:value=move || displayname_val.get()
                                on:input=move |ev| displayname_val.set(event_target_value(&ev))
                            />
                            <button
                                class="shrink-0 px-3 py-1 text-sm bg-(--accent-color) text-white rounded-ui hover:brightness-110 cursor-pointer"
                                on:click=move |_| {
                                    if let Err(e) = save_displayname(
                                        &displayname_val.get_untracked(),
                                        selected_room.get_untracked(),
                                    ) {
                                        log::error!("Failed to save displayname: {e}");
                                    }
                                }
                            >
                                "Save"
                            </button>
                            <button
                                class="shrink-0 px-3 py-1 text-sm border border-(--tile-border-color) text-muted rounded-ui hover:text-normal cursor-pointer"
                                on:click=move |_| {
                                    let localpart = uid_reset
                                        .split(':')
                                        .next()
                                        .and_then(|s| s.strip_prefix('@'))
                                        .unwrap_or(&uid_reset)
                                        .to_string();
                                    let name = localpart
                                        .chars()
                                        .next()
                                        .map(|c| c.to_uppercase().collect::<String>())
                                        .unwrap_or_default()
                                        + &localpart.chars().skip(1).collect::<String>();
                                    displayname_val.set(name);
                                }
                            >
                                "Reset"
                            </button>
                        </div>
                    </div>

                    <div class="flex flex-col gap-1">
                        <label class="text-xs text-muted font-medium">"Bannercolor"</label>
                        <div class="flex gap-2 items-center">
                            <div
                                class="relative shrink-0 w-5 h-5 rounded-full border border-(--tile-border-color) overflow-hidden cursor-pointer"
                                style=move || {
                                    format!("background-color: {};", bannercolor_val.get())
                                }
                            >
                                <input
                                    type="color"
                                    class="absolute inset-0 w-full h-full opacity-0 cursor-pointer"
                                    prop:value=move || bannercolor_val.get()
                                    on:input=move |ev| bannercolor_val.set(event_target_value(&ev))
                                />
                            </div>
                            <input
                                node_ref=bannercolor_ref
                                type="text"
                                class="flex-1 min-w-0 px-2 py-1 bg-transparent border border-(--tile-border-color) rounded-ui text-sm text-normal outline-none focus:border-(--accent-color)"
                                prop:value=move || bannercolor_val.get()
                                on:input=move |ev| bannercolor_val.set(event_target_value(&ev))
                            />
                            <button
                                class="shrink-0 px-3 py-1 text-sm bg-(--accent-color) text-white rounded-ui hover:brightness-110 cursor-pointer"
                                on:click=move |_| {
                                    if let Err(e) = save_banner_color(&bannercolor_val.get()) {
                                        log::error!("Failed to save banner color: {e}");
                                    }
                                }
                            >
                                "Save"
                            </button>
                            <button
                                class="shrink-0 px-3 py-1 text-sm border border-(--tile-border-color) text-muted rounded-ui hover:text-normal cursor-pointer"
                                on:click=move |_| {
                                    bannercolor_val.set(fallback_color.get().to_css_hsl());
                                }
                            >
                                "Reset"
                            </button>
                        </div>
                    </div>

                    <div class="flex flex-col gap-1">
                        <label class="text-xs text-muted font-medium">"Namecolor"</label>
                        <div class="flex gap-2 items-center">
                            <div
                                node_ref=hue_bar_ref
                                class="relative h-5 flex-1 rounded-full cursor-crosshair select-none touch-none"
                                style="background: linear-gradient(to right in hsl increasing hue, hsl(0,90%,70%), hsl(359,90%,70%))"
                                on:pointerdown=move |ev| {
                                    ev.prevent_default();
                                    if let Some(el) = hue_bar_ref.get() {
                                        let _ = el.set_pointer_capture(ev.pointer_id());
                                        let rect = el.get_bounding_client_rect();
                                        let x = (ev.client_x() as f64 - rect.left())
                                            .clamp(0.0, rect.width());
                                        namecolor_val
                                            .set((x / rect.width() * 360.0).round() as u32);
                                    }
                                }
                                on:pointermove=move |ev| {
                                    if ev.buttons() == 0 {
                                        return;
                                    }
                                    if let Some(el) = hue_bar_ref.get() {
                                        let rect = el.get_bounding_client_rect();
                                        let x = (ev.client_x() as f64 - rect.left())
                                            .clamp(0.0, rect.width());
                                        namecolor_val
                                            .set((x / rect.width() * 360.0).round() as u32);
                                    }
                                }
                            >
                                <div
                                    class="absolute top-1/2 -translate-y-1/2 w-4 h-4 rounded-full border-2 border-white pointer-events-none"
                                    style=move || {
                                        let hue: f64 = namecolor_val.get() as f64;
                                        format!(
                                            "left: clamp(0px, calc({}% - 8px), calc(100% - 16px)); background-color: hsl({}deg, 90%, 70%); box-shadow: 0 0 0 1.5px black, 0 2px 6px rgba(0,0,0,0.5);",
                                            hue / 360.0 * 100.0,
                                            hue as u32,
                                        )
                                    }
                                />
                            </div>
                            <input
                                node_ref=namecolor_ref
                                type="number"
                                min="0"
                                max="360"
                                class="w-16 shrink-0 px-2 py-1 bg-transparent border border-(--tile-border-color) rounded-ui text-sm text-normal outline-none focus:border-(--accent-color)"
                                prop:value=move || namecolor_val.get()
                                on:input=move |ev| {
                                    namecolor_val.set(event_target_value(&ev).parse().unwrap_or(0))
                                }
                            />
                            <button
                                class="shrink-0 px-3 py-1 text-sm bg-(--accent-color) text-white rounded-ui hover:brightness-110 cursor-pointer"
                                on:click=move |_| {
                                    if let Err(e) = save_name_color(
                                        &format!("hsl({} 90% 70%)", namecolor_val.get()),
                                    ) {
                                        log::error!("Failed to save name color: {e}");
                                    }
                                }
                            >
                                "Save"
                            </button>
                            <button
                                class="shrink-0 px-3 py-1 text-sm border border-(--tile-border-color) text-muted rounded-ui hover:text-normal cursor-pointer"
                                on:click=move |_| {
                                    namecolor_val.set(fallback_color.get().to_hsla()[0] as u32);
                                }
                            >
                                "Reset"
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
    .into_any()
}
