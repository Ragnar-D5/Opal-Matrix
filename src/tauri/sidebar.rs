use crate::app::call_tauri_no_args;
use leptos::leptos_dom::logging::{console_error, console_log};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use stylist::style;

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
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SidebarState {
    pub dms: Vec<RoomNode>,
    pub servers: Vec<RoomNode>,
    pub orphaned_rooms: Vec<RoomNode>,
}

#[component]
pub fn Sidebar(
    active_room_id: ReadSignal<Option<String>>,
    set_active_room_id: WriteSignal<Option<String>>,
    active_server_id: ReadSignal<Option<String>>,
    set_active_server_id: WriteSignal<Option<String>>,
) -> impl IntoView {
    let (sidebar_state, set_sidebar_state) = signal(SidebarState::default());

    let (selected_space, set_selected_space) = signal(None::<String>);

    let live_update_event: ReadSignal<Option<SidebarState>> = use_tauri_event("sidebar_update");

    Effect::new(move |_| {
        if let Some(mut new_state) = live_update_event.get() {
            console_log(&format!("{:?}", new_state));

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

    let style = style! {
        display: flex;
        height: 100vh;
        font-family: sans-serif;
        font-color: #000;

        /* Far-left Server Column */
        .servers {
            width: 60px;
            border-right: 1px solid #ccc;
            padding-top: 10px;
        }

        .server-btn {
            width: 40px;
            height: 40px;
            margin: 0 auto 10px auto;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            cursor: pointer;
        }

        .server-btn[data-active="true"] {
            // background-color: #bbb;
        }

        /* Inner Channel Column */
        .channels {
            width: 200px;
            border-right: 1px solid #ccc;
            display: flex;
            flex-direction: column;
        }

        .header {
            padding: 15px 10px;
            font-weight: bold;
            border-bottom: 1px solid #ddd;
        }

        .list {
            flex: 1;
            overflow-y: auto;
        }

        .item {
            padding: 10px;
            cursor: pointer;
        }

        .item:hover {
            background-color: #eee;
        }
    }
    .expect("Failed to parse minimal styles");

    let class_name = style.get_class_name().to_string();

    view! {
        <div class=class_name>

            <div class="servers">
                <div
                    class="server-btn"
                    attr:data-active=move || active_server_id.get().is_none().to_string()
                    on:click=move |_| set_active_server_id.set(None)
                >
                    "DM"
                </div>

                <For
                    each=move || sidebar_state.get().servers
                    key=|server| server.id().to_string()
                    children=move |server| {
                        let id_click = server.id().to_string();
                        let id_active = server.id().to_string();
                        let initial = server.display_name().chars().next().unwrap_or('?').to_string();

                        view! {
                            <div
                                class="server-btn"
                                attr:data-active=move || (selected_space.get() == Some(id_active.clone())).to_string()
                                on:click=move |_| set_active_server_id.set(Some(id_click.clone()))
                            >
                                {initial}
                            </div>
                        }
                    }
                />
            </div>

            <div class="channels">
                {move || {
                    let current_state = sidebar_state.get();

                    match active_server_id.get() {
                        None => view! {
                            <div class="header">"Direct Messages"</div>
                            <div class="list">
                                <For
                                    each=move || current_state.dms.clone()
                                    key=|dm| dm.id().to_string()
                                    children=move |dm| {
                                        let click_id = dm.id().to_string();
                                        view! {
                                            <div class="item" on:click=move |_| set_active_room_id.set(Some(click_id.clone()))>
                                                {dm.display_name()}
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        }.into_any(),

                        Some(active_id) => {
                            let active_server = current_state.servers.into_iter().find(|s| s.id() == active_id);

                            match active_server {
                                Some(RoomNode::Space { name, children, .. }) => view! {
                                    <div class="header">{name.unwrap_or_else(|| "Server".to_string())}</div>
                                    <div class="list">
                                        <For
                                            each=move || children.clone()
                                            key=|child| child.id().to_string()
                                            children=move |child| {
                                                let click_id = child.id().to_string();
                                                view! {
                                                    <div class="item" on:click=move |_| set_active_room_id.set(Some(click_id.clone()))>
                                                        "# " {child.display_name()}
                                                    </div>
                                                }
                                            }
                                        />
                                    </div>
                                }.into_any(),
                                _ => view! { <div class="item">"Not found"</div> }.into_any()
                            }
                        }
                    }
                }}
            </div>

        </div>
    }
}
