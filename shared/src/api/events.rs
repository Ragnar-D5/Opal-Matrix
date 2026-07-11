use std::collections::HashMap;

use macros::TauriEvent;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    profile::{MemberProfile, PresenceInfo},
    sidebar::{NotificationCounts, UserDevice},
    timeline::UiTimelineItem,
};

pub trait TauriEvent: Serialize + DeserializeOwned + PartialEq {
    fn name() -> String;
}

impl TauriEvent for String {
    fn name() -> String {
        "String".to_string()
    }
}

impl TauriEvent for Uuid {
    fn name() -> String {
        "Uuid".to_string()
    }
}

impl<T> TauriEvent for Vec<T>
where
    T: TauriEvent,
{
    fn name() -> String {
        format!("Vec_{}", T::name())
    }
}

impl<K, V> TauriEvent for HashMap<K, V>
where
    K: TauriEvent + std::hash::Hash + Eq,
    V: TauriEvent,
{
    fn name() -> String {
        format!("HashMap_{}_{}", K::name(), V::name())
    }
}

impl<K, V> TauriEvent for (K, V)
where
    K: TauriEvent,
    V: TauriEvent,
{
    fn name() -> String {
        format!("Tuple_{}_{}", K::name(), V::name())
    }
}

impl<K, V, T> TauriEvent for (K, V, T)
where
    K: TauriEvent,
    V: TauriEvent,
    T: TauriEvent,
{
    fn name() -> String {
        format!("Tuple_{}_{}_{}", K::name(), V::name(), T::name())
    }
}

pub type ProfileUpdates = HashMap<String, Vec<MemberProfile>>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TauriEvent)]
pub struct TypingUpdate {
    pub room_id: String,
    pub user_ids: Vec<String>,
}

pub type CallMemberUpdate = HashMap<String, Vec<UserDevice>>;

pub type PresenceUpdate = HashMap<String, PresenceInfo>;

pub type NotificationUpdate = HashMap<String, NotificationCounts>;

/// A single log line, buffered in the backend and streamed to the log window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TauriEvent)]
pub struct LogEntry {
    /// Monotonic sequence number, used for ordering and de-duplication when the
    /// live event stream and the backlog snapshot overlap.
    pub seq: u64,
    pub level: String,
    pub timestamp: String,
    pub path: String,
    pub line: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TauriEvent)]
pub struct SettingsUpdate {
    pub key: String,
    pub value: String,
    pub cloud: bool,
    /// When true the frontend should not re-upload to cloud; the backend
    /// already handled the cloud write (set_setting with to_cloud=true).
    pub skip_cloud_upload: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TauriEvent)]
pub enum NotificationEvent {
    UpdateAvailable,
    UpdateDownloaded,
    GenericNotification {
        title: String,
        message: String,
        level: NotificationLevel,
    },
}

pub type SearchResultUpdate = (Uuid, String, Vec<UiTimelineItem>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TauriEvent)]
pub struct RoomPinnedUpdate(pub (String, Vec<String>));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentEmoji {
    pub emoji: String,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TauriEvent, Default)]
pub struct RecentEmojies {
    pub top: Vec<RecentEmoji>,
    pub all_by_recency: Vec<RecentEmoji>,
}
