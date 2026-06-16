use phosphor_leptos::{Icon, IconData, IconWeight, HASH, MATRIX_LOGO, SPEAKER_HIGH};
use shared::{get_color, profile::MemberProfile, sidebar::UserDevice, unknown_color};

use crate::{
    components::{
        presence::PresenceBadge,
        user_profile::{render_url_icon, MemberProfileExt},
        DeafenMenu, FloatingTile, MuteMenu, SettingsIcon,
    },
    state::{AppState, ProfileStore},
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::sidebar::{RoomKind, RoomNode};

use crate::components::TextCircle;

pub trait RoomNodeExt {
    fn avatar_div<T: AsRef<str> + 'static>(&self, size_str: T) -> impl IntoView + 'static;
    fn room_id(&self) -> String;
    fn kind(&self) -> RoomKind;
}

impl RoomNodeExt for RoomNode {
    fn avatar_div<T: AsRef<str> + 'static>(&self, size_str: T) -> impl IntoView + 'static {
        let url = self.avatar_url();
        let name = self.get_name();
        let color = self.color.clone();

        let rounding = if let RoomKind::Dm { .. } = self.kind {
            "full"
        } else {
            "[25%]"
        };

        render_url_icon(url, name, size_str, color, rounding)
    }

    fn room_id(&self) -> String {
        self.room_id.clone()
    }

    fn kind(&self) -> RoomKind {
        self.kind.clone()
    }
}

impl RoomNodeExt for Option<RoomNode> {
    fn avatar_div<T: AsRef<str> + 'static>(&self, size_str: T) -> impl IntoView + 'static {
        if let Some(node) = self {
            node.avatar_div(size_str).into_any()
        } else {
            render_url_icon(None, "?", size_str, unknown_color(), "[25%]").into_any()
        }
    }

    fn room_id(&self) -> String {
        self.as_ref().map(|node| node.room_id()).unwrap_or_default()
    }

    fn kind(&self) -> RoomKind {
        self.as_ref().map(|node| node.kind()).unwrap_or_default()
    }
}

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

    let avatar_url = dm.avatar_url();
    let failed = RwSignal::new(avatar_url.is_none());
    let first_char = name.chars().next().unwrap_or('?').to_string();
    let avatar_content = view! {
        {avatar_url
            .map(|url| {
                view! {
                    <img
                        class="avatar-img w-8 h-8 rounded-full object-cover"
                        class:hidden=failed
                        src=url
                        alt=name.clone()
                        on:error=move |_| failed.set(true)
                        on:load=move |_| failed.set(false)
                    />
                }
            })}
        <TextCircle
            text=first_char
            color=color
            class="rounded-full w-8 h-8"
            class:hidden=move || !failed.get()
        />
    };

    let call_room_id = dm.room_id.clone();

    let notifications = move || {
        state
            .notification_counts
            .get()
            .get(&dm.room_id)
            .cloned()
            .unwrap_or_default()
    };

    let call_icon = move || {
        let members_in_call = state.get_call_members(&call_room_id).get();
        if !members_in_call.is_empty() {
            let user_in_call = members_in_call
                .iter()
                .any(|d| d.user_id == state.user_id.get());

            Some(
                view! {
                    <div class="pl-2 h-full items-center flex">
                        <Icon
                            icon=SPEAKER_HIGH
                            weight=IconWeight::Fill
                            size="16px"
                            color=if user_in_call {
                                "var(--status-online)"
                            } else {
                                "var(--status-offline)"
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

    view! {
        <div class="group flex flex-row w-full cursor-pointer px-2">
            <div class="transition-[width] duration-300 ease-out shrink-0 w-0 group-hover:w-3"></div>
            <div
                class="flex flex-row flex-grow items-center p-1 pl-2 rounded-[10px] cursor-pointer hover:text-normal"
                class=("bg-(--color-item-selected)", move || is_active.get())
                class=("text-normal", move || is_active.get())
                class=("hover:bg-[var(--color-item-hover)]", move || !is_active.get())
                class=("text-dim", move || !is_active.get())
            >
                <PresenceBadge presence=presence>{avatar_content}</PresenceBadge>
                <span class="inline-block align-center pl-2">{name}</span>
                {call_icon}
                {move || {
                    let notifications = notifications().notification_count;
                    if notifications > 0 {
                        view! {
                            <div class="ml-auto bg-[var(--mention-color)] text-white text-xs font-bold px-1.5 py-0.5 rounded-full">
                                {notifications}
                            </div>
                        }
                            .into_any()
                    } else {
                        view! { <div></div> }.into_any()
                    }
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

fn render_dm_preview(dm: RoomNode, members: Option<Vec<UserDevice>>) -> impl IntoView {
    let state: AppState = expect_context();

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

    let is_active = Memo::new(move |_| state.active_room_id() == Some(click_id.clone()));
    let room_id_for_count = dm.room_id.clone();

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
                fg_color: "white".to_string(),
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
                let user_in_call = user_ids_in_calls.contains(&state.user_id.get());
                Some(CutoutBadgeCorner {
                    fg_color: "white".to_string(),
                    bg_color: if user_in_call {
                        "var(--status-online)".to_string()
                    } else {
                        "var(--status-offline)".to_string()
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
                has_notifications=move || { notifications.get().has_notifications() }
            />

            <CutoutBadge bottom_right=br_corner top_right=tr_corner class="justify-center flex">
                <div class="avatar-circle w-10 h-10 rounded-full" style:justify-content="center">
                    {render_url_icon(
                        dm.avatar_url(),
                        initial,
                        "40px",
                        get_color(&dm.dm_user_id().unwrap_or_default()),
                        "full",
                    )}
                </div>
            </CutoutBadge>
        </div>
    }
}

#[component]
pub fn ServerIcon(server: impl RoomNodeExt) -> impl IntoView {
    let state = expect_context::<AppState>();

    let server_id = server.room_id();

    let server_id_for_active = server_id.clone();
    let server_id_for_click = server_id.clone();

    let is_active =
        Memo::new(move |_| state.active_server_id.get() == Some(server_id_for_active.clone()));

    let server_id_for_not = server_id.clone();
    let notifications = Memo::new(move |_| {
        state
            .notification_counts
            .get()
            .get(&server_id_for_not)
            .cloned()
            .unwrap_or_default()
    });

    let has_notifications = Memo::new(move |_| {
        let counts = notifications.get();
        counts.notification_count > 0 || counts.highlight_count > 0
    });

    let avatar_content = server.avatar_div("40px");

    let kind = server.kind();
    let tr_corner = move || {
        if let RoomKind::Space { all_children, .. } = kind.clone() {
            let user_ids_in_calls = state.get_call_members_in_rooms(all_children.clone());

            if !user_ids_in_calls.is_empty() {
                let user_in_call = user_ids_in_calls.contains(&state.user_id.get());
                Some(CutoutBadgeCorner {
                    fg_color: "white".to_string(),
                    bg_color: if user_in_call {
                        "var(--status-online)".to_string()
                    } else {
                        "var(--status-offline)".to_string()
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

    let server_id_for_click = server_id_for_click.clone();

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
            <IndicatorPill is_active=is_active has_notifications=has_notifications />
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
                            state.set_active_server_id(Some(server_id_for_click.clone()))
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

pub fn render_server_channel(child: RoomNode) -> impl IntoView {
    let state: AppState = expect_context();
    let store: ProfileStore = expect_context();

    let channel_icon = match child.kind {
        RoomKind::Dm { .. } => HASH,
        RoomKind::TextChannel => HASH,
        RoomKind::VoiceChannel => SPEAKER_HIGH,
        RoomKind::Space { .. } => MATRIX_LOGO,
    };

    let click_id = child.room_id.to_string();
    let check_id = child.room_id.to_string();
    let is_active = Memo::new(move |_| state.active_room_id() == Some(check_id.clone()));

    let room_id = child.room_id.clone();

    let call_participants_sig = state.get_call_members(&room_id);

    let empty_sig = call_participants_sig.clone();
    let call_empty = move || empty_sig.get().is_empty();

    let room_id_for_count = child.room_id.clone();
    let highlight_count = move || {
        let counts = state
            .notification_counts
            .get()
            .get(&room_id_for_count)
            .cloned()
            .unwrap_or_default();
        counts.highlight_count
    };

    let room_id_for_not = child.room_id.clone();
    let has_notifications = Memo::new(move |_| {
        let counts = state
            .notification_counts
            .get()
            .get(&room_id_for_not)
            .cloned()
            .unwrap_or_default();
        counts.notification_count > 0 || counts.highlight_count > 0
    });

    let participants = Memo::new(move |_| call_participants_sig.get());

    let call_preview = move || {
        if let RoomKind::VoiceChannel = &child.kind {
            let participants = participants.get();

            let views = participants.iter().map(|device| {
                    let profile = store.get_member_profile(&child.room_id, &device.user_id);
                    let clone = profile.clone();

                    view! {
                        <div class="hover:bg-(--color-item-hover) rounded-[10px] p-1 flex items-center gap-2 flex flex-grow cursor-pointer">
                            {move || profile.get().render_icon("22px")}
                            {move || clone.get().render_name("14px")}
                        </div>
                    }
                });

            Some(view! { <div class="flex pl-8 flex-col gap-1">{views.collect_view()}</div> })
        } else {
            None
        }
    };

    view! {
        <div class="group relative flex flex-row w-full cursor-pointer">

            {move || {
                has_notifications
                    .get()
                    .then(|| {
                        view! {
                            <div class="absolute top-1/2 -translate-y-1/2 -left-1 group-hover:left-1.5 transition-[left] duration-300 ease-out w-2 h-2 bg-(--text-bright) rounded-full z-10 pointer-events-none"></div>
                        }
                    })
            }}
            <div class="transition-[width] duration-300 ease-out shrink-0 w-2 group-hover:w-5"></div>

            <div
                class="flex flex-row flex-grow items-center p-1 rounded-[10px] cursor-pointer transition-colors hover:text-normal"
                class=("hover:bg-(--color-item-hover)", move || !is_active.get())
                class=("text-dim", move || !is_active.get() && !has_notifications.get())
                class=(
                    "text-normal",
                    move || { !is_active.get() && has_notifications.get() || is_active.get() },
                )
                class=("bg-(--color-item-selected)", move || is_active.get())
                on:click=move |_| { state.set_active_room_with_id(Some(click_id.clone())) }
            >
                <Icon
                    icon=channel_icon
                    size="20px"
                    color=move || if call_empty() { "currentColor" } else { "var(--online-color)" }
                />
                <div class="w-1"></div>
                {child.name}
                {if highlight_count() > 0 {
                    view! {
                        <div class="ml-auto bg-(--mention-color) text-white text-xs font-bold px-1.5 py-0.5 rounded-full">
                            {highlight_count}
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
    let state: AppState = expect_context();

    match active_server.kind {
        RoomKind::Space { children, .. } => {
            view! {
                <div class="header border-b border-(--tile-border-color) p-3 font-bold text-normal w-full">
                    {name}
                </div>
                <div class="list pr-2 w-full pt-1">
                    <For
                        each=move || children.clone()
                        key=|room_id| room_id.clone()
                        children=move |room_id| {
                            let Some(node) = state.get_room(&room_id) else {
                                return view! { <div class="item p-4">"Loading..."</div> }.into_any()
                            };
                            render_server_channel(node).into_any()
                        }
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
        let notifications = state.notification_counts.get();

        let sidebar = state.sidebar_state.get();

        sidebar
            .dms
            .into_iter()
            .filter_map(|dm| {
                let has_notifications = notifications
                    .get(&dm.room_id)
                    .cloned()
                    .unwrap_or_default()
                    .notification_count
                    > 0;

                let call_members = state.get_call_members(&dm.room_id).get();

                if !has_notifications && call_members.is_empty() {
                    None
                } else {
                    let members = (!call_members.is_empty()).then_some(call_members);
                    Some((dm, members))
                }
            })
            .collect::<Vec<_>>()
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
                        key=|(dm, _)| dm.room_id.to_string()
                        children=move |(dm, members)| { render_dm_preview(dm, members) }
                    />

                    <div class="w-8 h-[1px] bg-red-500 rounded-full my-2 gap-[1px]"></div>
                    <For
                        each=move || state.sidebar_state.get().top_level_servers
                        key=|server_id| server_id.clone()
                        children=move |server_id| {
                            let drag_id = server_id.clone();
                            let drop_id = server_id.clone();

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
                                            state.reorder_servers(&source_id, &drop_id);
                                        }
                                    }
                                    on:dragend=move |_| {
                                        set_dragged_server_id.set(None);
                                        spawn_local(async move {
                                            let current_servers = state
                                                .sidebar_state
                                                .get_untracked()
                                                .top_level_servers;
                                            state.set_server_order(current_servers);
                                        });
                                    }
                                >
                                    <ServerIcon server=state.get_room(&server_id) />
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
                                let Some(active_server_id) = current_state
                                    .top_level_servers
                                    .into_iter()
                                    .find(|s_id| s_id == &active_id) else {
                                    return view! { <div class="item p-4">"Not found"</div> }
                                        .into_any();
                                };
                                let Some(active_server) = state.get_room(&active_server_id) else {
                                    return view! { <div class="item p-4">"Loading..."</div> }
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

    let current_profile: RwSignal<Option<MemberProfile>> = RwSignal::new(None);

    let profile_store = store.clone();
    Effect::new(move |_| {
        let room_id = state.active_room_id();
        let user_id = state.user_id.get();
        if user_id.is_empty() {
            return;
        }

        if let Some(rid) = room_id {
            let profile = profile_store.get_member_profile(&rid, &user_id).get();
            current_profile.set(Some(profile));
        } else {
            let profile = profile_store.get_user_profile(&user_id).get();
            current_profile.set(Some(MemberProfile {
                room_id: "global".to_string(),
                profile,
            }));
        }
    });

    let current_room_profile = move || {
        let profile = current_profile.get();
        let name_profile = profile.clone();

        view! {
            <PresenceBadge presence=store
                .get_presence(
                    &profile.clone().map(|p| p.profile.user_id.clone()).unwrap_or_default(),
                )>{profile.render_icon("30px")}</PresenceBadge>
            {name_profile.render_name("14px")}
        }
        .into_any()
    };

    view! {
        <div class="flex items-center justify-start w-full h-full px-2 gap-2">
            {current_room_profile} <div class="ml-auto flex items-center h-full gap-2">
                <MuteMenu />
                <DeafenMenu />
                <SettingsIcon />
            </div>
        </div>
    }
}
