use std::collections::{HashMap, HashSet};

use leptos::task::spawn_local;
use log::error;
use serde_json::json;
use shared::{
    account_data::{Breadcrumbs, ServerOrder},
    api::{AudioDeviceInfos, SearchParameters, UpdateDownloadProgress, UpdateStatus, events::RecentEmojies},
    profile::{CustomProperties, MemberProfile, PresenceInfo, RoomProfile, UserProfile},
    sidebar::{
        DmList, NotificationCounts, RoomNode, ServerList, ServerRoomNode, SingleList,
        SpaceRoomNode, UserDevice,
    },
    synth::SonicSignature,
    timeline::{UiMediaSource, UiTimelineItem},
};

use crate::{
    app::{CurrentWindow, call_tauri},
    components::{chat::Attachment, user_profile::MemberProfileExt},
    tauri_functions::get_user_profile,
};
use leptos::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct RoomState {
    pub content: String,
    pub attachments: Vec<Attachment>,
    pub search_parameters: Option<SearchParameters>,
    pub search_results: Option<HashMap<String, Vec<UiTimelineItem>>>,
    pub pinned_result: Option<Vec<UiTimelineItem>>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum CurrentSection {
    Server(String),
    #[default]
    Dms,
    Single,
}

impl CurrentSection {
    pub fn key(&self) -> String {
        match self {
            CurrentSection::Server(server_id) => server_id.clone(),
            CurrentSection::Dms => "dms".to_string(),
            CurrentSection::Single => "single".to_string(),
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key {
            "dms" => CurrentSection::Dms,
            "single" => CurrentSection::Single,
            server_id => CurrentSection::Server(server_id.to_string()),
        }
    }

    pub fn is_not_server(&self) -> bool {
        !matches!(self, CurrentSection::Server(_))
    }
}

#[derive(Clone, Debug, Copy, Default)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub previous_window: RwSignal<CurrentWindow>,
    pub last_changed_time: RwSignal<f64>,
    pub user_id: RwSignal<String>,

    pub active_room: RwSignal<Option<RoomNode>>,

    pub active_room_id: RwSignal<Option<String>>,
    pub active_section: RwSignal<CurrentSection>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,
    pub data_initialized: RwSignal<bool>,

    pub room_map: RwSignal<HashMap<String, ArcRwSignal<RoomNode>>>,
    pub dm_list: RwSignal<DmList>,
    pub single_room_list: RwSignal<SingleList>,
    pub server_list: RwSignal<ServerList>,

    pub pinned_map: RwSignal<HashMap<String, Vec<String>>>,

    pub is_focused: RwSignal<bool>,

    pub room_states: RwSignal<HashMap<String, RoomState>>,

    pub lightbox_image: RwSignal<Option<LighboxImage>>,

    pub notification_counts: RwSignal<HashMap<String, NotificationCounts>>,
    pub call_members: RwSignal<HashMap<String, ArcRwSignal<Vec<UserDevice>>>>,

    pub typing_users: RwSignal<HashMap<String, ArcRwSignal<Vec<String>>>>,

    pub audio_devices: RwSignal<AudioDeviceInfos>,

    pub recent_emojies: RwSignal<RecentEmojies>,

    pub update_progress: RwSignal<UpdateDownloadProgress>,
    pub update_status: RwSignal<UpdateStatus>,

    pub app_version: RwSignal<String>,
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
        self.active_room.get_untracked().map(|room| room.name())
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
        let CurrentSection::Server(server_id) = self.active_section.get_untracked() else {
            return Vec::new();
        };

        let Some(RoomNode::Server(ServerRoomNode { all_children, .. })) =
            self.get_room_sig(&server_id).map(|sig| sig.get_untracked())
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
    fn set_active_room_node(&self, room_id: Option<String>, node: Option<RoomNode>) {
        if self.active_room_id.with_untracked(|cur| cur != &room_id) {
            self.active_room_id.set(room_id);
        }

        if self.active_room.with_untracked(|cur| cur != &node) {
            self.active_room.set(node);
        }
    }

    pub fn set_active_room_with_id(&self, room_id: Option<String>) {
        log::debug!(
            "Changing active room to {}",
            room_id.clone().unwrap_or("no room".into())
        );

        let active_room = if let Some(room_id) = &room_id {
            let node = self.room_map.get_untracked().get(room_id).cloned();

            node.and_then(|sig| sig.try_get_untracked())
        } else {
            None
        };
        self.set_active_room_node(room_id.clone(), active_room);

        let Some(room_id) = room_id else {
            return;
        };

        let key = self.active_section.get_untracked().key();

        self.breadcrums.update(|bc| {
            bc.last_space_ids.insert(key, room_id.clone());
        });

        self.append_room_id(room_id.clone());
        self.save_breadcrumbs();
    }

    pub fn set_active_section(&self, section: CurrentSection) {
        self.active_section.set(section.clone());

        if matches!(section, CurrentSection::Dms) {
            self.breadcrums.update(|bc| {
                bc.dms_last = true;
            });
        } else if matches!(section, CurrentSection::Single) {
            self.breadcrums.update(|bc| {
                bc.dms_last = false;
            });
        }

        let key = section.key();

        let cached_room_id = self
            .breadcrums
            .get_untracked()
            .last_space_ids
            .get(&key)
            .cloned()
            .filter(|room_id| self.room_belongs_to_section(room_id, &section));

        if let Some(room_id) = cached_room_id {
            self.set_active_room_with_id(Some(room_id.clone()));
            self.append_room_id(room_id);
            return;
        }

        match &section {
            CurrentSection::Dms => {
                if let Some(room_id) = self.dm_list.get_untracked().0.first().cloned() {
                    self.set_active_room_with_id(Some(room_id.clone()));
                    self.append_room_id(room_id);
                } else {
                    log::warn!("DM list is empty, cannot set active room");
                }
            }
            CurrentSection::Single => {
                if let Some(room_id) = self.single_room_list.get_untracked().0.first().cloned() {
                    self.set_active_room_with_id(Some(room_id.clone()));
                    self.append_room_id(room_id);
                } else {
                    log::warn!("Single room list is empty, cannot set active room");
                }
            }
            CurrentSection::Server(server_id) => {
                if let Some(room_id) = self.first_channel_id_for_server(server_id) {
                    self.set_active_room_with_id(Some(room_id.clone()));
                    self.append_room_id(room_id);
                } else {
                    log::warn!("Server {} has no channels to set as active room", server_id);
                }
            }
        }
    }

    fn first_channel_id_for_server(&self, server_id: &str) -> Option<String> {
        let map = self.room_map.get_untracked();

        let server = map.get(server_id)?.try_get_untracked()?;

        let res = match server {
            RoomNode::Server(ServerRoomNode { children, .. })
            | RoomNode::Space(SpaceRoomNode { children, .. }) => {
                for id in children {
                    if let Some(sig) = map.get(&id)
                        && let Some(node) = sig.try_get_untracked()
                        && !node.is_unjoined()
                    {
                        return Some(id.clone());
                    }
                }

                return None;
            }
            _ => None,
        };

        if res.is_none() {
            log::warn!("Server {} has no children", server_id);
        }
        res
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

    fn room_belongs_to_section(&self, room_id: &str, section: &CurrentSection) -> bool {
        match section {
            CurrentSection::Dms => self
                .dm_list
                .get_untracked()
                .0
                .iter()
                .any(|id| id == room_id),
            CurrentSection::Single => self
                .single_room_list
                .get_untracked()
                .0
                .iter()
                .any(|id| id == room_id),
            CurrentSection::Server(server_id) => {
                server_id == room_id
                    || self.find_server_id_for_room(room_id).as_deref() == Some(server_id.as_str())
            }
        }
    }

    pub fn section_for_room(&self, room_id: &str) -> CurrentSection {
        let recorded_section = self
            .breadcrums
            .get_untracked()
            .last_space_ids
            .iter()
            .find(|(_, last_room_id)| last_room_id.as_str() == room_id)
            .map(|(key, _)| CurrentSection::from_key(key));

        if let Some(section) = recorded_section {
            return section;
        }

        if self
            .dm_list
            .get_untracked()
            .0
            .iter()
            .any(|id| id == room_id)
        {
            CurrentSection::Dms
        } else if self
            .single_room_list
            .get_untracked()
            .0
            .iter()
            .any(|id| id == room_id)
        {
            CurrentSection::Single
        } else if let Some(server_id) = self.find_server_id_for_room(room_id) {
            CurrentSection::Server(server_id)
        } else {
            CurrentSection::default()
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

        let new_active_room = if let Some(room_id) = &current_room_id {
            self.get_room_sig(room_id).map(|sig| sig.get_untracked())
        } else {
            None
        };

        if let Some(room_id) = &current_room_id {
            let section = self.section_for_room(room_id);
            if self.active_section.get_untracked() != section {
                self.active_section.set(section);
            }
        }

        self.set_active_room_node(current_room_id, new_active_room);
    }

    pub fn find_room_in_rooms(
        &self,
        room_ids_to_search: &[String],
        target_room_id: &str,
    ) -> Option<RoomNode> {
        for room_id in room_ids_to_search {
            let room_node = self
                .room_map
                .get_untracked()
                .get(room_id)?
                .try_get_untracked()?;

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

            let server = self
                .room_map
                .get_untracked()
                .get(&server_id)?
                .try_get_untracked()?;

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

    pub fn get_room_untracked(&self, room_id: &str) -> Option<RoomNode> {
        self.room_map
            .get_untracked()
            .get(room_id)
            .and_then(|sig| sig.try_get())
    }
}

type MemberStoreRoomEntry = HashMap<String, ArcRwSignal<MemberProfile>>;

#[derive(Default, Clone)]
pub struct ProfileStore {
    pub members: ArcRwSignal<HashMap<String, MemberStoreRoomEntry>>,
    pub presences: RwSignal<HashMap<String, ArcRwSignal<PresenceInfo>>>,

    pub user_profiles: ArcRwSignal<HashMap<String, ArcRwSignal<UserProfile>>>,
}

impl ProfileStore {
    pub fn room_as_profile(&self, room_id: &str) -> MemberProfile {
        MemberProfile {
            room_id: room_id.to_string(),
            profile: UserProfile {
                user_id: room_id.to_string(),
                display_name: Some("room".to_string()),
                has_avatar: false,
                custom_properties: CustomProperties::from_user_id(room_id),
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
                custom_properties: CustomProperties::from_user_id(user_id),
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
            custom_properties: CustomProperties::from_user_id(user_id),
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

    pub fn signature(&self) -> SonicSignature {
        match self {
            ProfileSignal::User(sig) => sig.get().get_signature(),
            ProfileSignal::Member(sig) => sig.get().get_signature(),
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

    pub fn name_no_popup(self, font_size_str: String) -> impl IntoView {
        match self {
            ProfileSignal::User(sig) => sig.get().render_name_no_popup(font_size_str).into_any(),
            ProfileSignal::Member(sig) => sig.get().render_name_no_popup(font_size_str).into_any(),
        }
    }
}
