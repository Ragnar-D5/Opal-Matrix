use std::collections::HashMap;

use colorsys::Hsl;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::get_color;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct AbstractProgress {
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MediaUploadProgress {
    pub index: u64,
    pub progress: AbstractProgress,
}

/// State for messages which haven't been sent yet, or failed to send. This is used to show progress indicators for media uploads, and error messages for failed sends.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum EventState {
    NotSentYet {
        progress: Option<MediaUploadProgress>,
    },
    SendingFailed {
        error: String,
        is_recoverable: bool,
    },
    Sent {
        event_id: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Sender {
    pub id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct EventFlags {
    pub is_editable: bool,
    pub is_reactable: bool,
    pub is_highlighted: bool,
    pub can_be_replied_to: bool,
    pub contains_only_emojis: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Change<T> {
    pub old: T,
    pub new: T,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiMembershipChange {
    None,
    Error,
    Joined,
    Left,
    Banned,
    Unbanned,
    Kicked,
    Invited,
    KickedAndBanned,
    InvitationAccepted,
    InvitationRejected,
    InvitationRevoked,
    Knocked,
    KnockAccepted,
    KnockRetracted,
    KnockDenied,
    NotImplemented,
}

impl UiMembershipChange {
    /// Generates a user-friendly message describing the membership change, e.g. "joined the room", "was invited to the room", "left the room". Returns an empty string for UiMembershipChange::None, and a generic error message for UiMembershipChange::Error.
    pub fn display_string(&self) -> String {
        match self {
            UiMembershipChange::None => "".to_string(),
            UiMembershipChange::Error => "Failed to update membership".to_string(),
            UiMembershipChange::Joined => "joined the room".to_string(),
            UiMembershipChange::Left => "left the room".to_string(),
            UiMembershipChange::Banned => "was banned from the room".to_string(),
            UiMembershipChange::Unbanned => "was unbanned from the room".to_string(),
            UiMembershipChange::Kicked => "was kicked from the room".to_string(),
            UiMembershipChange::Invited => "was invited to the room".to_string(),
            UiMembershipChange::KickedAndBanned => {
                "was kicked and banned from the room".to_string()
            }
            UiMembershipChange::InvitationAccepted => {
                "accepted the invitation to the room".to_string()
            }
            UiMembershipChange::InvitationRejected => {
                "rejected the invitation to the room".to_string()
            }
            UiMembershipChange::InvitationRevoked => {
                "had their invitation to the room revoked".to_string()
            }
            UiMembershipChange::Knocked => "knocked on the room".to_string(),
            UiMembershipChange::KnockAccepted => "had their knock accepted by the room".to_string(),
            UiMembershipChange::KnockRetracted => "retracted their knock from the room".to_string(),
            UiMembershipChange::KnockDenied => "had their knock denied by the room".to_string(),
            UiMembershipChange::NotImplemented => "membership change not implemented".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum RoomIdFormat {
    Id(String),
    Alias(String),
}

impl RoomIdFormat {
    pub fn source(&self) -> String {
        match self {
            RoomIdFormat::Id(id) => id.clone(),
            RoomIdFormat::Alias(alias) => alias.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RichTextSpan {
    Plain(String),
    UserMention {
        user_id: String,
        display_name: String,
    },
    RoomMention {
        room_id: RoomIdFormat,
        display_name: String,
    },
    Link {
        url: String,
        text: Option<String>,
    },
    Newline,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum UiMediaSource {
    Uuid(Uuid),
    Plain(String),
    Encrypted { url: String, k: String, iv: String },
}

impl UiMediaSource {
    pub fn url(&self) -> String {
        match self {
            UiMediaSource::Plain(url) => url.to_string(),
            UiMediaSource::Encrypted { url, k, iv } => format!(
                "{url}?key={}&iv={}",
                urlencoding::encode(k),
                urlencoding::encode(iv)
            ),
            UiMediaSource::Uuid(uuid) => format!("mxc://media/{}", uuid),
        }
    }

    pub fn thumbnail_url(&self, width: u64, height: u64) -> String {
        match self {
            UiMediaSource::Uuid(uuid) => {
                format!("mxc://thumbnail/{}?width={}&height={}", uuid, width, height)
            }
            other => other.url(),
        }
    }
}

pub fn fit_dimensions(w: u64, h: u64, max_w: u64, max_h: u64) -> (u64, u64) {
    if w == 0 || h == 0 {
        return (max_w, max_h);
    }
    let scale = (max_w as f64 / w as f64)
        .min(max_h as f64 / h as f64)
        .min(1.0);
    ((w as f64 * scale) as u64, (h as f64 * scale) as u64)
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub enum UiPollKind {
    #[default]
    Undisclosed,
    Disclosed,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UiPollResult {
    pub question: String,
    pub kind: UiPollKind,
    pub max_selections: u64,
    // pub answers: Vec<PollResultAnswer>,
    pub votes: HashMap<String, Vec<String>>,
    pub end_time: Option<u64>,
    pub has_been_edited: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UiBeaconInfo {
    pub geo_uri: String,
    pub description: Option<String>,
    pub timestamp: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum UiMessageType {
    Audio {
        source: UiMediaSource,
        filename: String,
        duration: Option<u64>,
    },
    Emote,
    FailedToDecrypt,
    File {
        source: UiMediaSource,
        filename: String,
        mime_type: Option<String>,
        size: Option<u64>,
    },
    Gallery,
    Image {
        filename: String,
        source: UiMediaSource,
        width: Option<u64>,
        height: Option<u64>,
        size: Option<u64>,
        mime_type: Option<String>,
    },
    LiveLocation {
        locations: Vec<UiBeaconInfo>,
    },
    Location(UiBeaconInfo),
    Notice,
    Poll {
        fallback_text: Option<String>,
        result: UiPollResult,
        is_edit: bool,
    },
    Redacted,
    ServerNotice {
        admin_contact: Option<String>,
        limit_msg: Option<String>,
    },
    Sticker {
        source: UiMediaSource,
        width: Option<u64>,
        height: Option<u64>,
        size: Option<u64>,
        mime_type: Option<String>,
    },
    Text,
    Video {
        source: UiMediaSource,
        filename: String,
        width: Option<u64>,
        height: Option<u64>,
        duration: Option<u64>,
        size: Option<u64>,
        mime_type: Option<String>,
    },
    VerificationRequest,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReplyPreview {
    pub sender: DetailState<Sender>,
    pub content: Vec<RichTextSpan>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReplyInfo {
    pub event_id: String,
    pub event: DetailState<ReplyPreview>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReactionInfo {
    pub sender_id: String,
    pub timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct MessageContent {
    pub reactions: HashMap<String, Vec<ReactionInfo>>,
    pub in_reply_to: Option<ReplyInfo>,
    pub thread_root: Option<String>,
    pub is_edited: bool,

    pub is_redacted: bool,

    pub body: Vec<RichTextSpan>,

    pub msg_type: UiMessageType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProfileChange {
    pub user_id: String,
    pub display_name_change: Option<Change<Option<String>>>,
    pub avatar_url_change: Option<Change<Option<String>>>,
}

impl ProfileChange {
    pub fn display_string(&self) -> String {
        let mut changes = Vec::new();

        if let Some(Change { old, new }) = &self.display_name_change {
            if let Some(new) = new {
                if let Some(old) = old {
                    changes.push(format!(
                        "changed their display name from '{}' to '{}'",
                        old, new
                    ));
                } else {
                    changes.push(format!("set their display name to '{}'", new));
                }
            } else {
                changes.push("removed their display name".to_string());
            }
        }

        if let Some(Change { old, new }) = &self.avatar_url_change {
            if new.is_some() && old.is_none() {
                changes.push("set a profile picture".to_string());
            } else if new.is_none() && old.is_some() {
                changes.push("removed their profile picture".to_string());
            } else {
                changes.push("changed their profile picture".to_string());
            }
        }

        changes.join(" and ")
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiCallIntent {
    Audio,
    Video,
    Unknown,
}

impl std::fmt::Display for UiCallIntent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiCallIntent::Audio => write!(f, "Audio"),
            UiCallIntent::Video => write!(f, "Video"),
            UiCallIntent::Unknown => write!(f, "Call"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiGuestAccess {
    CanJoin,
    Forbidden,
    Unknown,
}

impl std::fmt::Display for UiGuestAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiGuestAccess::CanJoin => write!(f, "Guests can join"),
            UiGuestAccess::Forbidden => write!(f, "Guests cannot join"),
            UiGuestAccess::Unknown => write!(f, "Guest access unknown"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiHistoryVisibility {
    Invited,
    Joined,
    Shared,
    WorldReadable,
    Unknown,
}

impl std::fmt::Display for UiHistoryVisibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiHistoryVisibility::Invited => write!(f, "Only invited members can see history"),
            UiHistoryVisibility::Joined => write!(f, "Only joined members can see history"),
            UiHistoryVisibility::Shared => write!(f, "Shared history"),
            UiHistoryVisibility::WorldReadable => write!(f, "World readable history"),
            UiHistoryVisibility::Unknown => write!(f, "Unknown history visibility"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiJoinRule {
    Public,
    Knock,
    Invite,
    Private,
    Restricted,
    KnockRestricted,
    Unknown,
}

impl std::fmt::Display for UiJoinRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UiJoinRule::Public => write!(f, "Public"),
            UiJoinRule::Knock => write!(f, "Knock"),
            UiJoinRule::Invite => write!(f, "Invite"),
            UiJoinRule::Private => write!(f, "Private"),
            UiJoinRule::Restricted => write!(f, "Restricted"),
            UiJoinRule::KnockRestricted => write!(f, "Knock Restricted"),
            UiJoinRule::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SystemMessage {
    MembershipChange {
        user_id: String,
        change: Option<UiMembershipChange>,
    },
    ProfileChange(ProfileChange),
    CallInvite,
    CallMember,
    RtcNotification {
        call_intent: Option<UiCallIntent>,
        declined_by: Vec<String>,
    },
    PolicyRuleRoom,
    PolicyRuleServer,
    PolicyRuleUser,
    RoomAvatar {
        url: Option<String>,
    },
    RoomCanonicalAlias {
        alias: Option<String>,
    },
    RoomCreate {
        additional_creators: Vec<String>,
        room_type: Option<String>,
    },
    RoomEncryption {
        algorithm: String,
    },
    RoomGuestAccess {
        guest_access: UiGuestAccess,
    },
    RoomHistoryVisibility {
        visibility: UiHistoryVisibility,
    },
    RoomJoinRules {
        join_rule: UiJoinRule,
    },
    RoomName {
        name: String,
    },
    RoomPinnedEvents {
        pinned_events: Vec<String>,
    },
    RoomPowerLevels,
    RoomServerAcl,
    RoomThirdPartyInvite {
        display_name: String,
    },
    RoomTombstone {
        body: String,
        replacement_room: String,
    },
    RoomTopic {
        topic: String,
    },
    SpaceChild {
        via: Vec<String>,
        order: Option<String>,
        suggested: bool,
    },
    SpaceParent {
        via: Vec<String>,
        canonical: bool,
    },
    Redacted,
    Unknown,
    RoomImagePack,
    BeaconInfo,
    MemberHints,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum EventContent {
    MsgLike(Box<MessageContent>),
    FailedToParseMessageLike {
        event_type: String,
        error: String,
    },
    FailedToParseState {
        event_type: String,
        state_key: String,
        error: String,
    },
    SystemMessage(SystemMessage),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DetailState<T> {
    Unavailable,
    Pending,
    Ready(T),
    Error(String),
}

impl DetailState<Sender> {
    pub fn display_name(&self) -> String {
        match self {
            DetailState::Unavailable | DetailState::Pending | DetailState::Error(_) => {
                "Unknown".to_string()
            }
            DetailState::Ready(sender) => sender.display_name.clone().unwrap_or(sender.id.clone()),
        }
    }

    pub fn avatar_url(&self) -> Option<String> {
        match self {
            DetailState::Ready(sender) => sender.avatar_url.clone(),
            _ => None,
        }
    }

    pub fn color(&self) -> Hsl {
        match self {
            DetailState::Ready(sender) => get_color(&sender.id),
            DetailState::Error(_) => Hsl::new(0.0, 100.0, 70.0, None),
            DetailState::Pending => Hsl::new(0.0, 0.0, 50.0, None),
            DetailState::Unavailable => Hsl::new(50.0, 100.0, 80.0, None),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TimelineEvent {
    pub state: Option<EventState>,
    pub timestamp: u64,
    pub flags: EventFlags,
    pub sender: DetailState<Sender>,

    pub event_id: Option<String>,

    pub content: EventContent,
}

impl TimelineEvent {
    pub fn is_sending(&self) -> bool {
        matches!(self.state, Some(EventState::NotSentYet { .. }))
    }

    pub fn in_reply_to(&self) -> Option<ReplyInfo> {
        match &self.content {
            EventContent::MsgLike(content) => content.in_reply_to.clone(),
            _ => None,
        }
    }

    /// Returns Some(error_message) if the message failed to send, None otherwise.
    pub fn get_failed_message(&self) -> Option<String> {
        if let Some(EventState::SendingFailed { error, .. }) = &self.state {
            Some(error.clone())
        } else {
            None
        }
    }

    pub fn get_reactions(&self) -> Option<HashMap<String, Vec<ReactionInfo>>> {
        match &self.content {
            EventContent::MsgLike(content) => Some(content.reactions.clone()),
            _ => None,
        }
    }

    pub fn get_sender_avatar_url(&self) -> Option<String> {
        match &self.sender {
            DetailState::Ready(sender) => sender.avatar_url.clone(),
            _ => None,
        }
    }

    /// Returns the sender's display name if available, otherwise their user ID. Returns None if the sender details are not ready.
    pub fn get_sender_name(&self) -> Option<String> {
        match &self.sender {
            DetailState::Ready(sender) => {
                Some(sender.display_name.clone().unwrap_or(sender.id.clone()))
            }
            _ => None,
        }
    }

    pub fn get_sender_id(&self) -> Option<String> {
        match &self.sender {
            DetailState::Ready(sender) => Some(sender.id.clone()),
            _ => None,
        }
    }

    pub fn calculate_flags(&mut self, is_own: bool) {
        let can_be_replied_to = self.flags.can_be_replied_to
            && match &self.content {
                EventContent::MsgLike(content) => matches!(
                    &content.msg_type,
                    UiMessageType::Text
                        | UiMessageType::Emote
                        | UiMessageType::Notice
                        | UiMessageType::Poll { .. }
                        | UiMessageType::Audio { .. }
                        | UiMessageType::File { .. }
                        | UiMessageType::Gallery
                        | UiMessageType::Image { .. }
                        | UiMessageType::LiveLocation { .. }
                        | UiMessageType::Location(_)
                        | UiMessageType::Sticker { .. }
                        | UiMessageType::Video { .. }
                        | UiMessageType::FailedToDecrypt
                ),
                _ => false,
            };

        let is_editable = is_own
            && self.flags.is_editable
            && match &self.content {
                EventContent::MsgLike(content) => matches!(
                    &content.msg_type,
                    UiMessageType::Text
                        | UiMessageType::Emote
                        | UiMessageType::Notice
                        | UiMessageType::Poll { .. }
                ),
                _ => false,
            };

        let is_reactable = match &self.content {
            EventContent::MsgLike(content) if !content.is_redacted => matches!(
                &content.msg_type,
                UiMessageType::Text
                    | UiMessageType::Emote
                    | UiMessageType::Notice
                    | UiMessageType::Poll { .. }
                    | UiMessageType::Audio { .. }
                    | UiMessageType::File { .. }
                    | UiMessageType::Gallery
                    | UiMessageType::Image { .. }
                    | UiMessageType::LiveLocation { .. }
                    | UiMessageType::Location(_)
                    | UiMessageType::Sticker { .. }
                    | UiMessageType::Video { .. }
            ),
            _ => false,
        };

        self.flags.can_be_replied_to = can_be_replied_to;
        self.flags.is_editable = is_editable;
        self.flags.is_reactable = is_reactable;
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiTimelineItemKind {
    Event(Box<TimelineEvent>),
    DateDivider(u64),
    ReadMarker,
    TimelineStart,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UiTimelineItem {
    pub id: String,
    pub render_key: String,

    pub kind: UiTimelineItemKind,
}

impl UiTimelineItem {
    pub fn body(&self) -> Vec<RichTextSpan> {
        match &self.kind {
            UiTimelineItemKind::Event(event) => match &event.content {
                EventContent::MsgLike(content) => content.body.clone(),
                _ => vec![RichTextSpan::Plain("Unsupported event type".to_string())],
            },
            UiTimelineItemKind::DateDivider(_) => {
                vec![RichTextSpan::Plain("Date Divider".to_string())]
            }
            UiTimelineItemKind::ReadMarker => vec![RichTextSpan::Plain("Read Marker".to_string())],
            UiTimelineItemKind::TimelineStart => {
                vec![RichTextSpan::Plain("Start of Timeline".to_string())]
            }
        }
    }

    pub fn flags(&self) -> EventFlags {
        match &self.kind {
            UiTimelineItemKind::Event(event) => event.flags.clone(),
            _ => EventFlags::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UiTimelineDiff {
    Append { values: Vec<UiTimelineItem> },
    Clear,
    PushFront { value: UiTimelineItem },
    PushBack { value: UiTimelineItem },
    PopFront,
    PopBack,
    Insert { index: usize, value: UiTimelineItem },
    Set { index: usize, value: UiTimelineItem },
    Remove { index: usize },
    Truncate { length: usize },
    Reset { values: Vec<UiTimelineItem> },
}

pub fn coalesce_diffs(diffs: Vec<UiTimelineDiff>) -> Vec<UiTimelineDiff> {
    let mut optimized = Vec::new();

    let mut iter = diffs.into_iter().peekable();
    while let Some(diff) = iter.next() {
        match diff {
            UiTimelineDiff::Remove { index } => {
                // If the very next diff inserts or pushes back into the exact same spot,
                // it's just a replacement. Turn it into a Set.
                match iter.peek() {
                    Some(UiTimelineDiff::Insert {
                        index: i_idx,
                        value,
                    }) if index == *i_idx => {
                        let value = value.clone();
                        iter.next(); // Consume the Insert
                        optimized.push(UiTimelineDiff::Set { index, value });
                    }
                    Some(UiTimelineDiff::PushBack { value }) => {
                        // Assuming Remove was the last item, PushBack puts it right back
                        let value = value.clone();
                        iter.next(); // Consume the PushBack
                        optimized.push(UiTimelineDiff::Set { index, value });
                    }
                    _ => optimized.push(diff),
                }
            }
            _ => optimized.push(diff),
        }
    }

    optimized
}
