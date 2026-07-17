use phosphor_leptos::{
    BUG, CARET_DOWN, CARET_RIGHT, Icon, IconData, IconWeight, PLUS, QUESTION_MARK, SPEAKER_HIGH,
};
use ruma::{OwnedRoomId, OwnedUserId};
use shared::{
    profile::MemberProfile,
    sidebar::{ServerRoomNode, UserDevice},
};
use wasm_bindgen::JsCast;
use web_sys::Element;

use crate::{
    components::{
        AudioMenu, DeafenMenu, FloatingTile, MuteMenu,
        logo::Logo,
        overlays::space_search::{SpaceSearchState, open_space_search},
        presence::PresenceBadge,
        settings::SettingsIcon,
        user_profile::{MemberProfileExt, RoomNodeExt},
    },
    state::{AppState, CurrentSection, MainView, MediaCache, ProfileStore},
    tauri_functions::open_log_window,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::sidebar::RoomNode;
use web_sys::HtmlButtonElement;

fn render_full_room(node: RoomNode, other_user_id: StoredValue<Option<OwnedUserId>>) -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    let room_id = StoredValue::new(node.room_id());

    let is_active = Memo::new(move |_| state.active_room_id() == Some(room_id.get_value()));

    let notifications = move || {
        state
            .notification_counts
            .get()
            .get(&room_id.get_value())
            .cloned()
            .unwrap_or_default()
    };

    let call_icon = move || {
        let members_in_call = state.get_call_members(&room_id.get_value()).get();
        if !members_in_call.is_empty() {
            let user_in_call = members_in_call
                .iter()
                .any(|d| d.user_id == state.user_id.get().expect("user_id is not set"));

            Some(
                view! {
                    <div class="pl-2 h-full items-center flex">
                        <Icon
                            icon=SPEAKER_HIGH
                            weight=IconWeight::Fill
                            size="16px"
                            color=if user_in_call {
                                "var(--online-color)"
                            } else {
                                "var(--offline-color)"
                            }
                        />
                    </div>
                }
                .into_any(),
            )
        } else {
            None
        }
    };

    let store_clone = store.clone();
    let node_clone = node.clone();
    view! {
        <div
            class="flex flex-row flex-grow items-center p-1 rounded-ui cursor-pointer hover:text-normal border hover:border-(--tile-border-color) group"
            class=("bg-(--ui-solid-hover-bg)", move || is_active.get())
            class=("border-(--tile-border-color)", move || is_active.get())
            class=("border-transparent", move || !is_active.get())
            class=("text-normal", move || is_active.get())
            class=("text-dim", move || !is_active.get())
            on:click=move |_| {
                state.set_active_room_with_id(Some(room_id.get_value()), MainView::Chat)
            }
        >
            {move || {
                if let Some(user_id) = &other_user_id.get_value() {
                    let profile = store_clone.get_member_profile(&room_id.get_value(), user_id);
                    let presence = store_clone.get_presence(user_id);

                    view! {
                        <PresenceBadge presence=presence>
                            {move || profile.get().render_icon("32px", cache)}
                        </PresenceBadge>
                    }
                        .into_any()
                } else {
                    node.render_url_icon("32px", cache)
                }
            }}
            <span class="inline-block align-center pl-2">
                {move || {
                    if let Some(user_id) = &other_user_id.get_value() {
                        let profile = store.get_member_profile(&room_id.get_value(), user_id);
                        profile.get().get_name()
                    } else {
                        node_clone.name()
                    }
                }}
            </span>
            {call_icon}
            <button
                class="opacity-0 group-hover:opacity-100 text-dim hover:text-normal cursor-pointer flex items-center justify-center mr-1"
                on:click=move |e| {
                    e.stop_propagation();
                    state.set_active_room_with_id(Some(room_id.get_value()), MainView::Info);
                }
            >
                <Icon icon=QUESTION_MARK size="14px" />
            </button>
            {move || {
                let notifications = notifications().notification_count;
                if notifications > 0 {
                    view! {
                        <div class="ml-auto bg-(--mention-color) text-(--mention-text-color) text-xs font-bold px-1.5 py-0.5 rounded-full">
                            {notifications}
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}
        </div>
    }.into_any()
}

#[component]
pub fn IndicatorPill(
    #[prop(into)] is_active: Signal<bool>,
    #[prop(into)] has_notifications: Signal<bool>,
) -> impl IntoView {
    view! {
        <div
            class="absolute left-1 w-1 bg-white rounded-full top-1/2 -translate-y-1/2 transition-all duration-200 ease-in-out"

            class=("h-10", move || is_active.get())
            class=("h-3", move || !is_active.get() && has_notifications.get())
            class=("h-0", move || !is_active.get() && !has_notifications.get())
            class=("group-hover:h-[25px]", move || !is_active.get())

            class=("opacity-100", move || is_active.get() || has_notifications.get())
            class=("opacity-0", move || !is_active.get() && !has_notifications.get())
            class=("group-hover:opacity-100", move || !is_active.get())
        ></div>
    }
}

#[allow(dead_code)]
#[derive(Clone, PartialEq)]
pub enum CutoutBadgeContent {
    Number(u64),
    Text(String),
    Icon(IconData),
}

#[derive(Clone, PartialEq)]
pub struct CutoutBadgeCorner {
    pub fg_color: String,
    pub bg_color: String,
    pub content: CutoutBadgeContent,
}

#[component]
pub fn CutoutBadge(
    #[prop(into, optional)] top_right: Signal<Option<CutoutBadgeCorner>>,
    #[prop(into, optional)] top_left: Signal<Option<CutoutBadgeCorner>>,
    #[prop(into, optional)] bottom_right: Signal<Option<CutoutBadgeCorner>>,
    #[prop(into, optional)] bottom_left: Signal<Option<CutoutBadgeCorner>>,
    children: Children,
    #[prop(into, optional)] class: Signal<String>,
) -> impl IntoView {
    let render_content = |content: CutoutBadgeContent| match content {
        CutoutBadgeContent::Number(n) => view! { {n} }.into_any(),
        CutoutBadgeContent::Text(t) => view! { {t} }.into_any(),
        CutoutBadgeContent::Icon(i) => view! { <Icon icon=i weight=IconWeight::Fill /> }.into_any(),
    };

    let mask_style = move || {
        let mut masks = Vec::new();

        if top_right.get().is_some() {
            masks.push("radial-gradient(circle 11px at calc(100% - 8px) 8px, transparent 11px, black 11.5px)");
        }
        if bottom_right.get().is_some() {
            masks.push("radial-gradient(circle 11px at calc(100% - 8px) calc(100% - 8px), transparent 11px, black 11.5px)");
        }
        if bottom_left.get().is_some() {
            masks.push("radial-gradient(circle 11px at 8px calc(100% - 8px), transparent 11px, black 11.5px)");
        }
        if top_left.get().is_some() {
            masks.push("radial-gradient(circle 11px at 8px 8px, transparent 11px, black 11.5px)");
        }

        if !masks.is_empty() {
            let joined = masks.join(", ");
            format!(
                "-webkit-mask-image: {0}; -webkit-mask-composite: source-in; mask-image: {0}; mask-composite: intersect;",
                joined
            )
        } else {
            String::new()
        }
    };

    let render_corner = move |corner_signal: Signal<Option<CutoutBadgeCorner>>,
                              pos_classes: &'static str| {
        move || {
            corner_signal.get().map(|c| {
                    view! {
                        <div
                            class=format!(
                                "absolute {pos_classes} flex items-center justify-center text-[12px] font-extrabold w-4 h-4 rounded-full",
                            )
                            style=format!(
                                "background-color: {}; color: {};",
                                c.bg_color,
                                c.fg_color,
                            )
                        >
                            {render_content(c.content.clone())}
                        </div>
                    }
                })
        }
    };

    view! {
        <div class="relative w-fit h-fit">
            <div class=move || format!("w-full h-full {}", class.get()) style=mask_style>
                {children()}
            </div>

            {render_corner(top_right, "-top-0 -right-0")}
            {render_corner(bottom_right, "-bottom-0 -right-0")}
            {render_corner(bottom_left, "-bottom-0 -left-0")}
            {render_corner(top_left, "-top-0 -left-0")}
        </div>
    }
}

fn render_room_preview(room: RoomNode, members: Option<Vec<UserDevice>>) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    let click_id = room.room_id();
    let clone = click_id.clone();

    let is_active = Memo::new(move |_| state.active_room_id() == Some(click_id.clone()));
    let room_id_for_count = room.room_id();

    let notifications = Memo::new(move |_| {
        state
            .notification_counts
            .get()
            .get(&room_id_for_count)
            .cloned()
            .unwrap_or_default()
    });

    let br_corner = move || {
        if notifications.get().has_notifications() {
            Some(CutoutBadgeCorner {
                fg_color: "var(--mention-text-color)".to_string(),
                bg_color: "var(--mention-color)".to_string(),
                content: CutoutBadgeContent::Number(notifications.get().notification_count),
            })
        } else {
            None
        }
    };

    let tr_corner = move || {
        if let Some(members) = &members {
            let user_ids_in_calls = members
                .iter()
                .map(|d| d.user_id.clone())
                .collect::<Vec<_>>();

            if !user_ids_in_calls.is_empty() {
                let user_in_call =
                    user_ids_in_calls.contains(&state.user_id.get().expect("user_id is not set"));
                Some(CutoutBadgeCorner {
                    fg_color: "white".to_string(),
                    bg_color: if user_in_call {
                        "var(--online-color)".to_string()
                    } else {
                        "var(--offline-color)".to_string()
                    },
                    content: CutoutBadgeContent::Icon(SPEAKER_HIGH),
                })
            } else {
                None
            }
        } else {
            None
        }
    };

    let room_clone = room.clone();
    view! {
        <div class="h-2"></div>
        <div
            class="relative flex items-center justify-center group w-full cursor-pointer"
            on:click=move |_| {
                let section = if room_clone.is_dm() {
                    CurrentSection::Dms
                } else {
                    CurrentSection::Single
                };
                state.set_active_section(section);
                state.set_active_room_with_id(Some(clone.clone()), MainView::Chat);
            }
        >
            <IndicatorPill
                is_active=is_active
                has_notifications=move || { notifications.get().has_notifications() }
            />

            <CutoutBadge bottom_right=br_corner top_right=tr_corner class="justify-center flex">
                <div class="avatar-circle w-10 h-10 rounded-full" style:justify-content="center">
                    {move || {
                        if let RoomNode::Dm(dm) = &room {
                            let profile = store
                                .get_member_profile(&room.room_id(), &dm.other_user_id);
                            profile.get().render_icon("40px", cache)
                        } else {
                            room.render_url_icon("40px", cache)
                        }
                    }}
                </div>
            </CutoutBadge>
        </div>
    }
}

#[component]
pub fn ServerIcon(server: ServerRoomNode) -> impl IntoView {
    let state: AppState = expect_context();
    let cache: MediaCache = expect_context();

    let server_id = StoredValue::new(server.room_id());

    let is_active = Memo::new(move |_| {
        state.active_section.get() == CurrentSection::Server(server_id.get_value())
    });

    let all_children = server.all_children.clone();
    let notifications = Memo::new(move |_| {
        let counts = state.notification_counts.get();
        let server_id = &server_id.get_value();

        let mut count = counts.get(server_id).cloned().unwrap_or_default();

        for room_id in &all_children {
            count += counts.get(room_id).cloned().unwrap_or_default();
        }
        count
    });

    let avatar_content = RoomNode::Server(server.clone()).render_url_icon("40px", cache);

    let tr_corner = move || {
        let user_ids_in_calls =
            state.get_call_members_in_rooms(server.all_children.clone().into_iter().collect());

        if !user_ids_in_calls.is_empty() {
            let user_in_call =
                user_ids_in_calls.contains(&state.user_id.get().expect("user_id is not set"));
            Some(CutoutBadgeCorner {
                fg_color: "white".to_string(),
                bg_color: if user_in_call {
                    "var(--online-color)".to_string()
                } else {
                    "var(--offline-color)".to_string()
                },
                content: CutoutBadgeContent::Icon(SPEAKER_HIGH),
            })
        } else {
            None
        }
    };

    let br_corner = move || {
        let highlight_count = notifications.get().highlight_count;
        if highlight_count > 0 {
            Some(CutoutBadgeCorner {
                fg_color: "white".to_string(),
                bg_color: "var(--mention-color)".to_string(),
                content: CutoutBadgeContent::Number(highlight_count),
            })
        } else {
            None
        }
    };

    view! {
        <div class="relative flex items-center justify-center group w-full">
            <IndicatorPill
                is_active=is_active
                has_notifications=move || notifications.get().has_notifications()
            />
            <div class="relative w-10 h-10">
                <CutoutBadge
                    bottom_right=move || br_corner()
                    top_right=move || tr_corner()
                    class="justify-center flex"
                >
                    <div
                        class="server-btn flex items-center justify-center w-10 h-10 text-gray-800 font-semibold rounded-[25%] cursor-pointer transition-colors"
                        class=("bg-[var(--color-icon-selected)]", move || is_active.get())
                        class=("bg-[var(--color-icon-bg)]", move || !is_active.get())
                        class=("hover:bg-[var(--color-icon-hover)]", move || !is_active.get())
                        on:click=move |_| {
                            state.set_active_section(CurrentSection::Server(server_id.get_value()));
                        }
                    >
                        <div class="avatar-circle w-full h-full rounded-[25%] overflow-hidden">
                            {avatar_content}
                        </div>
                    </div>
                </CutoutBadge>
            </div>
        </div>
    }
            .into_any()
}

fn room_should_stay_visible(
    state: &AppState,
    room_id: &OwnedRoomId,
    active_room_id: &Option<OwnedRoomId>,
) -> bool {
    if active_room_id.as_deref() == Some(room_id) {
        return true;
    }

    let has_notifications = state
        .notification_counts
        .get()
        .get(room_id)
        .cloned()
        .unwrap_or_default()
        .has_notifications();

    if has_notifications {
        return true;
    }

    match state.get_room(room_id) {
        Some(RoomNode::Space(space)) => space
            .children
            .iter()
            .any(|child_id| room_should_stay_visible(state, child_id, active_room_id)),
        _ => false,
    }
}

fn render_server_channel(child: RoomNode) -> AnyView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    if let RoomNode::Space(space) = &child {
        let child_ids = StoredValue::new(space.children.clone());
        let name = space.name();
        let is_open = RwSignal::new(true);

        let has_visible_exception = Memo::new(move |_| {
            let active_id = state.active_room_id();
            child_ids
                .get_value()
                .iter()
                .any(|id| room_should_stay_visible(&state, id, &active_id))
        });

        return view! {
            <div class="flex flex-col w-full">
                <div
                    class="flex flex-row flex-grow items-center gap-1 p-1 rounded-ui cursor-pointer text-dim hover:text-normal"
                    on:click=move |_| is_open.update(|open| *open = !*open)
                >
                    <span>{name}</span>
                    {move || {
                        let icon = if is_open.get() { CARET_DOWN } else { CARET_RIGHT };
                        view! { <Icon icon=icon size="14px" /> }
                    }}
                </div>
                {move || {
                    if !is_open.get() && !has_visible_exception.get() {
                        return ().into_any();
                    }

                    view! {
                        <div class="flex flex-col gap-(--gap) pl-3 ml-[9px] border-l border-(--tile-border-color)">
                            <For
                                each=move || child_ids.get_value()
                                key=|child_id| child_id.clone()
                                children=move |child_id| {
                                    view! {
                                        {move || {
                                            let visible = is_open.get()
                                                || room_should_stay_visible(
                                                    &state,
                                                    &child_id,
                                                    &state.active_room_id(),
                                                );
                                            if !visible {
                                                return ().into_any();
                                            }
                                            match state.get_room(&child_id) {
                                                Some(child) => render_room(child),
                                                None => ().into_any(),
                                            }
                                        }}
                                    }
                                        .into_any()
                                }
                            />
                        </div>
                    }
                        .into_any()
                }}
            </div>
        }
        .into_any();
    }

    let room_id = StoredValue::new(child.room_id());
    let is_active = Memo::new(move |_| state.active_room_id() == Some(room_id.get_value()));

    let highlight_count = move || {
        let counts = state
            .notification_counts
            .get()
            .get(&room_id.get_value())
            .cloned()
            .unwrap_or_default();
        counts.highlight_count
    };
    let notification_count = move || {
        let counts = state
            .notification_counts
            .get()
            .get(&room_id.get_value())
            .cloned()
            .unwrap_or_default();
        counts.notification_count
    };

    let has_notifications = Memo::new(move |_| {
        let counts = state
            .notification_counts
            .get()
            .get(&room_id.get_value())
            .cloned()
            .unwrap_or_default();
        counts.notification_count > 0 || counts.highlight_count > 0
    });

    let notification_gradient = move || {
        if !has_notifications.get() {
            return String::new();
        }

        "linear-gradient(in srgb to right, oklch(from var(--mention-color) l c h / 0.15), oklch(from var(--accent-color) l c h / 0) 100%)".to_string()
    };

    let participants = Memo::new(move |_| state.get_call_members(&room_id.get_value()).get());
    let name = child.name();

    let child = StoredValue::new(child);

    let call_preview = move || {
        let participants = participants.get();

        if participants.is_empty() {
            return ().into_any();
        }

        let views = participants.iter().map(|device| {
                    let profile = store.get_member_profile(&child.get_value().room_id(), &device.user_id);
                    let clone = profile.clone();

                    view! {
                        <div class="hover:border-(--tile-border-color) border border-transparent rounded-[10px] p-1 flex items-center gap-2 flex flex-grow cursor-pointer">
                            {move || profile.get().render_icon("22px", cache)}
                            {move || clone.get().render_name_popup("14px")}
                        </div>
                    }
                });

        view! { <div class="flex pl-8 flex-col gap-1">{views.collect_view()}</div> }.into_any()
    };

    view! {
        <div class="group relative flex flex-row w-full cursor-pointer">
            <div
                class="flex flex-row flex-grow items-center p-1 rounded-ui cursor-pointer border hover:border-(--tile-border-color)"
                class=("hover:text-normal", move || !child.get_value().is_unjoined())
                class=("hover:text-dim", move || child.get_value().is_unjoined())
                class=(
                    "text-dim",
                    move || {
                        !is_active.get() && !has_notifications.get()
                            && !child.get_value().is_unjoined()
                    },
                )
                class=(
                    "text-muted",
                    move || {
                        !is_active.get() && !has_notifications.get()
                            && child.get_value().is_unjoined()
                    },
                )
                class=("text-normal", move || { has_notifications.get() || is_active.get() })
                class=("bg-(--ui-solid-hover-bg)", move || is_active.get())
                class=("border-transparent", move || !is_active.get())
                class=("border-(--tile-border-color)", move || is_active.get())
                style:background-image=notification_gradient
                on:click=move |_| {
                    let view = if child.get_value().is_unjoined() {
                        MainView::Info
                    } else {
                        MainView::Chat
                    };
                    state.set_active_room_with_id(Some(room_id.get_value()), view)
                }
            >
                <div class=(
                    "text-(--online-color)",
                    move || !participants.get().is_empty(),
                )>{child.get_value().render_simple_icon("20px")}</div>
                <div class="w-1"></div>
                {name}
                <div class="w-1"></div>
                <button
                    class="opacity-0 group-hover:opacity-100 text-dim hover:text-normal cursor-pointer flex items-center justify-center mr-1"
                    on:click=move |e| {
                        e.stop_propagation();
                        state.set_active_room_with_id(Some(room_id.get_value()), MainView::Info);
                    }
                >
                    <Icon icon=QUESTION_MARK size="14px" />
                </button>
                <div class="flex flex-1" />
                {move || {
                    if highlight_count() > 0 {
                        view! {
                            <div class="text-center bg-(--mention-color) w-5 h-5 text-(--mention-text-color) text-xs font-bold px-1.5 py-0.5 rounded-full">
                                {highlight_count}
                            </div>
                        }
                            .into_any()
                    } else if notification_count() > 0 {
                        view! { <div class="bg-(--text-color) w-2 h-2 rounded-full mr-1.5" /> }
                            .into_any()
                    } else {
                        view! { <div></div> }.into_any()
                    }
                }}
            </div>
        </div>
        {call_preview}
    }
    .into_any()
}

