use phosphor_leptos::{Icon, IconData, IconWeight, HASH, MATRIX_LOGO, SPEAKER_HIGH};
use shared::get_color;

use crate::components::presence::PresenceBadge;
use crate::components::user_profile::MemberProfileMaybeExt;
use crate::components::FloatingTile;
use crate::state::{AppState, ProfileStore};
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::sidebar::{RoomKind, RoomNode};

use crate::components::TextCircle;

#[component]
fn DmDiv(dm: RoomNode) -> impl IntoView {
    let state: AppState = expect_context();
    let members: ProfileStore = expect_context();

    let id = dm.room_id.to_string();
    let name = dm.name.clone().unwrap_or_else(|| "Unnamed".to_string());

    let is_active = Memo::new(move |_| state.active_room_id() == Some(id.clone()));

    let color = get_color(&dm.dm_user_id().unwrap_or_default());

    let user_id = dm.dm_user_id().clone().unwrap_or_default();

    let members = members.clone();
    let presence = members.get_presence(&user_id);

    let failed = RwSignal::new(false);
    let first_char = name.chars().next().unwrap_or('?').to_string();
    let avatar_content = view! {
        <img
            class="avatar-img w-8 h-8 rounded-full object-cover"
            class:hidden=failed
            src=format!("mxc://room/{}", dm.room_id)
            alt=name.clone()
            on:error=move |_| failed.set(true)
            on:load=move |_| failed.set(false)
        />
        <TextCircle
            text=first_char
            color=color
            class="rounded-full w-8 h-8"
            class:hidden=move || !failed.get()
        />
    };

    view! {
        <div class="group flex flex-row w-full cursor-pointer px-3">
            <div class="transition-[width] duration-300 ease-out shrink-0 w-0 group-hover:w-3"></div>
            <div
                class="flex flex-row flex-grow items-center p-1 pl-2 rounded-[10px] cursor-pointer hover:text-bright"
                class=("bg-[var(--color-item-selected)]", move || is_active.get())
                class=("text-bright", move || is_active.get())
                class=("hover:bg-[var(--color-item-hover)]", move || !is_active.get())
                class=("text-dim", move || !is_active.get())
            >
                <PresenceBadge presence=presence>{avatar_content}</PresenceBadge>
                <span class="inline-block align-center pl-2">{name}</span>
                {if dm.notification_count > 0 {
                    view! {
                        <div class="ml-auto bg-[var(--mention-color)] text-white text-xs font-bold px-1.5 py-0.5 rounded-full">
                            {dm.notification_count}
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }}
            </div>
        </div>
    }
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
#[derive(Clone)]
pub enum CutoutBadgeContent {
    Number(u64),
    Text(String),
    Icon(IconData),
}

#[derive(Clone)]
pub struct CutoutBadgeCorner {
    pub fg_color: String,
    pub bg_color: String,
    pub content: CutoutBadgeContent,
}

#[component]
pub fn CutoutBadge(
    #[prop(into, default = None)] top_right: Option<CutoutBadgeCorner>,
    #[prop(into, default = None)] top_left: Option<CutoutBadgeCorner>,
    #[prop(into, default = None)] bottom_right: Option<CutoutBadgeCorner>,
    #[prop(into, default = None)] bottom_left: Option<CutoutBadgeCorner>,
    children: Children,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    // Helper to render the inner content based on the enum variant
    let render_content = |content: CutoutBadgeContent| match content {
        CutoutBadgeContent::Number(n) => view! { {n} }.into_any(),
        CutoutBadgeContent::Text(t) => view! { {t} }.into_any(),
        CutoutBadgeContent::Icon(i) => view! { <Icon icon=i weight=IconWeight::Fill /> }.into_any(),
    };

    let mut masks = Vec::new();
    let mut badge_views = Vec::new();

    // Closure to process each corner and populate our mask and view vectors
    let mut process_corner = |corner: Option<CutoutBadgeCorner>,
                              mask_pos: &str,
                              badge_pos_classes: &str| {
        if let Some(c) = corner {
            masks.push(format!(
                "radial-gradient(circle 11px at {}, transparent 11px, black 11.5px)",
                mask_pos
            ));

            badge_views.push(view! {
                <div
                    class=format!(
                        "absolute {badge_pos_classes} flex items-center justify-center text-[12px] font-extrabold w-4 h-4 rounded-full",
                    )
                    style=format!("background-color: {}; color: {};", c.bg_color, c.fg_color)
                >
                    {render_content(c.content)}
                </div>
            });
        }
    };

    process_corner(top_right, "calc(100% - 8px) 8px", "-top-0 -right-0");
    process_corner(
        bottom_right,
        "calc(100% - 8px) calc(100% - 8px)",
        "-bottom-0 -right-0",
    );
    process_corner(bottom_left, "8px calc(100% - 8px)", "-bottom-0 -left-0");
    process_corner(top_left, "8px 8px", "-top-0 -left-0");

    // Construct the composite mask style
    let mask_style = if !masks.is_empty() {
        let joined_masks = masks.join(", ");
        format!(
            "-webkit-mask-image: {joined_masks}; -webkit-mask-composite: source-in; mask-image: {joined_masks}; mask-composite: intersect;"
        )
    } else {
        String::new()
    };

    view! {
        <div class="relative w-fit h-fit">
            <div class=format!("w-full h-full {class}") style=mask_style>
                {children()}
            </div>

            {badge_views}
        </div>
    }
}

#[component]
pub fn ServerIcon(server_id: String) -> impl IntoView {
    let state = expect_context::<AppState>();

    let server_id_for_lookup = server_id.clone();
    let server_id_for_active = server_id.clone();
    let server_id_for_click = server_id.clone();

    let server = Memo::new(move |_| {
        state.sidebar_state.with(|state| {
            state
                .servers
                .iter()
                .find(|srv| srv.room_id == server_id_for_lookup)
                .cloned()
        })
    });

    let is_active =
        Memo::new(move |_| state.active_server_id.get() == Some(server_id_for_active.clone()));
    let has_notifications = Memo::new(move |_| {
        server
            .get()
            .map(|server| server.notification_count > 0)
            .unwrap_or(false)
    });

    let content = move || {
        let server_id_for_click = server_id_for_click.clone();

        let Some(server) = server.get() else {
            return view! { <div class="relative w-10 h-10"></div> }.into_any();
        };

        let name = server.name.clone().unwrap_or("?".to_string());
        let color = get_color(&server_id);

        let br_corner = if server.highlight_count > 0 {
            Some(CutoutBadgeCorner {
                fg_color: "white".to_string(),
                bg_color: "var(--mention-color)".to_string(),
                content: CutoutBadgeContent::Number(server.highlight_count),
            })
        } else {
            None
        };
        let tr_corner = if let RoomKind::Space {
            user_ids_in_calls, ..
        } = &server.kind
        {
            if !user_ids_in_calls.is_empty() {
                let user_in_call = user_ids_in_calls.contains(&state.user_id.get());
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
        };

        let failed = RwSignal::new(false);
        let first_char = name.chars().next().unwrap_or('?').to_string();
        let avatar_content = view! {
            <img
                class="avatar-img object-cover w-full h-full"
                draggable="false"
                class:hidden=failed
                src=format!("mxc://room/{}", server_id)
                alt=name.clone()
                on:error=move |_| failed.set(true)
                on:load=move |_| failed.set(false)
            />
            <TextCircle
                text=first_char
                color=color
                class="rounded-[25%] w-full h-full select-none"
                class:hidden=move || !failed.get()
            />
        };

        view! {
            <div class="relative w-10 h-10">
                <CutoutBadge bottom_right=br_corner top_right=tr_corner class="justify-center flex">
                    <div
                        class="server-btn flex items-center justify-center w-10 h-10 text-gray-800 font-semibold rounded-[25%] cursor-pointer transition-colors"
                        class=("bg-[var(--color-icon-selected)]", move || is_active.get())
                        class=("bg-[var(--color-icon-bg)]", move || !is_active.get())
                        class=("hover:bg-[var(--color-icon-hover)]", move || !is_active.get())
                        on:click=move |_| {
                            state.set_active_server_id(Some(server_id_for_click.clone()))
                        }
                    >
                        <div class="avatar-circle w-full h-full rounded-[25%] overflow-hidden">
                            {avatar_content}
                        </div>
                    </div>
                </CutoutBadge>
            </div>
        }
            .into_any()
    };

    view! {
        <div class="relative flex items-center justify-center group w-full">
            <IndicatorPill is_active=is_active has_notifications=has_notifications />
            {content}
        </div>
    }
}

pub fn render_server_channel(child: RoomNode) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let channel_icon = match child.kind {
        RoomKind::Dm { .. } => HASH,
        RoomKind::TextChannel { .. } => HASH,
        RoomKind::VoiceChannel { .. } => SPEAKER_HIGH,
        RoomKind::Space { .. } => MATRIX_LOGO,
    };

    let click_id = child.room_id.to_string();
    let check_id = child.room_id.to_string();
    let is_active = Memo::new(move |_| state.active_room_id() == Some(check_id.clone()));
    let has_notifications = child.notification_count > 0;

    let call_empty = if let RoomKind::VoiceChannel { participants, .. } = &child.kind {
        participants.is_empty()
    } else {
        true
    };

    let call_preview = if let RoomKind::VoiceChannel { participants, .. } = &child.kind {
        let views = participants.keys().map(|user_id| {
            let profile = store.get_member_profile(&child.room_id, user_id);
            let clone = profile.clone();

            view! {
                <div class="hover:bg-(--color-item-hover) rounded-[10px] p-1 flex items-center gap-2 flex flex-grow cursor-pointer">
                    {move || profile.get().render_icon(22)} {move || clone.get().render_name(14)}
                </div>
            }
        });

        Some(view! { <div class="flex pl-8 flex-col gap-1">{views.collect_view()}</div> })
    } else {
        None
    };

    view! {
        <div class="group relative flex flex-row w-full cursor-pointer">

            {move || {
                has_notifications
                    .then(|| {
                        view! {
                            <div class="absolute top-1/2 -translate-y-1/2 -left-1 group-hover:left-1.5 transition-[left] duration-300 ease-out w-2 h-2 bg-[var(--bright-text-color)] rounded-full z-10 pointer-events-none"></div>
                        }
                    })
            }}
            <div class="transition-[width] duration-300 ease-out shrink-0 w-2 group-hover:w-5"></div>

            <div
                class="flex flex-row flex-grow items-center p-1 rounded-[10px] cursor-pointer transition-colors hover:text-bright"
                class=("hover:bg-[color:var(--color-item-hover)]", move || !is_active.get())
                class=("text-dim", move || !is_active.get() && !has_notifications)
                class=(
                    "text-bright",
                    move || { !is_active.get() && has_notifications || is_active.get() },
                )
                class=("bg-[color:var(--color-item-selected)]", move || is_active.get())
                on:click=move |_| { state.set_active_room_with_id(Some(click_id.clone())) }
            >
                <Icon
                    icon=channel_icon
                    size="20px"
                    color=if call_empty { "currentColor" } else { "var(--online-color)" }
                />
                <div class="w-1"></div>
                {child.name}
                {if child.highlight_count > 0 {
                    view! {
                        <div class="ml-auto bg-[var(--mention-color)] text-white text-xs font-bold px-1.5 py-0.5 rounded-full">
                            {child.highlight_count}
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }}
            </div>
        </div>
        {call_preview}
        <div class="h-[1px]"></div>
    }
    .into_any()
}

#[component]
pub fn ServerItems(active_server: RoomNode) -> impl IntoView {
    let name = active_server.get_name();

    match active_server.kind {
        RoomKind::Space { children, .. } => {
            view! {
                <div class="header border-b border-(--tile-border-color) p-3 font-bold text-normal w-full">
                    {name}
                </div>
                <div class="list pr-2 w-full">
                    <For
                        each=move || children.clone()
                        key=|child| child.room_id.to_string()
                        children=move |child| { render_server_channel(child) }
                    />
                </div>
            }
                .into_any()
        }
        _ => view! { <div class="item p-4">"Not found"</div> }.into_any(),
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let state: AppState = expect_context();

    let (dragged_server_id, set_dragged_server_id) = signal::<Option<String>>(None);

    let Ok(img) = web_sys::HtmlImageElement::new() else {
        return view! { <div class="item p-4">"Error initializing drag image"</div> }.into_any();
    };
    img.set_src("data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7");

    let active_dms = Memo::new(move |_| {
        state.sidebar_state.with(|state| {
            state
                .dms
                .iter()
                .filter(|dm| dm.notification_count > 0)
                .cloned()
                .collect::<Vec<_>>()
        })
    });

    view! {
        <div class="flex h-full gap-[var(--gap)] select-none">
            // Empty image used for drag ghost to avoid default semi-transparent preview
            <img
                id="drag-ghost"
                src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
                style="position: absolute; top: -1000px; left: -1000px; opacity: 0;"
            />

            <FloatingTile>
                <div class="servers w-16 flex flex-col items-center pt-3 pb-3 overflow-y-auto">

                    <div class="relative flex items-center justify-center group w-full">
                        <IndicatorPill
                            is_active=Memo::new(move |_| state.active_server_id.get().is_none())
                            has_notifications=Memo::new(move |_| false)
                        />

                        <div
                            class="server-btn flex items-center justify-center w-10 h-10 bg-gray-700 text-white rounded-[25%] cursor-pointer transition-colors"
                            style:background-color=move || {
                                if state.active_server_id.get().is_none() {
                                    "var(--accent-color)".to_string()
                                } else {
                                    "var(--color-item-hover)".to_string()
                                }
                            }
                            on:click=move |_| state.set_active_server_id(None)
                        >
                            <div
                                class="transition-colors w-full h-full flex items-center justify-center"
                                style:color=move || {
                                    if state.active_server_id.get().is_none() {
                                        "var(--color-item)".to_string()
                                    } else {
                                        "var(--accent-color)".to_string()
                                    }
                                }
                            >
                                <Icon
                                    icon=MATRIX_LOGO
                                    size="85%"
                                    color="currentColor"
                                    weight=IconWeight::Bold
                                />
                            </div>
                        </div>
                    </div>

                    <For
                        each=move || active_dms.get()
                        key=|dm| dm.room_id.to_string()
                        children=move |dm| {
                            let click_id = dm.room_id.to_string();
                            let clone = click_id.clone();
                            let initial = dm
                                .name
                                .clone()
                                .unwrap_or_else(|| "Unnamed".to_string())
                                .chars()
                                .next()
                                .unwrap_or_default()
                                .to_string();
                            let is_active = Memo::new(move |_| {
                                state.active_room_id() == Some(click_id.clone())
                            });
                            let has_notifications = Memo::new(move |_| dm.notification_count > 0);
                            let corner = if dm.notification_count > 0 {
                                Some(CutoutBadgeCorner {
                                    fg_color: "white".to_string(),
                                    bg_color: "var(--mention-color)".to_string(),
                                    content: CutoutBadgeContent::Number(dm.notification_count),
                                })
                            } else {
                                None
                            };
                            view! {
                                <div class="h-2"></div>
                                <div
                                    class="relative flex items-center justify-center group w-full cursor-pointer"
                                    on:click=move |_| {
                                        state.set_active_server_id(None);
                                        state.set_active_room_with_id(Some(clone.clone()));
                                    }
                                >
                                    <IndicatorPill
                                        is_active=is_active
                                        has_notifications=has_notifications
                                    />

                                    <CutoutBadge bottom_right=corner class="justify-center flex">
                                        <div
                                            class="avatar-circle w-10 h-10 rounded-full"
                                            style:justify-content="center"
                                        >
                                            {
                                                let failed = RwSignal::new(false);
                                                let url = format!("mxc://room/{}", dm.room_id);
                                                view! {
                                                    <img
                                                        class="avatar-img w-full h-full object-cover"
                                                        class:hidden=failed
                                                        src=url
                                                        alt=dm.name.clone()
                                                        on:error=move |_| failed.set(true)
                                                        on:load=move |_| failed.set(false)
                                                    />
                                                    <TextCircle
                                                        text=initial
                                                        color=get_color(&dm.room_id)
                                                        class="rounded-full w-10 h-10"
                                                        class:hidden=move || !failed.get()
                                                    />
                                                }
                                            }
                                        </div>
                                    </CutoutBadge>
                                </div>
                            }
                        }
                    />

                    <div class="w-8 h-[1px] bg-red-500 rounded-full my-2 gap-[1px]"></div>
                    <For
                        each=move || state.sidebar_state.get().servers
                        key=|server| server.room_id.to_string()
                        children=move |server| {
                            let drag_id = server.room_id.to_string();
                            let drop_id = server.room_id.to_string();

                            view! {
                                <div
                                    draggable="true"
                                    class="w-full flex flex-col items-center cursor-grab active:cursor-grabbing"
                                    on:dragstart={
                                        let img = img.clone();
                                        move |e| {
                                            if let Some(data_transfer) = e.data_transfer() {
                                                let _ = data_transfer.set_data("text/plain", &drag_id);
                                                data_transfer.set_drag_image(&img, 0, 0);
                                            }
                                            set_dragged_server_id.set(Some(drag_id.clone()));
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
                                            state
                                                .sidebar_state
                                                .update(|state| {
                                                    let src_opt = state
                                                        .servers
                                                        .iter()
                                                        .position(|s| s.room_id == source_id);
                                                    let dst_opt = state
                                                        .servers
                                                        .iter()
                                                        .position(|s| s.room_id == drop_id);
                                                    if let (Some(src_idx), Some(dst_idx)) = (src_opt, dst_opt) {
                                                        let item = state.servers.remove(src_idx);
                                                        state.servers.insert(dst_idx, item);
                                                    }
                                                });
                                        }
                                    }
                                    on:dragend=move |_| {
                                        set_dragged_server_id.set(None);
                                        spawn_local(async move {
                                            let current_servers = state
                                                .sidebar_state
                                                .get_untracked()
                                                .servers;
                                            let new_order: Vec<String> = current_servers
                                                .into_iter()
                                                .map(|s| s.room_id)
                                                .collect();
                                            state.set_server_order(new_order);
                                        });
                                    }
                                >
                                    <ServerIcon server_id=server.room_id.clone() />
                                    <div class="h-2 pointer-events-none"></div>
                                </div>
                            }
                        }
                    />
                </div>
            </FloatingTile>

            <div class="flex flex-col gap-(--gap)">
                <FloatingTile class="h-(--header-height)">"Search stuff"</FloatingTile>
                <FloatingTile class="w-65 flex-grow flex">
                    {move || {
                        let current_state = state.sidebar_state.get();
                        match state.active_server_id.get() {
                            None => {
                                view! {
                                    <div class="header border-b border-(--tile-border-color) font-bold text-normal p-3 flex flex-row w-full">
                                        "Direct Messages" <div class="flex flex-grow"></div>
                                    </div>
                                    <div class="py-1 gap-1 flex flex-col w-full">
                                        <For
                                            each=move || current_state.dms.clone()
                                            key=|dm| dm.room_id.to_string()
                                            children=move |dm| {
                                                let click_id = dm.room_id.to_string();

                                                view! {
                                                    <DmDiv
                                                        dm=dm.clone()
                                                        on:click=move |_| {
                                                            state.set_active_room_with_id(Some(click_id.clone()))
                                                        }
                                                    />
                                                }
                                            }
                                        />
                                    </div>
                                }
                                    .into_any()
                            }
                            Some(active_id) => {
                                let Some(active_server) = current_state
                                    .servers
                                    .into_iter()
                                    .find(|s| s.room_id == active_id) else {
                                    return view! { <div class="item p-4">"Not found"</div> }
                                        .into_any();
                                };
                                view! { <ServerItems active_server=active_server></ServerItems> }
                                    .into_any()
                            }
                        }
                    }}
                // </div>
                </FloatingTile>

                // Small card with current room profile
                <FloatingTile class="h-(--header-height) w-full">
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

    let current_room_profile = move || {
        let room_id = state.active_room_id();
        let user_id = state.user_id.get();

        if user_id.is_empty() {
            return ().into_any();
        }

        if let Some(room_id) = room_id {
            let profile_sig = store.get_member_profile(&room_id, &user_id);
            let name_sig = profile_sig.clone();

            view! {
                <PresenceBadge presence=store.get_presence(&user_id)>
                    {move || profile_sig.get().render_icon(40)}
                </PresenceBadge>
                {move || name_sig.get().render_name(16)}
            }
            .into_any()
        } else {
            let profile_sig = store.get_user_profile(&user_id);
            let name_sig = profile_sig.clone();

            view! {
                <PresenceBadge presence=store.get_presence(&user_id)>
                    {move || profile_sig.get().render_icon(40)}
                </PresenceBadge>
                {move || name_sig.get().render_name(16)}
            }
            .into_any()
        }
    };

    view! { <div class="flex items-center justify-start w-full h-full px-2">{current_room_profile}</div> }
}
