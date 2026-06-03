use std::collections::{HashMap, HashSet};

use leptos::task::spawn_local;
use log::error;
use serde_json::json;
use shared::{
    account_data::{Breadcrumbs, ServerOrder},
    sidebar::{RoomKind, RoomNode, SidebarState},
    timeline::UiMediaSource,
    user_profile::{PresenceInfo, UserProfile},
};

use crate::{
    app::{CurrentWindow, call_tauri},
    components::chat::Attachment,
    tauri_functions::get_members_for_room,
};
use leptos::prelude::*;

#[derive(Clone, Debug)]
pub struct MessageDraft {
    pub content: String,
    pub attachments: Vec<Attachment>,
}

impl Default for MessageDraft {
    fn default() -> Self {
        Self {
            content: "<br>".to_string(),
            attachments: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Copy)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub previous_window: RwSignal<CurrentWindow>,
    pub last_changed_time: RwSignal<f64>,
    pub user_id: RwSignal<String>,

    pub active_room: RwSignal<Option<RoomNode>>,
    pub active_server_id: RwSignal<Option<String>>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,

    pub sidebar_state: RwSignal<SidebarState>,

    pub is_focused: RwSignal<bool>,

    pub drafts: RwSignal<HashMap<String, MessageDraft>>,

    pub lightbox_image: RwSignal<Option<LighboxImage>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LighboxImage {
    pub name: String,
    pub sender_id: Option<String>,
    pub timestamp: u64,
    pub size: Option<u64>,
    pub source: UiMediaSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemberProfileHandle {
    pub user_id: String,
    pub profile: ArcRwSignal<Option<UserProfile>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoomHeader {
    Space(String),
    TextChannel(String),
    VoiceChannel(String),
    DM(ArcRwSignal<Option<UserProfile>>),
    Unknown,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            current_window: RwSignal::new(CurrentWindow::Loading),
            previous_window: RwSignal::new(CurrentWindow::Loading),
            last_changed_time: RwSignal::new(0.0),
            user_id: RwSignal::new(String::new()),
            active_room: RwSignal::new(None),
            active_server_id: RwSignal::new(None),
            breadcrums: RwSignal::new(Breadcrumbs::default()),
            server_order: RwSignal::new(ServerOrder::default()),
            sidebar_state: RwSignal::new(SidebarState::default()),
            is_focused: RwSignal::new(true),
            drafts: RwSignal::new(HashMap::new()),
            lightbox_image: RwSignal::new(None),
        }
    }

    pub fn active_room_id(&self) -> Option<String> {
        self.active_room.get().map(|room| room.room_id.clone())
    }

    pub fn active_room_id_untracked(&self) -> Option<String> {
        self.active_room
            .get_untracked()
            .map(|room| room.room_id.clone())
    }

    pub fn set_active_room_with_id(&self, room_id: Option<String>) {
        let active_room = room_id.as_ref().and_then(|id| {
            find_node_in_nodes(&self.sidebar_state.get_untracked().servers, id)
                .cloned()
                .or_else(|| {
                    self.sidebar_state
                        .get_untracked()
                        .dms
                        .iter()
                        .find(|dm| dm.room_id == *id)
                        .cloned()
                })
        });
        self.active_room.set(active_room);

        let Some(room_id) = room_id else {
            return;
        };

        let key = self
            .active_server_id
            .get_untracked()
            .unwrap_or("dms".to_string());

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
            self.set_active_room_with_id(Some(room_id.clone()));
            self.append_room_id(room_id);

            self.save_breadcrumbs();
            return;
        }

        let Some(server_id) = server_id.as_deref() else {
            return;
        };

        if let Some(room_id) = self.first_channel_id_for_server(server_id) {
            self.set_active_room_with_id(Some(room_id));
        }
    }

    fn first_channel_id_for_server(&self, server_id: &str) -> Option<String> {
        let state = self.sidebar_state.get();
        let server = state.servers.iter().find(|srv| srv.room_id == server_id)?;

        match &server.kind {
            RoomKind::Space { children, .. } => Self::find_first_channel(children),
            RoomKind::TextChannel { .. } => Some(server.room_id.clone()),
            RoomKind::Dm { .. } => Some(server.room_id.clone()),
            RoomKind::VoiceChannel { .. } => Some(server.room_id.clone()),
        }
    }

    fn find_first_channel(nodes: &[RoomNode]) -> Option<String> {
        for node in nodes {
            match &node.kind {
                RoomKind::TextChannel { .. } => return Some(node.room_id.clone()),
                RoomKind::Space { children, .. } => {
                    if let Some(room_id) = Self::find_first_channel(children) {
                        return Some(room_id);
                    }
                }
                RoomKind::Dm { .. } => return Some(node.room_id.clone()),
                RoomKind::VoiceChannel { .. } => return Some(node.room_id.clone()),
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
        let breadcrumbs = self.breadcrums.get_untracked();
        spawn_local(async move {
            let args = serde_wasm_bindgen::to_value(&json!({
                "breadcrumbs": breadcrumbs
            }))
            .expect("Failed to serialize breadcrumbs");

            if let Err(err) = call_tauri("set_breadcrumbs", args).await {
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
            let args = serde_wasm_bindgen::to_value(&json!({
                "server_order": order
            }))
            .expect("Failed to serialize server order");

            if let Err(err) = call_tauri("set_server_order", args).await {
                error!("Error saving server order: {:?}", err);
            }
        });
    }

    pub fn get_room_header(&self, member_store: MemberStore) -> RoomHeader {
        let Some(room) = self.active_room.get() else {
            return RoomHeader::Unknown;
        };

        let active_room_id = room.room_id.clone();

        match &room.kind {
            RoomKind::Dm { other_user_ids, .. } => {
                let Some(other_user_id) = other_user_ids.first() else {
                    return RoomHeader::Unknown;
                };
                let profile = member_store.get_profile(&active_room_id, other_user_id);

                RoomHeader::DM(profile)
            }
            RoomKind::TextChannel { .. } => RoomHeader::TextChannel(room.get_name()),
            RoomKind::VoiceChannel { .. } => RoomHeader::VoiceChannel(room.get_name()),
            RoomKind::Space { .. } => RoomHeader::Space(room.get_name()),
        }
    }

    pub fn update_active_room(&self) {
        let current_room_id = self.active_room_id_untracked();

        if let Some(room_id) = current_room_id {
            let sidebar_state = self.sidebar_state.get();
            let active_room = find_node_in_nodes(&sidebar_state.servers, &room_id)
                .cloned()
                .or_else(|| {
                    sidebar_state
                        .dms
                        .iter()
                        .find(|dm| dm.room_id == room_id)
                        .cloned()
                });

            self.active_room.set(active_room);
        } else {
            self.active_room.set(None);
        }
    }
}

fn find_node_in_nodes<'a>(nodes: &'a [RoomNode], room_id: &str) -> Option<&'a RoomNode> {
    for node in nodes {
        if node.room_id == room_id {
            return Some(node);
        }

        if let RoomKind::Space { children, .. } = &node.kind
            && let Some(found) = find_node_in_nodes(children, room_id)
        {
            return Some(found);
        }
    }

    None
}

type MemberStoreRoomEntry = HashMap<String, ArcRwSignal<Option<UserProfile>>>;

#[derive(Default, Clone)]
pub struct MemberStore {
    pub rooms: RwSignal<HashMap<String, MemberStoreRoomEntry>>,
    pub presences: RwSignal<HashMap<String, ArcRwSignal<PresenceInfo>>>,

    pub fetching: RwSignal<HashSet<String>>,
}

impl MemberStore {
    pub fn get_profile(&self, room_id: &str, user_id: &str) -> ArcRwSignal<Option<UserProfile>> {
        if room_id.is_empty() {
            return ArcRwSignal::new(None);
        }

        if room_id == user_id {
            return ArcRwSignal::new(Some(UserProfile {
                display_name: Some("room".into()),
                user_id: room_id.to_string(),
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
                .entry(room_id.to_string())
                .or_default()
                .insert(user_id.to_string(), new_signal.clone());
        });

        let is_fetching = self
            .fetching
            .with_untracked(|fetching| fetching.contains(room_id));

        if !is_fetching {
            self.fetching.update(|f| {
                f.insert(room_id.to_string());
            });

            let store = self.clone();
            let rid = room_id.to_string();

            spawn_local(async move {
                match get_members_for_room(&rid).await {
                    Ok(members) => {
                        store.rooms.update(|rooms| {
                            let room_entry = rooms.entry(rid.to_string()).or_default();

                            for profile in members.into_iter() {
                                let profile_signal = room_entry
                                    .entry(profile.user_id.clone())
                                    .or_insert_with(|| ArcRwSignal::new(Some(profile.clone())));

                                profile_signal.set(Some(profile));
                            }
                        });
                    }
                    Err(err) => {
                        error!("Failed to fetch members for room {}: {:?}", rid, err);
                    }
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
