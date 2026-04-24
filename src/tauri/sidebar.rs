use crate::app::{call_tauri_no_args, AppState};
use crate::components::FloatingTile;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;

use crate::hooks::use_tauri_event;

#[derive(Debug, Deserialize, Clone)]
pub enum RoomNode {
    Space {
        room_id: String,
        name: Option<String>,
        topic: Option<String>,
        avatar_url: Option<String>,

        children: Vec<RoomNode>,
    },
    Channel {
        room_id: String,
        name: Option<String>,
        topic: Option<String>,
        avatar_url: Option<String>,

        last_ts: Option<i64>,
    },
}

impl RoomNode {
    pub fn id(&self) -> &str {
        match self {
            RoomNode::Space { room_id, .. } => room_id,
            RoomNode::Channel { room_id, .. } => room_id,
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            RoomNode::Space { name, .. } | RoomNode::Channel { name, .. } => {
                name.clone().unwrap_or_else(|| "Unnamed".to_string())
            }
        }
    }

    pub fn last_ts(&self) -> Option<i64> {
        match self {
            RoomNode::Space { .. } => None,
            RoomNode::Channel { last_ts, .. } => *last_ts,
        }
    }

    pub fn avatar_url(&self) -> Option<String> {
        match self {
            RoomNode::Space { avatar_url, .. } | RoomNode::Channel { avatar_url, .. } => {
                avatar_url.clone()
            }
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SidebarState {
    pub dms: Vec<RoomNode>,
    pub servers: Vec<RoomNode>,
    pub orphaned_rooms: Vec<RoomNode>,
}

#[component]
fn DmDiv(dm: RoomNode) -> impl IntoView {
    let state = expect_context::<AppState>();

    let id = dm.id().to_string();
    let name = dm.display_name();
    let avatar_url = dm.avatar_url();
    let initial = name.chars().take(2).collect::<String>();

    let is_active = Memo::new(move |_| state.active_room_id.get() == Some(id.clone()));

    view! {
        <div class="group flex flex-row w-full cursor-pointer">
            <div class="transition-[width] duration-300 ease-out shrink-0 w-0 group-hover:w-3"></div>
            <div
                class="flex flex-row flex-grow items-center p-1 pl-2 rounded-[10px] cursor-pointer text-dim hover:text-bright"
                class=("bg-[var(--color-item-selected)]", move || is_active.get())
                class=("text-bright", move || is_active.get())
                class=("hover:bg-[var(--color-item-hover)]", move || !is_active.get())
                class=("text-dim", move || !is_active.get())
            >
                <div class="avatar-circle w-8 h-8 rounded-full">
                    {match avatar_url {
                        Some(url) => view! {
                            <img class="avatar-img" src=url alt=name.clone() />
                        }.into_any(),
                        None => view! {
                            <span>{initial}</span>
                        }.into_any(),
                    }}
                </div>
                <span class="inline-block align-center pl-2">{name}</span>
            </div>
        </div>
    }
}

#[component]
pub fn IndicatorPill(#[prop(into)] is_active: Signal<bool>) -> impl IntoView {
    view! {
        <div
            class="absolute left-1 w-1 bg-white rounded-full top-1/2 -translate-y-1/2 transition-all duration-200 ease-in-out"
            class=("h-10", move || is_active.get())
            class=("h-0", move || !is_active.get())
            class=("group-hover:h-[20px]", move || !is_active.get())
        ></div>
    }
}

#[component]
pub fn ServerIcon(server: RoomNode) -> impl IntoView {
    let state = expect_context::<AppState>();

    let id = server.id().to_string();
    let cloned_id = id.clone();

    let initial = server
        .display_name()
        .chars()
        .next()
        .unwrap_or('?')
        .to_string();

    let is_active = Memo::new(move |_| state.active_server_id.get() == Some(cloned_id.clone()));

    view! {
        <div class="relative flex items-center justify-center group w-full">

            // Pass the Memo into the extracted pill
            <IndicatorPill is_active=is_active />

            <div
                class="server-btn flex items-center justify-center w-10 h-10 text-gray-800 font-semibold rounded-[25%] cursor-pointer transition-colors"
                class=("bg-[var(--color-icon-selected)]", move || is_active.get())
                class=("bg-[var(--color-icon-bg)]", move || !is_active.get())
                class=("hover:bg-[var(--color-icon-hover)]", move || !is_active.get())
                on:click=move |_| state.active_server_id.set(Some(id.clone()))
            >
                {match server.avatar_url() {
                    Some(url) => view! {
                        <img class="avatar-img rounded-[25%]" src=url alt=initial.clone() />
                    }.into_any(),
                    None => view! {
                        <span>{initial}</span>
                    }.into_any(),
                }}
            </div>
        </div>
    }
}

#[component]
pub fn Sidebar() -> impl IntoView {
    let state = expect_context::<AppState>();

    let (sidebar_state, set_sidebar_state) = signal(SidebarState::default());

    let (selected_space, set_selected_space) = signal(None::<String>);

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    Effect::new(move |_| {
        if let Some(mut new_state) = sidebar_update_event.get() {
            new_state
                .dms
                .sort_by(|a, b| b.last_ts().unwrap_or(0).cmp(&a.last_ts().unwrap_or(0)));

            set_sidebar_state.set(new_state);
        }
    });

    Effect::new(move |_| {
        // fire once after mount/listener setup
        spawn_local(async move {
            let _ = call_tauri_no_args("send_frontend").await;
        });
    });

    view! {
        <div class="flex h-full gap-[var(--gap)] select-none">
            <FloatingTile>
                <div class="servers w-16 flex flex-col items-center pt-3 pb-3 overflow-y-auto">

                    <div class="relative flex items-center justify-center group w-full">
                        // Discord-style dynamic indicator pill
                        <IndicatorPill is_active=Memo::new(move |_| state.active_server_id.get().is_none()) />

                        <div
                            class="server-btn flex items-center justify-center w-10 h-10 bg-gray-700 text-white rounded-[25%] cursor-pointer hover:bg-gray-600 transition-colors"
                            on:click=move |_| state.active_server_id.set(None)
                        >
                            // Discord generic logo SVG
                            <svg class="w-[60%] h-[60%] fill-current" viewBox="0 0 127.14 96.36">
                                <path d="M107.7,8.07A105.15,105.15,0,0,0,81.47,0a72.06,72.06,0,0,0-3.36,6.83A97.68,97.68,0,0,0,49,6.83,72.37,72.37,0,0,0,45.64,0,105.89,105.89,0,0,0,19.39,8.09C2.79,32.65-1.71,56.6.54,80.21h0A105.73,105.73,0,0,0,32.71,96.36,77.7,77.7,0,0,0,39.6,85.25a68.42,68.42,0,0,1-10.85-5.18c.91-.66,1.8-1.34,2.66-2a75.57,75.57,0,0,0,64.32,0c.87.71,1.76,1.39,2.66,2a68.68,68.68,0,0,1-10.87,5.19,77,77,0,0,0,6.89,11.1,105.25,105.25,0,0,0,32.19-16.14c2.64-27.38-4.51-51.11-19.32-72.15ZM42.56,65.3c-5.36,0-9.8-4.83-9.8-10.79s4.38-10.79,9.8-10.79,9.85,4.83,9.8,10.79c0,5.96-4.45,10.79-9.8,10.79Zm42,0c-5.36,0-9.8-4.83-9.8-10.79s4.38-10.79,9.8-10.79,9.85,4.83,9.8,10.79c0,5.96-4.45,10.79-9.8,10.79Z"/>
                            </svg>
                        </div>
                    </div>

                    // Divider line
                    <div class="w-8 h-[1px] bg-gray-300 rounded-full my-2 gap-[1px]"></div>
                    <For
                        each=move || sidebar_state.get().servers
                        key=|server| server.id().to_string()
                        children=move |server| {
                            let id_click = server.id().to_string();
                            let id_active = server.id().to_string();
                            let initial = server.display_name().chars().next().unwrap_or('?').to_string();

                            let is_active = Memo::new(move |_| state.active_server_id.get() == Some(id_active.clone()));

                            view! {
                                <ServerIcon server=server.clone() />
                                <div class="h-2"></div>
                            }
                        }
                    />
                </div>
            </FloatingTile>

            <FloatingTile>
                <div class="channels w-75 p-2">
                    {move || {
                        let current_state = sidebar_state.get();

                        match state.active_server_id.get() {
                            None => view! {
                                <div class="header border-b border-gray-300 p-3 font-bold">"Direct Messages"</div>
                                <div class="list">
                                    <span class="pl-1 text-normal">"Direct messages"</span>
                                    <For
                                        each=move || current_state.dms.clone()
                                        key=|dm| dm.id().to_string()
                                        children=move |dm| {
                                            let click_id = dm.id().to_string();
                                            let check_id = dm.id().to_string();
                                            let is_active = Memo::new(move |_| state.active_room_id.get() == Some(check_id.clone()));

                                            view! {
                                                <DmDiv dm=dm.clone()
                                                    on:click=move |_| state.active_room_id.set(Some(click_id.clone())) />
                                            }
                                        }
                                    />
                                </div>
                            }.into_any(),

                            Some(active_id) => {
                                let active_server = current_state.servers.into_iter().find(|s| s.id() == active_id);

                                match active_server {
                                    Some(RoomNode::Space { name, children, .. }) => view! {
                                        <div class="header border-b border-gray-300 p-3 font-bold text-normal">{name.unwrap_or_else(|| "Server".to_string())}</div>
                                        <div class="list pl-1">
                                            <For
                                                each=move || children.clone()
                                                key=|child| child.id().to_string()
                                                children=move |child| {
                                                    let click_id = child.id().to_string();
                                                    let check_id = child.id().to_string();
                                                    let is_active = Memo::new(move |_| state.active_room_id.get() == Some(check_id.clone()));

                                                    view! {
                                                        <div class="group flex flex-row w-full cursor-pointer">
                                                            <div class="transition-[width] duration-300 ease-out shrink-0 w-0 group-hover:w-3"></div>
                                                            <div
                                                                class="flex flex-row flex-grow items-center p-1 rounded-[10px] cursor-pointer transition-colors hover:text-bright"
                                                                class=("hover:bg-[color:var(--color-item-hover)]", move || !is_active.get())
                                                                class=("text-dim", move || !is_active.get())
                                                                class=("bg-[color:var(--color-item-selected)]", move || is_active.get())
                                                                class=("text-bright", move || is_active.get())
                                                                on:click=move |_| state.active_room_id.set(Some(click_id.clone()))
                                                            >
                                                                "# " {child.display_name()}
                                                            </div>
                                                        </div>
                                                        <div class="h-[1px]"></div>
                                                    }
                                                }
                                            />
                                        </div>
                                    }.into_any(),
                                    _ => view! { <div class="item p-4">"Not found"</div> }.into_any()
                                }
                            }
                        }
                    }}
                </div>
            </FloatingTile>

        </div>
    }
}