fn render_room(room: RoomNode) -> AnyView {
    match &room {
        RoomNode::Dm(dm) => render_full_room(
            room.clone(),
            StoredValue::new(Some(dm.other_user_id.clone())),
        ),
        RoomNode::Single(_) => render_full_room(room, StoredValue::new(None)),
        _ => render_server_channel(room),
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let state: AppState = expect_context();
    let space_search_state: SpaceSearchState = expect_context();

    let dragged_server_id: RwSignal<Option<OwnedRoomId>> = RwSignal::new(None);

    let dms_btn = NodeRef::new();
    let rooms_btn = NodeRef::new();

    let pill_left = RwSignal::new(0);
    let pill_width = RwSignal::new(0);
    let has_measured = RwSignal::new(false);

    Effect::new(move |_| {
        let is_rooms = state.active_section.get() == CurrentSection::Single;
        let target_node: Option<HtmlButtonElement> = if is_rooms {
            rooms_btn.get()
        } else {
            dms_btn.get()
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

    let Ok(img) = web_sys::HtmlImageElement::new() else {
        return view! { <div class="item p-4">"Error initializing drag image"</div> }.into_any();
    };
    img.set_src("data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7");

    let active_rooms = Memo::new(move |_| {
        let notifications = state.notification_counts.get();

        state
            .dm_list
            .get()
            .0
            .into_iter()
            .chain(state.single_room_list.get().0)
            .filter_map(|dm_id| {
                let has_notifications = notifications
                    .get(&dm_id)
                    .cloned()
                    .unwrap_or_default()
                    .notification_count
                    > 0;

                let call_members = state.get_call_members(&dm_id).get();

                if !has_notifications && call_members.is_empty() {
                    None
                } else {
                    let members = (!call_members.is_empty()).then_some(call_members);
                    let room = state.get_room(&dm_id)?;
                    Some((room, members))
                }
            })
            .collect::<Vec<_>>()
    });

    let rooms_to_show = Memo::new(move |_| match state.active_section.get() {
        CurrentSection::Dms => state.get_dms(),
        CurrentSection::Single => state.get_single_rooms(),
        CurrentSection::Server(id) => state.get_server_rooms(&id),
    });

    view! {
        <div class="flex h-full gap-[var(--gap)] select-none z-10">
            // Empty image used for drag ghost to avoid default semi-transparent preview
            <img
                id="drag-ghost"
                src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
                style="position: absolute; top: -1000px; left: -1000px; opacity: 0;"
            />

            <FloatingTile>
                <div class="servers w-16 flex flex-col items-center pt-3 pb-3 overflow-y-auto flex-grow min-h-0">
                    <div class="relative flex items-center justify-center group w-full">
                        <IndicatorPill
                            is_active=Memo::new(move |_| state.active_section.get().is_not_server())
                            has_notifications=Memo::new(move |_| false)
                        />

                        <div
                            class="server-btn flex items-center justify-center w-10 h-10 rounded-[25%] cursor-pointer transition-colors border"
                            class=(
                                "border-(--accent-color)",
                                move || state.active_section.get().is_not_server(),
                            )
                            class=(
                                "border-(--tile-border-color)",
                                move || !state.active_section.get().is_not_server(),
                            )
                            class=(
                                "bg-(--ui-solid-hover-bg)",
                                move || state.active_section.get().is_not_server(),
                            )
                            class=(
                                "ui-solid-bg",
                                move || !state.active_section.get().is_not_server(),
                            )

                            on:click=move |_| {
                                let section = if state.breadcrums.get_untracked().dms_last {
                                    CurrentSection::Dms
                                } else {
                                    CurrentSection::Single
                                };
                                state.set_active_section(section);
                            }
                        >
                            <Logo size="85%" inherit_color=false />
                        </div>
                    </div>

                    <For
                        each=move || active_rooms.get()
                        key=|(dm, _)| dm.room_id()
                        children=move |(room, members)| { render_room_preview(room, members) }
                    />

                    <div class="w-8 h-[2px] bg-(--tile-border-color) rounded-full my-2 gap-[1px]"></div>
                    <For
                        each=move || state.server_list.get().0
                        key=|server_id| server_id.clone()
                        children=move |server_id| {
                            let drag_id = server_id.clone();
                            let drop_id = server_id.clone();
                            let Some(server) = state
                                .get_room(&server_id)
                                .and_then(|r| r.as_server()) else {
                                return ().into_any();
                            };

                            view! {
                                <div
                                    draggable="true"
                                    class="w-full flex flex-col items-center cursor-grab active:cursor-grabbing"
                                    on:dragstart={
                                        let img = img.clone();
                                        move |e| {
                                            if let Some(data_transfer) = e.data_transfer() {
                                                let _ = data_transfer
                                                    .set_data("text/plain", drag_id.as_ref());
                                                data_transfer.set_drag_image(&img, 0, 0);
                                            }
                                            dragged_server_id.set(Some(drag_id.clone()));
                                        }
                                    }
                                    on:dragover=move |e| {
                                        e.prevent_default();
                                    }
                                    on:dragenter=move |e| {
                                        e.prevent_default();
                                        let Some(source_id) = dragged_server_id.get() else {
                                            return
                                        };
                                        if source_id != drop_id {
                                            state.reorder_servers(&source_id, &drop_id);
                                        }
                                    }
                                    on:dragend=move |_| {
                                        dragged_server_id.set(None);
                                        spawn_local(async move {
                                            let current_servers = state.server_list.get_untracked().0;
                                            state.set_server_order(current_servers);
                                        });
                                    }
                                >
                                    <ServerIcon server=server />
                                    <div class="h-2 pointer-events-none"></div>
                                </div>
                            }
                                .into_any()
                        }
                    />

                    <button
                        class="server-btn flex items-center justify-center w-10 h-10 rounded-[25%] cursor-pointer transition-colors ui-solid-bg hover:bg-(--ui-solid-hover-bg) border border-(--tile-border-color) text-dim hover:text-(--accent-color) mt-1 flex-shrink-0"
                        on:click=move |ev| {
                            let anchor: Element = ev.target().unwrap().unchecked_into();
                            open_space_search(&anchor, space_search_state);
                        }
                    >
                        <Icon icon=PLUS size="55%" weight=IconWeight::Bold />
                    </button>
                </div>
                <div class="relative flex items-center justify-center group w-full border-t border-(--tile-border-color) pt-3 pb-3 flex-shrink-0">
                    <div
                        class="server-btn flex items-center justify-center w-10 h-10 rounded-[25%] cursor-pointer transition-colors ui-solid-bg hover:bg-(--ui-solid-hover-bg) border border-(--tile-border-color) text-(--dim-text-color) hover:text-(--accent-color)"
                        on:click=move |_| open_log_window()
                    >
                        <Icon icon=BUG size="65%" weight=IconWeight::Thin />
                    </div>
                </div>
            </FloatingTile>

            <div class="flex flex-col gap-(--gap) w-70">
                <FloatingTile class="flex-grow">
                    <div class="relative header border-b border-(--tile-border-color) font-bold text-normal p-3 flex flex-row gap-3 w-full">
                        {move || {
                            match state.active_section.get() {
                                CurrentSection::Dms | CurrentSection::Single => {
                                    view! {
                                        <button
                                            node_ref=dms_btn
                                            class="font-medium hover:text-normal cursor-pointer"
                                            class=(
                                                "text-(--accent-color)",
                                                move || state.active_section.get() == CurrentSection::Dms,
                                            )
                                            class=(
                                                "text-dim",
                                                move || state.active_section.get() != CurrentSection::Dms,
                                            )
                                            on:click=move |_| {
                                                state.set_active_section(CurrentSection::Dms);
                                            }
                                        >
                                            "Direct Messages"
                                        </button>
                                        <button
                                            node_ref=rooms_btn
                                            class="font-medium hover:text-normal cursor-pointer"
                                            class=(
                                                "text-(--accent-color)",
                                                move || state.active_section.get() == CurrentSection::Single,
                                            )
                                            class=(
                                                "text-dim",
                                                move || state.active_section.get() != CurrentSection::Single,
                                            )
                                            on:click=move |_| {
                                                state.set_active_section(CurrentSection::Single);
                                            }
                                        >
                                            "Rooms"
                                        </button>
                                        <div class="flex flex-grow"></div>
                                        <div
                                            class="absolute bottom-3 h-[2px] rounded-full bg-(--accent-color)"
                                            class=("transition-all", move || has_measured.get())
                                            class=("duration-100", move || has_measured.get())
                                            class=("ease-in-out", move || has_measured.get())
                                            style=pill_style
                                        />
                                    }
                                        .into_any()
                                }
                                CurrentSection::Server(server_id) => {
                                    view! {
                                        <span>
                                            {move || {
                                                state
                                                    .get_room(&server_id)
                                                    .map(|r| r.name())
                                                    .unwrap_or_default()
                                            }}
                                        </span>
                                    }
                                        .into_any()
                                }
                            }
                        }}
                    </div>

                    <div class="p-(--gap) gap-(--gap) flex flex-col w-full">
                        <For
                            each=move || rooms_to_show.get()
                            key=|room| room.room_id()
                            children=move |room| render_room(room)
                        />
                    </div>
                </FloatingTile>

                <FloatingTile class="h-(--header-height) w-full" style="overflow: visible;">
                    <ProfileCard />
                </FloatingTile>
            </div>
        </div>
    }.into_any()
}

#[component]
pub fn ProfileCard() -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();
    let cache: MediaCache = expect_context();

    let current_profile: RwSignal<Option<MemberProfile>> = RwSignal::new(None);
    let open_audio_menu: RwSignal<Option<AudioMenu>> = RwSignal::new(None);

    let profile_store = store.clone();
    Effect::new(move |_| {
        let room_id = state.active_room_id();
        let Some(user_id) = state.user_id.get() else {
            return;
        };

        if let Some(rid) = room_id {
            let profile = profile_store.get_member_profile(&rid, &user_id).get();
            current_profile.set(Some(profile));
        } else {
            let CurrentSection::Server(rid) = state.active_section.get() else {
                return;
            };
            let profile = profile_store.get_member_profile(&rid, &user_id).get();
            current_profile.set(Some(profile));
        }
    });

    let current_room_profile = move || {
        let profile = current_profile.get();
        let name_profile = profile.clone();

        let Some(profile) = profile else {
            return ().into_any();
        };

        view! {
            <PresenceBadge presence=store
                .get_presence(
                    &profile.user_id(),
                )>{profile.render_icon("30px", cache)}</PresenceBadge>
            {name_profile.render_name_popup("14px")}
        }
        .into_any()
    };

    view! {
        <div class="flex items-center justify-start w-full h-full px-2 gap-2">
            {current_room_profile} <div class="ml-auto flex items-center h-full gap-2">
                <MuteMenu open=open_audio_menu />
                <DeafenMenu open=open_audio_menu />
                <SettingsIcon />
            </div>
        </div>
    }
}
