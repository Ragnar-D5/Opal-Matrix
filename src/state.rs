use std::collections::{HashMap, HashSet};

use leptos::{leptos_dom::logging::console_error, prelude::RwSignal, task::spawn_local};
use serde::Serialize;
use shared::{
    account_data::{AccountDataArgs, AccountDataPayload, Breadcrumbs, ServerOrder},
    sidebar::{RoomKind, RoomNode, SidebarState},
    user_profile::{PresenceInfo, UserProfile},
};

use crate::app::{call_tauri, CurrentWindow};
use leptos::prelude::*;

#[derive(Clone, Debug, Copy)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub login_name: RwSignal<String>,

    pub active_room_id: RwSignal<Option<String>>,
    pub active_server_id: RwSignal<Option<String>>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,

    pub sidebar_state: RwSignal<SidebarState>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoomHeader {
    Channel { name: String },
    DM(ArcRwSignal<UserProfile>),
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_window: RwSignal::new(CurrentWindow::LoadingPage),
            login_name: RwSignal::new(String::new()),
            active_room_id: RwSignal::new(None),
            active_server_id: RwSignal::new(None),
            breadcrums: RwSignal::new(Breadcrumbs::default()),
            server_order: RwSignal::new(ServerOrder::default()),
            sidebar_state: RwSignal::new(SidebarState::default()),
        }
    }

    pub fn set_active_room_id(&self, room_id: Option<String>) {
        self.active_room_id.set(room_id.clone());

        let Some(room_id) = room_id else {
            return;
        };

        let key = self.active_server_id.get().unwrap_or("dms".to_string());

        self.breadcrums.update(|bc| {
            bc.last_space_ids.insert(key, room_id.clone());
        });

        self.append_room_id(room_id.clone());
        self.save_breadcrumbs();
    }

    pub fn set_active_server_id(&self, server_id: Option<String>) {
        self.active_server_id.set(server_id.clone());

        let key = server_id.clone().unwrap_or("dms".to_string());

        if let Some(room_id) = self.breadcrums.get().last_space_ids.get(&key).cloned() {
            self.active_room_id.set(Some(room_id.clone()));
            self.append_room_id(room_id);

            self.save_breadcrumbs();
        }
    }

    fn append_room_id(&self, room_id: String) {
        self.breadcrums.update(|bc| {
            bc.recent_rooms.retain(|id| id != &room_id);
            bc.recent_rooms.insert(0, room_id);

            if bc.recent_rooms.len() > 10 {
                bc.recent_rooms.pop();
            }
        });
    }

    fn save_breadcrumbs(&self) {
        let breadcrumbs = self.breadcrums.get();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&AccountDataArgs {
                payload: AccountDataPayload::Breadcrumbs(breadcrumbs),
            })
            .expect("Failed to serialize breadcrumbs");
            if let Err(err) = call_tauri("set_account_data", args).await {
                console_error(&format!("Error saving breadcrumbs: {:?}", err));
            }
        });
    }

    pub fn set_server_order(&self, servers: Vec<String>) {
        self.server_order.set(ServerOrder { servers });
        self.save_server_order();
    }

    fn save_server_order(&self) {
        let order = self.server_order.get_untracked();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&AccountDataArgs {
                payload: AccountDataPayload::ServerOrder(order),
            })
            .expect("Failed to serialize server order");
            if let Err(err) = call_tauri("set_account_data", args).await {
                console_error(&format!("Error saving server order: {:?}", err));
            }
        });
    }

    pub fn get_active_profile(
        &self,
        member_store: MemberStore,
    ) -> Option<ArcRwSignal<UserProfile>> {
        let Some(current_room_id) = self.active_room_id.get() else {
            return None;
        };

        for dm in self
            .sidebar_state
            .get()
            .dms
            .iter()
            .filter(|d| d.room_id == current_room_id)
        {
            if let Some(user_id) = &dm.dm_user_id {
                if user_id.is_empty() {
                    continue;
                }

                return Some(member_store.get_profile(&current_room_id, user_id));
            }
        }

        return None;
    }

    pub fn get_room_header(&self, member_store: MemberStore) -> RoomHeader {
        if let Some(profile) = self.get_active_profile(member_store) {
            return RoomHeader::DM(profile);
        }

        let Some(active_room_id) = self.active_room_id.get() else {
            return RoomHeader::Channel {
                name: "Unknown Room".to_string(),
            };
        };

        let sidebar_state = self.sidebar_state.get();
        let name = find_room_name_in_nodes(&sidebar_state.servers, &active_room_id)
            .or_else(|| find_room_name_in_nodes(&sidebar_state.orphaned_rooms, &active_room_id))
            .or_else(|| find_room_name_in_nodes(&sidebar_state.dms, &active_room_id))
            .unwrap_or("Unknown Room".to_string());

        RoomHeader::Channel { name }
    }
}

