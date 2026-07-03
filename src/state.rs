use std::collections::{HashMap, HashSet};

use leptos::task::spawn_local;
use log::error;
use serde_json::json;
use shared::{
    account_data::{Breadcrumbs, ServerOrder}, api::AudioDeviceInfos, profile::{CustomProperties, MemberProfile, PresenceInfo, RoomProfile, UserProfile}, sidebar::{DmList, DmRoomNode, NotificationCounts, RoomNode, ServerList, ServerRoomNode, SpaceRoomNode, UserDevice}, synth::ProfileAudio, timeline::UiMediaSource,
};

use crate::{
    app::{CurrentWindow, call_tauri},
    components::{chat::Attachment, user_profile::MemberProfileExt},
    tauri_functions::get_user_profile,
};
use leptos::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct MessageDraft {
    pub content: String,
    pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug, Copy, Default)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub previous_window: RwSignal<CurrentWindow>,
    pub last_changed_time: RwSignal<f64>,
    pub user_id: RwSignal<String>,

    pub active_room: RwSignal<Option<RoomNode>>,

    pub active_room_id: RwSignal<Option<String>>,
    pub active_server_id: RwSignal<Option<String>>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,
    pub data_initialized: RwSignal<bool>,

    pub room_map: RwSignal<HashMap<String, ArcRwSignal<RoomNode>>>,
    pub dm_list: RwSignal<DmList>,
    pub server_list: RwSignal<ServerList>,

    pub is_focused: RwSignal<bool>,

    pub drafts: RwSignal<HashMap<String, MessageDraft>>,

    pub lightbox_image: RwSignal<Option<LighboxImage>>,

    pub notification_counts: RwSignal<HashMap<String, NotificationCounts>>,
    pub call_members: RwSignal<HashMap<String, ArcRwSignal<Vec<UserDevice>>>>,

    pub typing_users: RwSignal<HashMap<String, ArcRwSignal<Vec<String>>>>,

    pub audio_devices: RwSignal<AudioDeviceInfos>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LighboxImage {
    pub name: String,
    pub sender_id: String,
    pub timestamp: u64,
    pub size: Option<u64>,
    pub source: UiMediaSource,
    pub origin_rect: Option<(f64, f64, f64, f64)>, // left, top, width, height of clicked thumbnail
    pub width: Option<u64>,
    pub height: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RoomHeader {
    Space(String),
    TextChannel(String),
    VoiceChannel(String),
    DM(ArcRwSignal<MemberProfile>),
    Unknown,
}

impl AppState {
    pub fn update_typing_users(&self, room_id: &str, user_ids: Vec<String>) {
        let existing_signal = self
            .typing_users
            .with_untracked(|map| map.get(room_id).cloned());

        if let Some(signal) = existing_signal {
            let should_update = signal.with_untracked(|current_users| current_users != &user_ids);

            if should_update {
                signal.set(user_ids.clone());
            }
        } else {
            let new_signal = ArcRwSignal::new(user_ids.clone());

            self.typing_users.update(|map| {
                map.insert(room_id.to_string(), new_signal);
            });
        }
    }

    pub fn get_typing_users(&self, room_id: &str) -> ArcRwSignal<Vec<String>> {
        if let Some(signal) = self
            .typing_users
            .with_untracked(|map| map.get(room_id).cloned())
        {
            signal
        } else {
            let new_signal = ArcRwSignal::new(Vec::new());
            self.typing_users.update(|map| {
                map.insert(room_id.to_string(), new_signal.clone());
            });
            new_signal
        }
    }

    pub fn active_room_id(&self) -> Option<String> {
        self.active_room_id.get()
    }

    pub fn active_room_id_untracked(&self) -> Option<String> {
        self.active_room_id.get_untracked()
    }

    pub fn active_room_name_untracked(&self) -> Option<String> {
        self.active_room.get_untracked().and_then(|room| room.name())
    }

    pub fn apply_server_order(&self) {
        let old_list = self.server_list.get_untracked();

        let new_list = old_list.apply_order(self.server_order.get_untracked());

        if new_list != old_list {
            self.server_list.set(new_list);
        }
    }

    pub fn reorder_servers(&self, source_id: &str, target_id: &str) {
        let old_list = self.server_list.get_untracked();

        let new_list = old_list.reorder_servers(source_id, target_id);

        if new_list != old_list {
            self.server_list.set(new_list);

            log::debug!("Reordered servers: {} -> {}", source_id, target_id);
            self.save_server_order();
        }
    }

    pub fn get_room_sig(&self, room_id: &str) -> Option<ArcRwSignal<RoomNode>> {
        let sidebar_state = self.room_map.get();

        sidebar_state.get(room_id).cloned()
    }

    pub fn get_room_profiles_in_active_server(&self) -> Vec<RoomProfile> {
        let Some(server_id) = self.active_server_id.get_untracked() else {
            return Vec::new();
        };

        let Some(RoomNode::Server(ServerRoomNode {all_children, ..})) = self.get_room_sig(&server_id).map(|sig| sig.get_untracked())
        else {
            return Vec::new();
        };

        self.room_map
            .get_untracked()
            .into_iter()
            .filter_map(|(room_id, room)| {
                if all_children.contains(&room_id) {
                    room.try_get_untracked().map(RoomProfile::from)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Update the active room node and its id together, firing each signal only
    /// when its own value changed. `active_room` carries metadata that updates on
    /// every sync (unread counts, `last_ts`, …); `active_room_id` only changes
    /// when a different room becomes active, so id-only subscribers (the timeline
    /// loader) don't react to that churn.
    fn set_active_room_node(&self, node: Option<RoomNode>) {
        let new_id = node.as_ref().map(|n| n.room_id().to_string());

        if self.active_room_id.with_untracked(|cur| cur != &new_id) {
            self.active_room_id.set(new_id);
        }

        if self.active_room.with_untracked(|cur| cur != &node) {
            self.active_room.set(node);
        }
    }

    pub fn set_active_room_with_id(&self, room_id: Option<String>) {
        let active_room = if let Some(room_id) = &room_id {
            if let Some(node) = self.room_map.get_untracked().get(room_id).cloned() {
                node.try_get_untracked()
            } else {
                None
            }
        } else {
            None
        };
        self.set_active_room_node(active_room);

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
            self.set_active_room_with_id(Some(room_id.clone()));
        }
    }

    fn first_channel_id_for_server(&self, server_id: &str) -> Option<String> {
        let server = self.room_map.get_untracked().get(server_id)?.try_get_untracked()?;

        if let RoomNode::Server(ServerRoomNode { children, .. }) = &server {
            children.first().cloned()
        } else {
            None
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
        if !self.data_initialized.get_untracked() {
            return;
        }
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
        if !self.data_initialized.get_untracked() {
            return;
        }
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

    pub fn get_room_header(&self, member_store: ProfileStore) -> RoomHeader {
        let Some(room) = self.active_room.get() else {
            return RoomHeader::Unknown;
        };

        let active_room_id = room.room_id().to_string();

        match &room {
            RoomNode::Dm(DmRoomNode { other_user_id, .. }) => {
                let profile = member_store.get_member_profile(&active_room_id, other_user_id);

                RoomHeader::DM(profile)
            }
            RoomNode::Single(_) => RoomHeader::TextChannel(room.display_name()),
            RoomNode::TextChannel(_) => RoomHeader::TextChannel(room.display_name()),
            RoomNode::VoiceChannel(_) => RoomHeader::VoiceChannel(room.display_name()),
            RoomNode::Space(_) => RoomHeader::Space(room.display_name()),
            RoomNode::Server(_) => RoomHeader::Space(room.display_name()),
        }
    }

    pub fn update_active_room(&self) {
        let current_room_id = self.active_room_id_untracked().or_else(|| {
            self.breadcrums
                .get_untracked()
                .recent_rooms
                .first()
                .cloned()
        });

        let new_active_room = if let Some(room_id) = current_room_id {
            self.get_room_sig(&room_id).map(|sig| sig.get_untracked())
        } else {
            None
        };

        self.set_active_room_node(new_active_room);
    }

    pub fn find_room_in_rooms(
        &self,
        room_ids_to_search: &[String],
        target_room_id: &str,
    ) -> Option<RoomNode> {
        for room_id in room_ids_to_search {
            let room_node = self.room_map.get_untracked().get(room_id)?.try_get_untracked()?;

            if room_node.room_id() == target_room_id {
                return Some(room_node);
            }

            match room_node {
                RoomNode::Server(ServerRoomNode { children, .. })
                | RoomNode::Space(SpaceRoomNode { children, .. }) => {
                    if let Some(found_room) = self.find_room_in_rooms(&children, target_room_id) {
                        return Some(found_room);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Finds the top-level server room_id that contains `room_id` by searching
    /// the sidebar directly. Returns `None` if the room is a DM or not found.
    pub fn find_server_id_for_room(&self, room_id: &str) -> Option<String> {
        for server_id in self.server_list.get_untracked().0 {
            if server_id == room_id {
                return Some(server_id.clone());
            }

            let server = self.room_map.get_untracked().get(&server_id)?.try_get_untracked()?;

            if let RoomNode::Server(ServerRoomNode { children, .. }) = server
                && self.find_room_in_rooms(&children, room_id).is_some()
            {
                return Some(server_id.clone());
            }
        }
        None
    }

    /// Update the hashmap of call members with new data. Update signals for the room_ids
    pub fn update_call_members(&self, update: HashMap<String, Vec<UserDevice>>) {
        self.call_members.update(|members| {
            for (room_id, devices) in update.iter() {
                let device_signal = members
                    .entry(room_id.clone())
                    .or_insert_with(|| ArcRwSignal::new(Vec::new()));
                device_signal.set(devices.clone());
            }
        });
    }

    pub fn get_call_members(&self, room_id: &str) -> ArcRwSignal<Vec<UserDevice>> {
        self.call_members
            .get()
            .entry(room_id.to_string())
            .or_insert_with(|| ArcRwSignal::new(Vec::new()))
            .clone()
    }

    /// Get call members for a set of room ids.
    pub fn get_call_members_in_rooms(&self, room_ids: HashSet<String>) -> HashSet<String> {
        let members = self.call_members.get();
        room_ids
            .into_iter()
            .filter_map(|room_id| {
                let call_member_sig = members.get(&room_id)?;

                let call_member_list = call_member_sig.get();
                if call_member_list.is_empty() {
                    None
                } else {
                    let list: HashSet<String> =
                        call_member_list.iter().map(|d| d.user_id.clone()).collect();
                    Some(list)
                }
            })
            .flatten()
            .collect()
    }

    pub fn get_rooms(&self) -> Vec<RoomNode> {
        self.room_map
            .get()
            .values()
            .filter_map(|sig| sig.try_get())
            .collect()
    }

    pub fn get_room(&self, room_id: &str) -> Option<RoomNode> {
        self.room_map
            .get()
            .get(room_id)
            .and_then(|sig| sig.try_get())
    }

    // pub fn get_room_untracked(&self, room_id: &str) -> Option<RoomNode> {
    //     let sidebar_state = self.sidebar_state.get_untracked();

    //     sidebar_state
    //         .server_rooms
    //         .get(room_id)
    //         .cloned()
    //         .or_else(|| sidebar_state.dms.iter().find(|dm| dm.room_id == room_id).cloned())
    // }
}

type MemberStoreRoomEntry = HashMap<String, ArcRwSignal<MemberProfile>>;

#[derive(Default, Clone)]
pub struct ProfileStore {
    pub members: ArcRwSignal<HashMap<String, MemberStoreRoomEntry>>,
    pub presences: RwSignal<HashMap<String, ArcRwSignal<PresenceInfo>>>,

    pub user_profiles: ArcRwSignal<HashMap<String, ArcRwSignal<UserProfile>>>,
    default_profile_audio: ProfileAudio,
}

impl ProfileStore {
    pub fn room_as_profile(&self, room_id: &str) -> MemberProfile {
        MemberProfile {
            room_id: room_id.to_string(),
            profile: UserProfile {
                user_id: room_id.to_string(),
                display_name: Some("room".to_string()),
                has_avatar: false,
                custom_properties: CustomProperties::from_user_id(
                    room_id,
                    self.default_profile_audio.clone(),
                ),
            },
        }
    }

    pub fn get_member_profile(&self, room_id: &str, user_id: &str) -> ArcRwSignal<MemberProfile> {
        if room_id == user_id {
            return ArcRwSignal::new(self.room_as_profile(room_id));
        }

        let existing_signal = self.members.with_untracked(|rooms| {
            rooms
                .get(room_id)
                .and_then(|users| users.get(user_id))
                .cloned()
        });

        if let Some(sig) = existing_signal {
            return sig;
        }

        let sig = ArcRwSignal::new(MemberProfile {
            room_id: room_id.to_string(),
            profile: UserProfile {
                user_id: user_id.to_string(),
                display_name: None,
                has_avatar: false,
                custom_properties: CustomProperties::from_user_id(
                    user_id,
                    self.default_profile_audio.clone(),
                ),
            },
        });

        self.members.update(|members| {
            members
                .entry(room_id.to_string())
                .or_insert_with(HashMap::new)
                .insert(user_id.to_string(), sig.clone());
        });

        sig
    }

    pub fn get_presence(&self, user_id: &str) -> ArcRwSignal<PresenceInfo> {
        let existing_signal = self.presences.with_untracked(|p| p.get(user_id).cloned());

        if let Some(sig) = existing_signal {
            return sig;
        }

        let new_signal = ArcRwSignal::new(PresenceInfo::default());

        self.presences.update(|presences| {
            presences.insert(user_id.to_string(), new_signal.clone());
        });

        new_signal
    }

    pub fn get_user_profile(&self, user_id: &str) -> ArcRwSignal<UserProfile> {
        let existing_signal = self
            .user_profiles
            .with_untracked(|p| p.get(user_id).cloned());

        if let Some(sig) = existing_signal {
            return sig;
        }

        let sig = ArcRwSignal::new(UserProfile {
            user_id: user_id.to_string(),
            display_name: None,
            has_avatar: false,
            custom_properties: CustomProperties::from_user_id(
                user_id,
                self.default_profile_audio.clone(),
            ),
        });

        self.user_profiles.update(|profiles| {
            profiles.insert(user_id.to_string(), sig.clone());
        });

        let user_id = user_id.to_string();
        let sig_clone = sig.clone();
        spawn_local(async move {
            match get_user_profile(&user_id).await {
                Ok(res) => {
                    sig_clone.set(res);
                }
                Err(e) => {
                    log::error!("Failed to get user profile: {e}");
                }
            };
        });

        sig
    }

    pub fn get_profile_signal(&self, room_id: Option<String>, user_id: &str) -> ProfileSignal {
        if let Some(room_id) = room_id {
            ProfileSignal::Member(self.get_member_profile(&room_id, user_id))
        } else {
            ProfileSignal::User(self.get_user_profile(user_id))
        }
    }

    pub fn get_members(self, room_id: &str) -> Vec<MemberProfile> {
        self.members
            .get()
            .get(room_id)
            .map(|m| m.values().map(|s| s.get()).collect())
            .unwrap_or_default()
    }

    pub fn get_member_signals(self, room_id: &str) -> HashMap<String, ArcRwSignal<MemberProfile>> {
        self.members.get().get(room_id).cloned().unwrap_or_default()
    }
}

#[derive(Clone, PartialEq)]
pub enum ProfileSignal {
    User(ArcRwSignal<UserProfile>),
    Member(ArcRwSignal<MemberProfile>),
}

impl ProfileSignal {
    pub fn banner_color(&self) -> String {
        match self {
            ProfileSignal::User(sig) => sig.get().banner_color().to_css_hsl(),
            ProfileSignal::Member(sig) => sig.get().profile.name_color().to_css_hsl(),
        }
    }

    pub fn audio(&self) -> ProfileAudio {
        match self {
            ProfileSignal::User(sig) => sig.get().get_audio(),
            ProfileSignal::Member(sig) => sig.get().get_audio(),
        }
    }

    pub fn icon(self, size_str: String) -> impl IntoView {
        match self {
            ProfileSignal::User(sig) => sig.get().render_icon(size_str).into_any(),
            ProfileSignal::Member(sig) => sig.get().render_icon(size_str).into_any(),
        }
    }

    pub fn name(self, font_size_str: String) -> impl IntoView {
        match self {
            ProfileSignal::User(sig) => sig.get().render_name_popup(font_size_str).into_any(),
            ProfileSignal::Member(sig) => sig.get().render_name_popup(font_size_str).into_any(),
        }
    }
}
