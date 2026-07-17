use std::collections::{HashMap, HashSet};

use leptos::task::spawn_local;
use log::error;
use ruma::{
    MxcUri, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedUserId, RoomId, UserId,
    events::room::MediaSource,
};
use serde_json::json;
use shared::{
    UiThumbnailMethod, UiThumbnailSettings,
    account_data::{Breadcrumbs, ServerOrder},
    api::{
        AudioDeviceInfos, SearchParameters, UpdateDownloadProgress, UpdateStatus,
        events::RecentEmojies,
    },
    profile::{CustomProperties, MemberProfile, PresenceInfo, RoomProfile, UserProfile},
    sidebar::{
        DmList, NotificationCounts, RoomNode, ServerList, ServerRoomNode, SingleList,
        SpaceRoomNode, UserDevice,
    },
    synth::SonicSignature,
    timeline::{UiMediaSource, UiTimelineItem},
};

use crate::{
    app::CurrentWindow,
    components::{chat::Attachment, user_profile::MemberProfileExt},
    hooks::call_tauri,
    tauri_functions::{get_media_blob_url, get_thumbnail_blob_url, get_user_profile},
};
use leptos::prelude::*;

#[derive(Clone, Debug, Default)]
pub struct RoomState {
    pub content: String,
    pub attachments: Vec<Attachment>,
    pub search_parameters: Option<SearchParameters>,
    pub search_results: Option<HashMap<OwnedRoomId, Vec<UiTimelineItem>>>,
    pub pinned_result: Option<Vec<UiTimelineItem>>,
}

#[derive(Clone, Debug, Copy, PartialEq, Default)]
pub enum MainView {
    #[default]
    Chat,
    Info,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum CurrentSection {
    Server(OwnedRoomId),
    #[default]
    Dms,
    Single,
}

impl CurrentSection {
    pub fn is_not_server(&self) -> bool {
        !matches!(self, CurrentSection::Server(_))
    }
}

#[derive(Clone, Debug, Copy, Default)]
pub struct AppState {
    pub current_window: RwSignal<CurrentWindow>,
    pub previous_window: RwSignal<CurrentWindow>,
    pub last_changed_time: RwSignal<f64>,
    pub user_id: RwSignal<Option<OwnedUserId>>,

    pub active_room: RwSignal<Option<RoomNode>>,

    pub active_room_id: RwSignal<Option<OwnedRoomId>>,
    pub active_section: RwSignal<CurrentSection>,
    pub main_view: RwSignal<MainView>,

    pub breadcrums: RwSignal<Breadcrumbs>,
    pub server_order: RwSignal<ServerOrder>,
    pub data_initialized: RwSignal<bool>,

    pub room_map: RwSignal<HashMap<OwnedRoomId, ArcRwSignal<RoomNode>>>,
    pub dm_list: RwSignal<DmList>,
    pub single_room_list: RwSignal<SingleList>,
    pub server_list: RwSignal<ServerList>,

    pub pinned_map: RwSignal<HashMap<OwnedRoomId, Vec<OwnedEventId>>>,

    pub is_focused: RwSignal<bool>,

    pub room_states: RwSignal<HashMap<OwnedRoomId, RoomState>>,

    pub lightbox_image: RwSignal<Option<LighboxImage>>,

    pub notification_counts: RwSignal<HashMap<OwnedRoomId, NotificationCounts>>,
    pub call_members: RwSignal<HashMap<OwnedRoomId, ArcRwSignal<Vec<UserDevice>>>>,

    pub typing_users: RwSignal<HashMap<OwnedRoomId, ArcRwSignal<Vec<OwnedUserId>>>>,

    pub audio_devices: RwSignal<AudioDeviceInfos>,

    pub recent_emojies: RwSignal<RecentEmojies>,

    pub update_progress: RwSignal<UpdateDownloadProgress>,
    pub update_status: RwSignal<UpdateStatus>,