fn find_room_name_in_nodes(nodes: &[RoomNode], room_id: &str) -> Option<String> {
    for node in nodes {
        if node.room_id == room_id {
            return Some(
                node.name
                    .clone()
                    .unwrap_or_else(|| "Unknown Room".to_string()),
            );
        }

        if let RoomKind::Space { children } = &node.kind {
            if let Some(name) = find_room_name_in_nodes(children, room_id) {
                return Some(name);
            }
        }
    }

    None
}

#[derive(Default, Clone)]
pub struct MemberStore {
    pub rooms: RwSignal<HashMap<String, HashMap<String, ArcRwSignal<UserProfile>>>>,
    pub presences: RwSignal<HashMap<String, ArcRwSignal<PresenceInfo>>>,

    pub fetching: RwSignal<HashSet<String>>,
}

#[derive(Serialize, Debug)]
struct GetMembersArgs {
    room_id: String,
}

impl MemberStore {
    pub fn get_profile(&self, room_id: &String, user_id: &String) -> ArcRwSignal<UserProfile> {
        let existing_signal = self.rooms.with_untracked(|rooms| {
            rooms
                .get(room_id)
                .and_then(|users| users.get(user_id))
                .cloned()
        });

        if let Some(sig) = existing_signal {
            return sig;
        }

        let new_signal = ArcRwSignal::new(UserProfile {
            user_id: user_id.clone(),
            display_name: None,
            avatar_url: None,
        });

        self.rooms.update(|rooms| {
            rooms
                .entry(room_id.clone())
                .or_default()
                .insert(user_id.clone(), new_signal.clone());
        });

        let is_fetching = self
            .fetching
            .with_untracked(|fetching| fetching.contains(room_id));

        if !is_fetching {
            self.fetching.update(|f| {
                f.insert(room_id.clone());
            });

            let store = self.clone();
            let rid = room_id.clone();

            spawn_local(async move {
                let args = serde_wasm_bindgen::to_value(&GetMembersArgs {
                    room_id: rid.clone(),
                })
                .unwrap();

                if let Ok(js_val) = call_tauri("get_members", args).await {
                    let updates: HashMap<String, UserProfile> =
                        serde_wasm_bindgen::from_value(js_val).unwrap();

                    batch(move || {
                        store.rooms.update(|rooms| {
                            let room_entry = rooms.entry(rid.clone()).or_default();

                            for (user_id, profile) in updates.into_iter() {
                                let profile_signal = room_entry
                                    .entry(user_id.clone())
                                    .or_insert_with(|| ArcRwSignal::new(profile.clone()));

                                profile_signal.set(profile);
                            }
                        });
                        store.fetching.update(|f| {
                            f.remove(&rid);
                        });
                    });
                } else {
                    store.fetching.update(|f| {
                        f.remove(&rid);
                    });
                }
            });
        }

        new_signal
    }

    pub fn get_presence(&self, user_id: &String) -> ArcRwSignal<PresenceInfo> {
        let existing_signal = self.presences.with_untracked(|p| p.get(user_id).cloned());

        if let Some(sig) = existing_signal {
            return sig;
        }

        let new_signal = ArcRwSignal::new(PresenceInfo::default());

        self.presences.update(|presences| {
            presences.insert(user_id.clone(), new_signal.clone());
        });

        new_signal
    }
}
