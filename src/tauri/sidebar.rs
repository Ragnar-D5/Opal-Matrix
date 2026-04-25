use std::collections::HashMap;

use crate::app::{call_tauri_no_args, AppState};
use crate::components::FloatingTile;
use leptos::leptos_dom::logging::console_error;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use shared::sidebar::{RoomKind, RoomNode, SidebarState};

use crate::hooks::use_tauri_event;

#[component]
fn DmDiv(dm: RoomNode) -> impl IntoView {
    let state = expect_context::<AppState>();

    let id = dm.room_id.to_string();
    let name = dm.name.unwrap_or_else(|| "Unnamed".to_string());
    let avatar_url = dm.avatar_url;
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

    let id = server.room_id.to_string();
    let cloned_id = id.clone();

    let initial = server
        .name
        .unwrap_or("?".to_string())
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
                on:click=move |_| state.set_active_server_id(Some(id.clone()))
            >
                {match server.avatar_url {
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
    let (dragged_server_id, set_dragged_server_id) = signal::<Option<String>>(None);

    let sidebar_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    Effect::new(move |_| {
        if let Some(mut new_state) = sidebar_update_event.get() {
            new_state
                .dms
                .sort_by(|a, b| b.last_ts().unwrap_or(0).cmp(&a.last_ts().unwrap_or(0)));

            let current_order = state.server_order.get_untracked();

            let order_map: HashMap<&String, usize> = current_order
                .servers
                .iter()
                .enumerate()
                .map(|(index, id)| (id, index))
                .collect();

            new_state.servers.sort_by(|a, b| {
                let pos_a = order_map.get(&a.room_id).copied().unwrap_or(usize::MAX);
                let pos_b = order_map.get(&b.room_id).copied().unwrap_or(usize::MAX);

                if pos_a == usize::MAX && pos_b == usize::MAX {
                    let name_a = a.name.as_deref().unwrap_or("");
                    let name_b = b.name.as_deref().unwrap_or("");
                    return name_a.cmp(name_b);
                }

                pos_a.cmp(&pos_b)
            });

            let final_order: Vec<String> = new_state
                .servers
                .iter()
                .map(|s| s.room_id.clone())
                .collect();

            if final_order != current_order.servers {
                state.set_server_order(final_order);
            }

            set_sidebar_state.set(new_state);
        }
    });

    let Ok(img) = web_sys::HtmlImageElement::new() else {
        return view! { <div class="item p-4">"Error initializing drag image"</div> }.into_any();
    };
    img.set_src("data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7");

    view! {
        <div class="flex h-full gap-[var(--gap)] select-none">
            <img
                id="drag-ghost"
                src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7"
                style="position: absolute; top: -1000px; left: -1000px; opacity: 0;"
            />

            <FloatingTile>
                <div class="servers w-16 flex flex-col items-center pt-3 pb-3 overflow-y-auto">

                    <div class="relative flex items-center justify-center group w-full">
                        // Discord-style dynamic indicator pill
                        <IndicatorPill is_active=Memo::new(move |_| state.active_server_id.get().is_none()) />

                        <div
                            class="server-btn flex items-center justify-center w-10 h-10 bg-gray-700 text-white rounded-[25%] cursor-pointer hover:bg-gray-600 transition-colors"
                            on:click=move |_| state.set_active_server_id(None)
                        >
                            // Discord generic logo SVG
                            <svg class="w-[60%] h-[60%] fill-current" viewBox="0 0 127.14 96.36">
                                <path d="M107.7,8.07A105.15,105.15,0,0,0,81.47,0a72.06,72.06,0,0,0-3.36,6.83A97.68,97.68,0,0,0,49,6.83,72.37,72.37,0,0,0,45.64,0,105.89,105.89,0,0,0,19.39,8.09C2.79,32.65-1.71,56.6.54,80.21h0A105.73,105.73,0,0,0,32.71,96.36,77.7,77.7,0,0,0,39.6,85.25a68.42,68.42,0,0,1-10.85-5.18c.91-.66,1.8-1.34,2.66-2a75.57,75.57,0,0,0,64.32,0c.87.71,1.76,1.39,2.66,2a68.68,68.68,0,0,1-10.87,5.19,77,77,0,0,0,6.89,11.1,105.25,105.25,0,0,0,32.19-16.14c2.64-27.38-4.51-51.11-19.32-72.15ZM42.56,65.3c-5.36,0-9.8-4.83-9.8-10.79s4.38-10.79,9.8-10.79,9.85,4.83,9.8,10.79c0,5.96-4.45,10.79-9.8,10.79Zm42,0c-5.36,0-9.8-4.83-9.8-10.79s4.38-10.79,9.8-10.79,9.85,4.83,9.8,10.79c0,5.96-4.45,10.79-9.8,10.79Z"/>
                            </svg>
                        </div>
                    </div>

                    <div class="w-8 h-[1px] bg-gray-300 rounded-full my-2 gap-[1px]"></div>
                    <For
                        each=move || sidebar_state.get().servers
                        key=|server| server.room_id.to_string()
                        children=move |server| {
                            let id_click = server.room_id.to_string();
                            let id_active = server.room_id.to_string();
                            let initial = server.name.clone().unwrap_or_else(|| "Unnamed".to_string());

                            let drag_id = server.room_id.to_string();
                            let drop_id = server.room_id.to_string();

                            let is_active = Memo::new(move |_| state.active_server_id.get() == Some(id_active.clone()));

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
                                        let Some(source_id) = dragged_server_id.get() else { return };

                                        if source_id != drop_id {
                                            set_sidebar_state.update(|state| {
                                                let src_opt = state.servers.iter().position(|s| s.room_id == source_id);
                                                let dst_opt = state.servers.iter().position(|s| s.room_id == drop_id);

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
                                            let current_servers = sidebar_state.get_untracked().servers;
                                            let new_order: Vec<String> = current_servers.into_iter().map(|s| s.room_id).collect();

                                            state.set_server_order(new_order);
                                        });
                                    }
                                >
                                    <ServerIcon server=server.clone() />
                                    <div class="h-2 pointer-events-none"></div>
                                </div>
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
                                        key=|dm| dm.room_id.to_string()
                                        children=move |dm| {
                                            let click_id = dm.room_id.to_string();
                                            let check_id = dm.room_id.to_string();

                                            view! {
                                                <DmDiv dm=dm.clone()
                                                    on:click=move |_| state.set_active_room_id(Some(click_id.clone())) />
                                            }
                                        }
                                    />
                                </div>
                            }.into_any(),

                            Some(active_id) => {
                                let Some(active_server) = current_state.servers.into_iter().find(|s| s.room_id == active_id) else {
                                    return view! { <div class="item p-4">"Not found"</div> }.into_any();
                                };
                                let name = active_server.name.clone();

                                match active_server.kind {
                                    RoomKind::Space { children } => view! {
                                        <div class="header border-b border-gray-300 p-3 font-bold text-normal">{name.unwrap_or_else(|| "Server".to_string())}</div>
                                        <div class="list pl-1">
                                            <For
                                                each=move || children.clone()
                                                key=|child| child.room_id.to_string()
                                                children=move |child| {
                                                    let click_id = child.room_id.to_string();
                                                    let check_id = child.room_id.to_string();
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
                                                                on:click=move |_| state.set_active_room_id(Some(click_id.clone()))
                                                            >
                                                                "# " {child.name} {child.notification_count}
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
    }.into_any()
}
