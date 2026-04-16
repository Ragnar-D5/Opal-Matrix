use crate::app::call_tauri_no_args;
use leptos::leptos_dom::logging::{console_error, console_log};
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
    },
}

impl RoomNode {
    pub fn id(&self) -> &str {
        match self {
            RoomNode::Space { room_id, .. } => room_id,
            RoomNode::Channel { room_id, .. } => room_id,
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
pub fn Sidebar(set_active_room_id: WriteSignal<Option<String>>) -> impl IntoView {
    let (sidebar_state, set_sidebar_state) = signal(SidebarState::default());

    let (selected_space, set_selected_space) = signal(None::<String>);

    let live_update_event = use_tauri_event("sidebar_update");

    Effect::new(move |_| {
        if let Some(new_state) = live_update_event.get() {
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
        <div class="sidebar-container" style="display: flex;">
            <div class="server-column">
                // Display a string of all ids
                {move || sidebar_state.get().dms.iter().map(|s| s.id()).collect::<Vec<_>>().join(", ")}
                "dwadwa"
            </div>

            <div class="channel-column">
                {move || match selected_space.get() {
                    None => view! {
                        <div class="dm-list">
                            <h3 style="color: gray; padding-left: 10px; text-transform: uppercase; font-size: 12px;">"Direct Messages"</h3>
                        </div>
                    },
                    Some(active_server_id) => view! {
                        <div class="channel-list">
                            <h3 style="color: gray; padding-left: 10px; text-transform: uppercase; font-size: 12px;">"Channels"</h3>
                        </div>
                    }
                }}
            </div>
        </div>
    }
}