    pub app_version: RwSignal<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LighboxImage {
    pub name: String,
    pub sender_id: OwnedUserId,
    pub timestamp: u64,
    pub size: Option<u64>,
    pub source: UiMediaSource,
    pub origin_rect: Option<(f64, f64, f64, f64)>, // left, top, width, height of clicked thumbnail
    pub width: Option<u64>,
    pub height: Option<u64>,
}

impl AppState {
    pub fn update_typing_users(&self, room_id: &RoomId, user_ids: Vec<OwnedUserId>) {
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
                map.insert(room_id.to_owned(), new_signal);
            });
        }
    }

    pub fn get_typing_users(&self, room_id: &RoomId) -> ArcRwSignal<Vec<OwnedUserId>> {
        if let Some(signal) = self
            .typing_users
            .with_untracked(|map| map.get(room_id).cloned())
        {
            signal
        } else {
            let new_signal = ArcRwSignal::new(Vec::new());
            self.typing_users.update(|map| {
                map.insert(room_id.to_owned(), new_signal.clone());
            });
            new_signal
        }
    }

    pub fn active_room_id(&self) -> Option<OwnedRoomId> {
        self.active_room_id.get()
    }

    pub fn active_room_id_untracked(&self) -> Option<OwnedRoomId> {
        self.active_room_id.get_untracked()
    }

    pub fn apply_server_order(&self) {
        let old_list = self.server_list.get_untracked();

        let new_list = old_list.apply_order(self.server_order.get_untracked());

        if new_list != old_list {
            self.server_list.set(new_list);
        }
    }

    pub fn reorder_servers(&self, source_id: &RoomId, target_id: &RoomId) {
        let old_list = self.server_list.get_untracked();

        let new_list = old_list.reorder_servers(source_id, target_id);

        if new_list != old_list {
            self.server_list.set(new_list);

            log::debug!("Reordered servers: {} -> {}", source_id, target_id);
            self.save_server_order();
        }
    }

    pub fn get_room_sig(&self, room_id: &RoomId) -> Option<ArcRwSignal<RoomNode>> {
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
    fn set_active_room_node(&self, room_id: Option<OwnedRoomId>, node: Option<RoomNode>) {
        if self.active_room_id.with_untracked(|cur| cur != &room_id) {
            self.active_room_id.set(room_id);
        }

        if self.active_room.with_untracked(|cur| cur != &node) {
            self.active_room.set(node);
        }
    }

    /// Change which room is active and which view (chat, room info, …) is shown
    /// for it. `view` is taken explicitly rather than always resetting to
    /// `MainView::Chat` so callers like the per-room info button can jump
    /// straight to `MainView::Info`, including when re-selecting the room
    /// that's already active.
    pub fn set_active_room_with_id(&self, room_id: Option<OwnedRoomId>, view: MainView) {
        log::debug!("Changing active room to {room_id:?}");

        let active_room = if let Some(room_id) = &room_id {
            let node = self.room_map.get_untracked().get(room_id).cloned();

            node.and_then(|sig| sig.try_get_untracked())
        } else {
            None
        };
        self.set_active_room_node(room_id.clone(), active_room);

        if self.main_view.with_untracked(|cur| cur != &view) {
            self.main_view.set(view);
        }

        let Some(room_id) = room_id else {
            return;
        };

        self.breadcrums
            .update(|bc| match self.active_section.get_untracked() {
                CurrentSection::Dms => bc.last_dm_id = Some(room_id.clone()),
                CurrentSection::Single => bc.last_single_id = Some(room_id.clone()),
                CurrentSection::Server(server) => {
                    bc.last_space_ids.insert(server.clone(), room_id.clone());
                }
            });

        self.append_room_id(&room_id);
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

        let breadcrumbs = self.breadcrums.get_untracked();
        let section_clone = section.clone();

        let cached_room_id = match &section {
            CurrentSection::Server(server_id) => breadcrumbs
                .last_space_ids
                .get(server_id)
                .cloned()
                .filter(move |rid| self.room_belongs_to_section(rid, &section_clone)),
            CurrentSection::Dms => breadcrumbs.last_dm_id.clone(),
            CurrentSection::Single => breadcrumbs.last_single_id.clone(),
        };

        if let Some(room_id) = cached_room_id {
            self.set_active_room_with_id(Some(room_id.clone()), MainView::Chat);
            self.append_room_id(&room_id);
            return;
        }

        match &section {
            CurrentSection::Dms => {
                if let Some(room_id) = self.dm_list.get_untracked().0.first().cloned() {
                    self.set_active_room_with_id(Some(room_id.clone()), MainView::Chat);
                    self.append_room_id(&room_id);
                } else {
                    log::warn!("DM list is empty, cannot set active room");
                }
            }
            CurrentSection::Single => {
                if let Some(room_id) = self.single_room_list.get_untracked().0.first().cloned() {
                    self.set_active_room_with_id(Some(room_id.clone()), MainView::Chat);
                    self.append_room_id(&room_id);
                } else {
                    log::warn!("Single room list is empty, cannot set active room");
                }
            }
            CurrentSection::Server(server_id) => {
                if let Some(room_id) = self.first_channel_id_for_server(server_id) {
                    self.set_active_room_with_id(Some(room_id.clone()), MainView::Chat);
                    self.append_room_id(&room_id);
                } else {
                    log::warn!("Server {} has no channels to set as active room", server_id);
                }
            }
        }
    }

    fn first_channel_id_for_server(&self, server_id: &RoomId) -> Option<OwnedRoomId> {
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

    fn append_room_id(&self, room_id: &RoomId) {
        self.breadcrums.update(|bc| {
            bc.recent_rooms.retain(|id| id != room_id);
            bc.recent_rooms.insert(0, room_id.to_owned());

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

    pub fn set_server_order(&self, servers: Vec<OwnedRoomId>) {
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

    fn room_belongs_to_section(&self, room_id: &RoomId, section: &CurrentSection) -> bool {
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
                    || self.find_server_id_for_room(room_id).as_deref() == Some(server_id)
            }
        }
    }

    pub fn section_for_room(&self, room_id: &RoomId) -> CurrentSection {
        let breadcrumbs = self.breadcrums.get_untracked();
        let dm_list = self.dm_list.get_untracked();
        let single_room_list = self.single_room_list.get_untracked();

        if Some(room_id.to_owned()) == breadcrumbs.last_dm_id
            && dm_list.0.iter().any(|id| id == room_id)
        {
            CurrentSection::Dms
        } else if Some(room_id.to_owned()) == breadcrumbs.last_single_id
            && single_room_list.0.iter().any(|id| id == room_id)
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
        room_ids_to_search: &[OwnedRoomId],
        target_room_id: &RoomId,
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
    pub fn find_server_id_for_room(&self, room_id: &RoomId) -> Option<OwnedRoomId> {
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
    pub fn update_call_members(&self, update: HashMap<OwnedRoomId, Vec<UserDevice>>) {
        self.call_members.update(|members| {
            for (room_id, devices) in update.iter() {
                let device_signal = members
                    .entry(room_id.clone())
                    .or_insert_with(|| ArcRwSignal::new(Vec::new()));
                device_signal.set(devices.clone());
            }
        });
    }

    pub fn get_call_members(&self, room_id: &RoomId) -> ArcRwSignal<Vec<UserDevice>> {
        self.call_members
            .get()
            .entry(room_id.to_owned())
            .or_insert_with(|| ArcRwSignal::new(Vec::new()))
            .clone()
    }

    /// Get call members for a set of room ids.
    pub fn get_call_members_in_rooms(
        &self,
        room_ids: HashSet<OwnedRoomId>,
    ) -> HashSet<OwnedUserId> {
        let members = self.call_members.get();
        room_ids
            .into_iter()
            .filter_map(|room_id| {
                let call_member_sig = members.get(&room_id)?;

                let call_member_list = call_member_sig.get();
                if call_member_list.is_empty() {
                    None
                } else {
                    let list: HashSet<OwnedUserId> =
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

    pub fn get_room(&self, room_id: &RoomId) -> Option<RoomNode> {
        self.room_map
            .get()
            .get(room_id)
            .and_then(|sig| sig.try_get())
    }

    pub fn get_room_untracked(&self, room_id: &RoomId) -> Option<RoomNode> {
        self.room_map
            .get_untracked()
            .get(room_id)
            .and_then(|sig| sig.try_get_untracked())
    }

    pub fn get_dms(&self) -> Vec<RoomNode> {
        self.dm_list
            .get()
            .0
            .iter()
            .filter_map(|room_id| self.get_room(room_id))
            .collect()
    }

    pub fn get_single_rooms(&self) -> Vec<RoomNode> {
        self.single_room_list
            .get()
            .0
            .iter()
            .filter_map(|room_id| self.get_room(room_id))
            .collect()
    }

    pub fn get_server_rooms(&self, server_id: &RoomId) -> Vec<RoomNode> {
        let Some(server) = self.get_room(server_id).and_then(|s| s.as_server()) else {
            return vec![];
        };
        server
            .children
            .iter()
            .filter_map(|room_id| self.get_room(room_id))
            .collect()
    }
}

type MemberStoreRoomEntry = HashMap<OwnedUserId, ArcRwSignal<MemberProfile>>;

#[derive(Default, Clone, Copy)]
pub struct ProfileStore {
    pub members: RwSignal<HashMap<OwnedRoomId, MemberStoreRoomEntry>>,
    pub presences: RwSignal<HashMap<OwnedUserId, ArcRwSignal<PresenceInfo>>>,

    pub user_profiles: RwSignal<HashMap<OwnedUserId, ArcRwSignal<UserProfile>>>,
}

impl ProfileStore {
    pub fn get_member_profile(
        &self,
        room_id: &RoomId,
        user_id: &UserId,
    ) -> ArcRwSignal<MemberProfile> {
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
            room_id: room_id.to_owned(),
            profile: UserProfile {
                user_id: user_id.to_owned(),
                display_name: None,
                avatar_url: None,
                custom_properties: CustomProperties::from_user_id(user_id),
            },
        });

        self.members.update(|members| {
            members
                .entry(room_id.to_owned())
                .or_insert_with(HashMap::new)
                .insert(user_id.to_owned(), sig.clone());
        });

        sig
    }

    pub fn get_presence(&self, user_id: &UserId) -> ArcRwSignal<PresenceInfo> {
        let existing_signal = self.presences.with_untracked(|p| p.get(user_id).cloned());

        if let Some(sig) = existing_signal {
            return sig;
        }

        let new_signal = ArcRwSignal::new(PresenceInfo::default());

        self.presences.update(|presences| {
            presences.insert(user_id.to_owned(), new_signal.clone());
        });

        new_signal
    }

    pub fn get_user_profile(&self, user_id: &UserId) -> ArcRwSignal<UserProfile> {
        let existing_signal = self
            .user_profiles
            .with_untracked(|p| p.get(user_id).cloned());

        if let Some(sig) = existing_signal {
            return sig;
        }

        let sig = ArcRwSignal::new(UserProfile {
            user_id: user_id.to_owned(),
            display_name: None,
            avatar_url: None,
            custom_properties: CustomProperties::from_user_id(user_id),
        });

        self.user_profiles.update(|profiles| {
            profiles.insert(user_id.to_owned(), sig.clone());
        });

        let sig_clone = sig.clone();
        let user_id = user_id.to_owned();
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

    pub fn get_profile_signal(
        &self,
        room_id: Option<OwnedRoomId>,
        user_id: &UserId,
    ) -> ProfileSignal {
        if let Some(room_id) = room_id {
            ProfileSignal::Member(self.get_member_profile(&room_id, user_id))
        } else {
            ProfileSignal::User(self.get_user_profile(user_id))
        }
    }

    pub fn get_members(self, room_id: &RoomId) -> Vec<MemberProfile> {
        self.members
            .get()
            .get(room_id)
            .map(|m| m.values().map(|s| s.get()).collect())
            .unwrap_or_default()
    }

    pub fn get_member_signals(
        self,
        room_id: &RoomId,
    ) -> HashMap<OwnedUserId, ArcRwSignal<MemberProfile>> {
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

    pub fn icon(self, size_str: String, cache: MediaCache) -> impl IntoView {
        match self {
            ProfileSignal::User(sig) => sig.get().render_icon(size_str, cache).into_any(),
            ProfileSignal::Member(sig) => sig.get().render_icon(size_str, cache).into_any(),
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

fn media_source_key(source: &MediaSource) -> String {
    match source {
        MediaSource::Plain(uri) => uri.to_string(),
        MediaSource::Encrypted(file) => file.url.to_string(),
    }
}

type ThumbnailKey = (String, UiThumbnailSettings);

#[derive(Default, Clone, Copy)]
pub struct MediaCache {
    avatars: RwSignal<HashMap<OwnedMxcUri, ArcRwSignal<Option<String>>>>,
    files: RwSignal<HashMap<String, ArcRwSignal<Option<String>>>>,
    thumbnails: RwSignal<HashMap<ThumbnailKey, ArcRwSignal<Option<String>>>>,
}

const AVATAR_THUMBNAIL_SETTINGS: UiThumbnailSettings = UiThumbnailSettings {
    method: UiThumbnailMethod::Crop,
    width: 96,
    height: 96,
    animated: true,
};

impl MediaCache {
    pub fn get_avatar(&self, mxc: &MxcUri) -> ArcRwSignal<Option<String>> {
        let existing_signal = self.avatars.get_untracked().get(mxc).cloned();

        if let Some(signal) = existing_signal {
            return signal;
        }

        let signal = get_thumbnail_blob_url(
            &MediaSource::Plain(mxc.to_owned()),
            &AVATAR_THUMBNAIL_SETTINGS,
        );

        self.avatars.update_untracked(|map| {
            map.insert(mxc.to_owned(), signal.clone());
        });

        signal
    }

    pub fn get_file(&self, source: &MediaSource) -> ArcRwSignal<Option<String>> {
        let key = media_source_key(source);
        let existing_signal = self.files.get_untracked().get(&key).cloned();

        if let Some(signal) = existing_signal {
            return signal;
        }

        let signal = get_media_blob_url(source);

        self.files.update_untracked(|map| {
            map.insert(key, signal.clone());
        });

        signal
    }

    pub fn get_thumbnail(
        &self,
        source: &MediaSource,
        settings: &UiThumbnailSettings,
    ) -> ArcRwSignal<Option<String>> {
        let (width, height) =
            shared::timeline::snap_thumbnail_size(settings.width, settings.height);
        let settings = UiThumbnailSettings {
            width,
            height,
            ..settings.clone()
        };

        let key = (media_source_key(source), settings.clone());
        let existing_signal = self.thumbnails.get_untracked().get(&key).cloned();

        if let Some(signal) = existing_signal {
            return signal;
        }

        let signal = get_thumbnail_blob_url(source, &settings);

        self.thumbnails.update_untracked(|map| {
            map.insert(key, signal.clone());
        });

        signal
    }
}
