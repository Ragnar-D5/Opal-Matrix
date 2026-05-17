use phosphor_leptos::{Icon, IconWeight, HASH, MATRIX_LOGO};

use crate::components::presence::PresenceBadge;
use crate::components::user_profile::UserProfileMaybeExt;
use crate::components::{get_color, FloatingTile};
use crate::state::{AppState, MemberStore};
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::sidebar::{RoomKind, RoomNode};

use crate::components::TextCircle;

#[component]
fn DmDiv(dm: RoomNode) -> impl IntoView {
    let state: AppState = expect_context();
    let members: MemberStore = expect_context();

    let id = dm.room_id.to_string();
    let name = dm.name.clone().unwrap_or_else(|| "Unnamed".to_string());
    let avatar_url = dm.avatar_url;
    let initial = name.chars().next().unwrap_or('?').to_string();

    let is_active = Memo::new(move |_| state.active_room_id.get() == Some(id.clone()));
    let color = get_color(dm.dm_user_id.clone().unwrap_or_default());

    let user_id = dm.dm_user_id.clone().unwrap_or_default();

    let profile = members.get_profile(&dm.room_id, &user_id);

    let avatar_content = match avatar_url {
        Some(url) => view! { <img class="avatar-img w-8 h-8 rounded-full object-cover" src=url alt=name.clone() /> }.into_any(),
        None => view! { <TextCircle text=initial color=color class="rounded-full w-8 h-8" /> }.into_any(),
    };

    let members = members.clone();
    let presence = members.get_presence(&user_id);

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
                <PresenceBadge presence=presence>
                    {move || { profile.get().render_icon(32) }}
                </PresenceBadge>
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

#[component]
pub fn CutoutBadge(
    count: u32,
    children: Children,
    #[prop(into, optional)] class: String,
) -> impl IntoView {
    let mask_style = if count > 0 {
        "-webkit-mask: radial-gradient(circle 11px at calc(100% - 8px) calc(100% - 8px), transparent 11px, black 11.5px); mask: radial-gradient(circle 11px at calc(100% - 8px) calc(100% - 8px), transparent 11px, black 11.5px);"
    } else {
        ""
    };

    view! {
        <div class="relative w-fit h-fit">
            <div class=format!("w-full h-full {class}") style=mask_style>
                {children()}
            </div>

            {if count > 0 {
                view! {
                    <div class="absolute -bottom-0 -right-0 flex items-center justify-center
                    bg-[var(--mention-color)] text-white text-[12px] font-extrabold
                    w-4 h-4 rounded-full">{count}</div>
                }
                    .into_any()
            } else {
                view! { <span class="hidden"></span> }.into_any()
            }}
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

    view! {
        <div class="relative flex items-center justify-center group w-full">
            <IndicatorPill is_active=is_active has_notifications=has_notifications />

            {move || {
                let server_id_for_click = server_id_for_click.clone();
                let Some(server) = server.get() else {
                    return view! { <div class="relative w-10 h-10"></div> }.into_any();
                };
                let name = server.name.clone().unwrap_or("?".to_string());
                let initial = name.chars().next().unwrap_or('?').to_string();
                let color = get_color(server.get_name());

                view! {
                    <div class="relative w-10 h-10">
                        <CutoutBadge count=server.highlight_count>
                            <div
                                class="server-btn flex items-center justify-center w-10 h-10 text-gray-800 font-semibold rounded-[25%] cursor-pointer transition-colors"
                                class=("bg-[var(--color-icon-selected)]", move || is_active.get())
                                class=("bg-[var(--color-icon-bg)]", move || !is_active.get())
                                class=(
                                    "hover:bg-[var(--color-icon-hover)]",
                                    move || !is_active.get(),
                                )
                                on:click=move |_| {
                                    state.set_active_server_id(Some(server_id_for_click.clone()))
                                }
                            >
                                <div class="avatar-circle w-full h-full rounded-[25%] overflow-hidden">
                                    {match server.avatar_url {
                                        Some(url) => {
                                            view! {
                                                <img
                                                    draggable="false"
                                                    class="avatar-img object-cover w-full h-full"
                                                    src=url
                                                    alt=name.clone()
                                                />
                                            }
                                                .into_any()
                                        }
                                        None => {
                                            view! {
                                                <TextCircle
                                                    text=initial
                                                    color=color
                                                    class="rounded-[25%] w-full h-full"
                                                />
                                            }
                                                .into_any()
                                        }
                                    }}
                                </div>
                            </div>
                        </CutoutBadge>
                    </div>
                }
                    .into_any()
            }}
        </div>
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
                                state.active_room_id.get() == Some(click_id.clone())
                            });
                            let has_notifications = Memo::new(move |_| dm.notification_count > 0);

                            view! {
                                <div class="h-2"></div>
                                <div
                                    class="relative flex items-center justify-center group w-full cursor-pointer"
                                    on:click=move |_| {
                                        state.set_active_server_id(None);
                                        state.set_active_room_id(Some(clone.clone()));
                                    }
                                >
                                    <IndicatorPill
                                        is_active=is_active
                                        has_notifications=has_notifications
                                    />

                                    <CutoutBadge
                                        count=dm.notification_count
                                        class="justify-center flex"
                                    >
                                        <div
                                            class="avatar-circle w-10 h-10 rounded-full"
                                            style:justify-content="center"
                                        >
                                            {match dm.avatar_url {
                                                Some(url) => {
                                                    view! {
                                                        <img
                                                            class="avatar-img w-full h-full object-cover"
                                                            src=url
                                                            alt=dm.name.clone()
                                                        />
                                                    }
                                                        .into_any()
                                                }
                                                None => {
                                                    let color = get_color(
                                                        dm.topic.clone().unwrap_or_else(|| "Unnamed".to_string()),
                                                    );
                                                    view! {
                                                        <TextCircle
                                                            text=initial
                                                            color=color
                                                            class="rounded-full w-10 h-10"
                                                        />
                                                    }
                                                        .into_any()
                                                }
                                            }}
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
                                                let _ = data_transfer.set_drag_image(&img, 0, 0);
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

            <div class="flex flex-col">
                <FloatingTile class="mb-(--gap) h-(--header-height)">"Search stuff"</FloatingTile>
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
                                                            state.set_active_room_id(Some(click_id.clone()))
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
                                let name = active_server.get_name();
                                match active_server.kind {
                                    RoomKind::Space { children } => {

                                        view! {
                                            <div class="header border-b border-(--tile-border-color) p-3 font-bold text-normal w-full">
                                                {name}
                                            </div>
                                            <div class="list pr-2 w-full">
                                                <For
                                                    each=move || children.clone()
                                                    key=|child| child.room_id.to_string()
                                                    children=move |child| {
                                                        let click_id = child.room_id.to_string();
                                                        let check_id = child.room_id.to_string();
                                                        let is_active = Memo::new(move |_| {
                                                            state.active_room_id.get() == Some(check_id.clone())
                                                        });
                                                        let has_notifications = child.notification_count > 0;

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
                                                                    class=(
                                                                        "hover:bg-[color:var(--color-item-hover)]",
                                                                        move || !is_active.get(),
                                                                    )
                                                                    class=(
                                                                        "text-dim",
                                                                        move || !is_active.get() && !has_notifications,
                                                                    )
                                                                    class=(
                                                                        "text-bright",
                                                                        move || {
                                                                            !is_active.get() && has_notifications || is_active.get()
                                                                        },
                                                                    )
                                                                    class=(
                                                                        "bg-[color:var(--color-item-selected)]",
                                                                        move || is_active.get(),
                                                                    )
                                                                    on:click=move |_| {
                                                                        state.set_active_room_id(Some(click_id.clone()))
                                                                    }
                                                                >
                                                                    <Icon icon=HASH size="20px" />
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
                                                            <div class="h-[1px]"></div>
                                                        }
                                                    }
                                                />
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    _ => {
                                        view! { <div class="item p-4">"Not found"</div> }.into_any()
                                    }
                                }
                            }
                        }
                    }}
                // </div>
                </FloatingTile>
            </div>

        </div>
    }.into_any()
}
