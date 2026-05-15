use std::collections::{HashMap, HashSet};

use leptos::task::spawn_local;
use log::error;
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
    pub previous_window: RwSignal<CurrentWindow>,
    pub last_changed_time: RwSignal<f64>,
    pub user_id: RwSignal<String>,

    pub active_room_id: RwSignal<Option<String>>,
    pub active_server_id: RwSignal<Option<String>>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,

    pub sidebar_state: RwSignal<SidebarState>,

    pub is_focused: RwSignal<bool>,

    pub drafts: RwSignal<HashMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemberProfileHandle {
    pub user_id: String,
    pub profile: ArcRwSignal<Option<UserProfile>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoomHeader {
    Channel(RoomNode),
    DM(MemberProfileHandle),
    Unknown,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_window: RwSignal::new(CurrentWindow::LoadingPage),
            previous_window: RwSignal::new(CurrentWindow::LoadingPage),
            last_changed_time: RwSignal::new(0.0),
            user_id: RwSignal::new(String::new()),
            active_room_id: RwSignal::new(None),
            active_server_id: RwSignal::new(None),
            breadcrums: RwSignal::new(Breadcrumbs::default()),
            server_order: RwSignal::new(ServerOrder::default()),
            sidebar_state: RwSignal::new(SidebarState::default()),
            is_focused: RwSignal::new(true),
            drafts: RwSignal::new(HashMap::new()),
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
            return;
        }

        let Some(server_id) = server_id.as_deref() else {
            return;
        };

        if let Some(room_id) = self.first_channel_id_for_server(server_id) {
            self.set_active_room_id(Some(room_id));
        }
    }

    fn first_channel_id_for_server(&self, server_id: &str) -> Option<String> {
        let state = self.sidebar_state.get();
        let server = state.servers.iter().find(|srv| srv.room_id == server_id)?;

        match &server.kind {
            RoomKind::Space { children } => Self::find_first_channel(children),
            RoomKind::Channel { .. } => Some(server.room_id.clone()),
        }
    }

    fn find_first_channel(nodes: &[RoomNode]) -> Option<String> {
        for node in nodes {
            match &node.kind {
                RoomKind::Channel { .. } => return Some(node.room_id.clone()),
                RoomKind::Space { children } => {
                    if let Some(room_id) = Self::find_first_channel(children) {
                        return Some(room_id);
                    }
                }
            }
        }

        None
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
                error!("Error saving breadcrumbs: {:?}", err);
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
                error!("Error saving server order: {:?}", err);
            }
        });
    }

    pub fn get_active_profile(&self, member_store: MemberStore) -> Option<MemberProfileHandle> {
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

                return Some(MemberProfileHandle {
                    user_id: user_id.clone(),
                    profile: member_store.get_profile(&current_room_id, user_id),
                });
            }
        }

        return None;
    }

    pub fn get_room_header(&self, member_store: MemberStore) -> RoomHeader {
        if let Some(profile) = self.get_active_profile(member_store) {
            return RoomHeader::DM(profile);
        }

        let Some(active_room_id) = self.active_room_id.get() else {
            return RoomHeader::Unknown;
        };

        let sidebar_state = self.sidebar_state.get();
        let Some(node) = find_node_in_nodes(&sidebar_state.servers, &active_room_id) else {
            return RoomHeader::Unknown;
        };

        RoomHeader::Channel(node.clone())
    }
}

fn find_node_in_nodes<'a>(nodes: &'a [RoomNode], room_id: &str) -> Option<&'a RoomNode> {
    for node in nodes {
        if node.room_id == room_id {
            return Some(node);
        }

        if let RoomKind::Space { children } = &node.kind {
            if let Some(found) = find_node_in_nodes(children, room_id) {
                return Some(found);
            }
        }
    }

    None
}

#[derive(Default, Clone)]
pub struct MemberStore {
    pub rooms: RwSignal<HashMap<String, HashMap<String, ArcRwSignal<Option<UserProfile>>>>>,
    pub presences: RwSignal<HashMap<String, ArcRwSignal<PresenceInfo>>>,

    pub fetching: RwSignal<HashSet<String>>,
}

#[derive(Serialize, Debug)]
struct GetMembersArgs {
    room_id: String,
}

impl MemberStore {
    pub fn get_profile(
        &self,
        room_id: &String,
        user_id: &String,
    ) -> ArcRwSignal<Option<UserProfile>> {
        if room_id == user_id {
            return ArcRwSignal::new(Some(UserProfile {
                display_name: Some("room".into()),
                user_id: room_id.clone(),
                avatar_url: None,
            }));
        }

        let existing_signal = self.rooms.with_untracked(|rooms| {
            rooms
                .get(room_id)
                .and_then(|users| users.get(user_id))
                .cloned()
        });

        if let Some(sig) = existing_signal {
            return sig;
        }

        let new_signal = ArcRwSignal::new(None);

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
                                    .or_insert_with(|| ArcRwSignal::new(Some(profile.clone())));

                                profile_signal.set(Some(profile));
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
