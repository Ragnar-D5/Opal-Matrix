use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use matrix_sdk::ruma::{
    MilliSecondsSinceUnixEpoch,
    events::{
        poll::start::PollKind,
        room::{EncryptedFileInfo, MediaSource, message::MessageType},
        rtc::notification::CallIntent,
        sticker::StickerMediaSource,
    },
};
use matrix_sdk_ui::{
    eyeball_im::VectorDiff,
    timeline::{
        BeaconInfo, EventSendState, EventTimelineItem, MembershipChange, MsgLikeKind,
        ReactionsByKeyBySender, TimelineDetails, TimelineItem, TimelineItemContent,
        TimelineItemKind, VirtualTimelineItem,
    },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct AbstractProgress {
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct MediaUploadProgress {
    pub index: u64,
    pub progress: AbstractProgress,
}

/// State for messages which haven't been sent yet, or failed to send. This is used to show progress indicators for media uploads, and error messages for failed sends.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
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

impl From<&EventSendState> for EventState {
    fn from(state: &EventSendState) -> Self {
        match state {
            EventSendState::NotSentYet { progress } => EventState::NotSentYet {
                progress: progress.clone().map(|p| MediaUploadProgress {
                    index: p.index,
                    progress: AbstractProgress {
                        current: p.progress.current,
                        total: p.progress.total,
                    },
                }),
            },
            EventSendState::SendingFailed {
                error,
                is_recoverable,
            } => EventState::SendingFailed {
                error: error.to_string(),
                is_recoverable: *is_recoverable,
            },
            EventSendState::Sent { event_id } => EventState::Sent {
                event_id: event_id.to_string(),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct Sender {
    pub id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct EventFlags {
    pub is_editable: bool,
    pub is_highlighted: bool,
    pub can_be_replied_to: bool,
    pub contains_only_emojis: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct Change<T> {
    pub old: T,
    pub new: T,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
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

impl From<MembershipChange> for UiMembershipChange {
    fn from(value: matrix_sdk_ui::timeline::MembershipChange) -> Self {
        match value {
            MembershipChange::None => UiMembershipChange::None,
            MembershipChange::Error => UiMembershipChange::Error,
            MembershipChange::Joined => UiMembershipChange::Joined,
            MembershipChange::Left => UiMembershipChange::Left,
            MembershipChange::Banned => UiMembershipChange::Banned,
            MembershipChange::Unbanned => UiMembershipChange::Unbanned,
            MembershipChange::Kicked => UiMembershipChange::Kicked,
            MembershipChange::Invited => UiMembershipChange::Invited,
            MembershipChange::KickedAndBanned => UiMembershipChange::KickedAndBanned,
            MembershipChange::InvitationAccepted => UiMembershipChange::InvitationAccepted,
            MembershipChange::InvitationRejected => UiMembershipChange::InvitationRejected,
            MembershipChange::InvitationRevoked => UiMembershipChange::InvitationRevoked,
            MembershipChange::Knocked => UiMembershipChange::Knocked,
            MembershipChange::KnockAccepted => UiMembershipChange::KnockAccepted,
            MembershipChange::KnockRetracted => UiMembershipChange::KnockRetracted,
            MembershipChange::KnockDenied => UiMembershipChange::KnockDenied,
            MembershipChange::NotImplemented => UiMembershipChange::NotImplemented,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct InReplyToDetails {
    pub event_id: String,
    pub sender: Sender,
    pub content: Box<EventContent>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub enum RichTextSpan {
    Plain(String),
    UserMention {
        user_id: String,
        display_name: String,
    },
    RoomMention,
    Link {
        url: String,
        text: Option<String>,
    },
    Newline,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub enum UiMediaSource {
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
        }
    }
}

impl From<MediaSource> for UiMediaSource {
    fn from(value: MediaSource) -> Self {
        match value {
            MediaSource::Plain(url) => UiMediaSource::Plain(url.to_string()),
            MediaSource::Encrypted(file) => {
                let url = file.url.to_string();

                let EncryptedFileInfo::V2(info) = file.info else {
                    return UiMediaSource::Plain(url);
                };

                // let Some(EncryptedFileHash::Sha256(hash)) =
                //     file.hashes.get(&EncryptedFileHashAlgorithm::Sha256)
                // else {
                //     return UiMediaSource::Plain(url);
                // };

                UiMediaSource::Encrypted {
                    url,
                    k: info.k.to_string(),
                    iv: info.iv.to_string(),
                    // hash: hash.to_string(),
                }
            }
        }
    }
}

impl From<StickerMediaSource> for UiMediaSource {
    fn from(value: StickerMediaSource) -> Self {
        let value: MediaSource = value.into();

        value.into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default, Hash)]
pub enum UiPollKind {
    #[default]
    Undisclosed,
    Disclosed,
}

impl From<PollKind> for UiPollKind {
    fn from(value: PollKind) -> Self {
        match value {
            PollKind::Undisclosed => UiPollKind::Undisclosed,
            PollKind::Disclosed => UiPollKind::Disclosed,
            _ => UiPollKind::default(),
        }
    }
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

impl Hash for UiPollResult {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.question.hash(state);
        self.kind.hash(state);
        self.max_selections.hash(state);
        self.end_time.hash(state);
        self.has_been_edited.hash(state);

        for (answer_id, voters) in &self.votes {
            answer_id.hash(state);
            for voter in voters {
                voter.hash(state);
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct UiBeaconInfo {
    pub geo_uri: String,
    pub description: Option<String>,
    pub timestamp: u64,
}

impl From<BeaconInfo> for UiBeaconInfo {
    fn from(value: BeaconInfo) -> Self {
        UiBeaconInfo {
            geo_uri: value.geo_uri().to_string(),
            description: value.description().map(|d| d.to_string()),
            timestamp: value.ts().as_secs().into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
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
pub struct MessageContent {
    pub reactions: HashMap<String, Vec<String>>,
    pub in_reply_to: Option<String>,
    pub thread_root: Option<String>,
    pub is_edited: bool,

    pub is_redacted: bool,

    pub body: Vec<RichTextSpan>,

    pub msg_type: UiMessageType,
}

impl Hash for MessageContent {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.in_reply_to.hash(state);
        self.thread_root.hash(state);
        self.is_edited.hash(state);
        self.is_redacted.hash(state);
        self.body.hash(state);
        self.msg_type.hash(state);

        for (emoji, reactors) in &self.reactions {
            emoji.hash(state);
            for reactor in reactors {
                reactor.hash(state);
            }
        }
    }
}

fn get_reactions(reactions: ReactionsByKeyBySender) -> HashMap<String, Vec<String>> {
    reactions
        .iter()
        .map(|(key, by_sender)| {
            let reactors: Vec<String> = by_sender
                .iter()
                .map(|(sender, _)| sender.to_string())
                .collect();
            (key.clone(), reactors)
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum SystemMessage {
    MembershipChange {
        user_id: String,
        change: Option<UiMembershipChange>,
    },
    ProfileChange {
        user_id: String,
        display_name_change: Option<Change<Option<String>>>,
        avatar_url_changed: Option<Change<Option<String>>>,
    },
    CallInvite,
    RtcNotification {
        call_intent: Option<CallIntent>,
        declined_by: Vec<String>,
    },
    OtherEvent,
}

impl Hash for SystemMessage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            SystemMessage::MembershipChange { user_id, change } => {
                user_id.hash(state);
                change.hash(state);
            }
            SystemMessage::ProfileChange {
                user_id,
                display_name_change,
                avatar_url_changed,
            } => {
                user_id.hash(state);
                display_name_change.hash(state);
                avatar_url_changed.hash(state);
            }
            SystemMessage::CallInvite => {
                "CallInvite".hash(state);
            }
            SystemMessage::RtcNotification {
                call_intent,
                declined_by,
            } => {
                call_intent
                    .clone()
                    .map(|v| v.to_string())
                    .unwrap_or_default()
                    .hash(state);
                for user in declined_by {
                    user.hash(state);
                }
            }
            SystemMessage::OtherEvent => {
                "OtherEvent".hash(state);
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
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

impl From<&TimelineItemContent> for EventContent {
    fn from(value: &TimelineItemContent) -> Self {
        match value.clone() {
            TimelineItemContent::MsgLike(content) => {
                let mut is_redacted = false;
                let mut is_edited = false;

                let (body, msg_type) = match content.kind.clone() {
                    MsgLikeKind::Message(msg) => {
                        is_edited = msg.is_edited();

                        match msg.msgtype().clone() {
                            MessageType::Audio(content) => (
                                vec![RichTextSpan::Plain(content.body.clone())],
                                UiMessageType::Audio {
                                    source: content.source.clone().into(),
                                    filename: content.filename().to_string(),
                                    duration: content
                                        .info
                                        .map(|v| v.duration.map(|d| d.as_secs()))
                                        .unwrap_or_default(),
                                },
                            ),
                            MessageType::Emote(content) => (
                                vec![RichTextSpan::Plain(content.body.clone())],
                                UiMessageType::Emote,
                            ),
                            MessageType::File(content) => {
                                let info = content.info.clone().unwrap_or_default();

                                (
                                    vec![RichTextSpan::Plain(content.body.clone())],
                                    UiMessageType::File {
                                        source: content.source.clone().into(),
                                        filename: content.filename().to_string(),
                                        mime_type: info.mimetype.map(|m| m.to_string()),
                                        size: info.size.map(|s| s.into()),
                                    },
                                )
                            }
                            MessageType::Image(content) => {
                                let info = content.info.clone().unwrap_or_default();

                                (
                                    vec![RichTextSpan::Plain(content.body.clone())],
                                    UiMessageType::Image {
                                        filename: content.filename().to_string(),
                                        source: content.source.into(),
                                        width: info.width.map(|w| w.into()),
                                        height: info.height.map(|h| h.into()),
                                        size: info.size.map(|s| s.into()),
                                        mime_type: info.mimetype.map(|m| m.to_string()),
                                    },
                                )
                            }
                            MessageType::Location(content) => (
                                vec![RichTextSpan::Plain(content.body.clone())],
                                UiMessageType::Location(UiBeaconInfo {
                                    geo_uri: content.geo_uri,
                                    description: content.message.map(|v| {
                                        v.find_plain().map(|p| p.to_string()).unwrap_or_default()
                                    }),
                                    timestamp: content
                                        .ts
                                        .unwrap_or(MilliSecondsSinceUnixEpoch::now())
                                        .as_secs()
                                        .into(),
                                }),
                            ),
                            MessageType::Notice(content) => (
                                vec![RichTextSpan::Plain(content.body)],
                                UiMessageType::Notice,
                            ),
                            MessageType::ServerNotice(content) => (
                                vec![RichTextSpan::Plain(content.body)],
                                UiMessageType::ServerNotice {
                                    admin_contact: content.admin_contact.map(|c| c.to_string()),
                                    limit_msg: content.limit_type.map(|m| m.to_string()),
                                },
                            ),
                            MessageType::Text(content) => (
                                vec![RichTextSpan::Plain(content.body.clone())],
                                UiMessageType::Text,
                            ),
                            MessageType::Video(content) => {
                                let info = content.info.clone().unwrap_or_default();

                                (
                                    vec![RichTextSpan::Plain(content.body.clone())],
                                    UiMessageType::Video {
                                        source: content.source.clone().into(),
                                        filename: content.filename().to_string(),
                                        width: info.width.map(|w| w.into()),
                                        height: info.height.map(|h| h.into()),
                                        duration: info.duration.map(|d| d.as_secs().into()),
                                        size: info.size.map(|s| s.into()),
                                        mime_type: info.mimetype.map(|m| m.to_string()),
                                    },
                                )
                            }
                            _ => (
                                vec![RichTextSpan::Plain(
                                    content.as_message().unwrap().body().to_string(),
                                )],
                                UiMessageType::Text,
                            ),
                        }
                    }
                    MsgLikeKind::Sticker(sticker) => {
                        let content = sticker.content();
                        let info = content.info.clone();

                        (
                            vec![RichTextSpan::Plain(content.body.clone())],
                            UiMessageType::Sticker {
                                source: content.source.clone().into(),
                                width: info.width.map(|w| w.into()),
                                height: info.height.map(|h| h.into()),
                                size: info.size.map(|s| s.into()),
                                mime_type: info.mimetype.map(|m| m.to_string()),
                            },
                        )
                    }
                    MsgLikeKind::Poll(poll) => {
                        let result = poll.results();
                        (
                            vec![RichTextSpan::Plain(
                                poll.fallback_text().unwrap_or("Poll".to_string()),
                            )],
                            UiMessageType::Poll {
                                fallback_text: poll.fallback_text(),
                                is_edit: poll.is_edit(),
                                result: UiPollResult {
                                    question: result.question,
                                    kind: result.kind.into(),
                                    max_selections: result.max_selections,
                                    votes: result.votes,
                                    end_time: result.end_time.map(|t| t.as_secs().into()),
                                    has_been_edited: result.has_been_edited,
                                },
                            },
                        )
                    }
                    MsgLikeKind::Redacted => {
                        is_redacted = true;
                        (
                            vec![RichTextSpan::Plain("[redacted]".to_string())],
                            UiMessageType::Redacted,
                        )
                    }
                    MsgLikeKind::UnableToDecrypt(_) => (
                        vec![RichTextSpan::Plain("[unable to decrypt]".to_string())],
                        UiMessageType::FailedToDecrypt,
                    ),
                    MsgLikeKind::Other(_) => (
                        vec![RichTextSpan::Plain(
                            content.as_message().unwrap().body().to_string(),
                        )],
                        UiMessageType::Text,
                    ),
                    MsgLikeKind::LiveLocation(loc) => (
                        vec![RichTextSpan::Plain(
                            loc.latest_location()
                                .map(|l| l.description().unwrap_or("a location update").to_string())
                                .unwrap_or("Live location".to_string()),
                        )],
                        UiMessageType::LiveLocation {
                            locations: loc.locations().iter().map(|l| l.clone().into()).collect(),
                        },
                    ),
                };

                EventContent::MsgLike(Box::new(MessageContent {
                    reactions: get_reactions(content.clone().reactions),
                    in_reply_to: content.clone().in_reply_to.map(|v| v.event_id.to_string()),
                    thread_root: content.clone().thread_root.map(|v| v.to_string()),
                    is_edited,

                    is_redacted,
                    body,

                    msg_type,
                }))
            }
            TimelineItemContent::MembershipChange(change) => {
                EventContent::SystemMessage(SystemMessage::MembershipChange {
                    user_id: change.user_id().into(),
                    change: change.change().map(|v| v.into()),
                })
            }
            TimelineItemContent::CallInvite => {
                EventContent::SystemMessage(SystemMessage::CallInvite)
            }
            TimelineItemContent::RtcNotification {
                call_intent,
                declined_by,
            } => EventContent::SystemMessage(SystemMessage::RtcNotification {
                call_intent,
                declined_by: declined_by.iter().map(|v| v.to_string()).collect(),
            }),
            TimelineItemContent::ProfileChange(change) => {
                EventContent::SystemMessage(SystemMessage::ProfileChange {
                    user_id: change.user_id().to_string(),
                    display_name_change: change.displayname_change().map(|c| Change {
                        old: c.old.clone().map(|v| v.to_string()),
                        new: c.new.clone().map(|v| v.to_string()),
                    }),
                    avatar_url_changed: change.avatar_url_change().map(|c| Change {
                        old: c.old.clone().map(|v| v.to_string()),
                        new: c.new.clone().map(|v| v.to_string()),
                    }),
                })
            }
            TimelineItemContent::FailedToParseMessageLike { event_type, error } => {
                EventContent::FailedToParseMessageLike {
                    event_type: event_type.to_string(),
                    error: error.to_string(),
                }
            }
            TimelineItemContent::FailedToParseState {
                event_type,
                state_key,
                error,
            } => EventContent::FailedToParseState {
                event_type: event_type.to_string(),
                state_key,
                error: error.to_string(),
            },
            TimelineItemContent::OtherState(_) => {
                EventContent::SystemMessage(SystemMessage::OtherEvent)
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub enum DetailState<T> {
    Unavailable,
    Pending,
    Ready(T),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct TimelineEvent {
    pub state: Option<EventState>,
    pub timestamp: u64,
    pub flags: EventFlags,
    pub sender: DetailState<Sender>,

    pub content: EventContent,
}

impl TimelineEvent {
    pub fn is_sending(&self) -> bool {
        matches!(self.state, Some(EventState::NotSentYet { .. }))
    }

    /// Returns Some(error_message) if the message failed to send, None otherwise.
    pub fn get_failed_message(&self) -> Option<String> {
        if let Some(EventState::SendingFailed { error, .. }) = &self.state {
            Some(error.clone())
        } else {
            None
        }
    }

    pub fn get_reactions(&self) -> Option<HashMap<String, Vec<String>>> {
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
}

impl From<&EventTimelineItem> for TimelineEvent {
    fn from(item: &EventTimelineItem) -> Self {
        let sender_id = item.sender();

        TimelineEvent {
            state: item.send_state().map(|v| v.into()),
            timestamp: item.timestamp().as_secs().into(),
            flags: EventFlags {
                is_editable: item.is_editable(),
                is_highlighted: item.is_highlighted(),
                can_be_replied_to: item.can_be_replied_to(),
                contains_only_emojis: item.contains_only_emojis(),
            },
            sender: match item.sender_profile().clone() {
                TimelineDetails::Error(e) => DetailState::Error(e.to_string()),
                TimelineDetails::Pending => DetailState::Pending,
                TimelineDetails::Unavailable => DetailState::Unavailable,
                TimelineDetails::Ready(profile) => DetailState::Ready(Sender {
                    id: sender_id.into(),
                    display_name: profile.display_name,
                    avatar_url: profile.avatar_url.map(|u| u.to_string()),
                }),
            },

            content: item.content().into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub enum UiTimelineItemKind {
    Event(Box<TimelineEvent>),
    DateDivider(u64),
    ReadMarker,
    TimelineStart,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Hash)]
pub struct UiTimelineItem {
    pub id: String,
    pub render_key: Uuid,

    pub kind: UiTimelineItemKind,
}

impl From<&TimelineItem> for UiTimelineItem {
    fn from(item: &TimelineItem) -> Self {
        let kind = match item.kind() {
            TimelineItemKind::Virtual(event) => match event {
                VirtualTimelineItem::ReadMarker => UiTimelineItemKind::ReadMarker,
                VirtualTimelineItem::DateDivider(timestamp) => {
                    UiTimelineItemKind::DateDivider(timestamp.as_secs().into())
                }
                VirtualTimelineItem::TimelineStart => UiTimelineItemKind::TimelineStart,
            },
            TimelineItemKind::Event(event) => UiTimelineItemKind::Event(Box::new(event.into())),
        };

        UiTimelineItem {
            id: item.unique_id().clone().0,
            render_key: Uuid::new_v4(),

            kind,
        }
    }
}

impl From<&Arc<TimelineItem>> for UiTimelineItem {
    fn from(value: &Arc<TimelineItem>) -> Self {
        UiTimelineItem::from(value.as_ref())
    }
}

impl From<TimelineItem> for UiTimelineItem {
    fn from(value: TimelineItem) -> Self {
        UiTimelineItem::from(&value)
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

impl From<&VectorDiff<Arc<TimelineItem>>> for UiTimelineDiff {
    fn from(diff: &VectorDiff<Arc<TimelineItem>>) -> Self {
        match diff {
            VectorDiff::Append { values } => UiTimelineDiff::Append {
                values: values.iter().map(|v| v.into()).collect(),
            },
            VectorDiff::Clear => UiTimelineDiff::Clear,
            VectorDiff::PushFront { value } => UiTimelineDiff::PushFront {
                value: value.into(),
            },
            VectorDiff::PushBack { value } => UiTimelineDiff::PushBack {
                value: value.into(),
            },
            VectorDiff::PopFront => UiTimelineDiff::PopFront,
            VectorDiff::PopBack => UiTimelineDiff::PopBack,
            VectorDiff::Insert { index, value } => UiTimelineDiff::Insert {
                index: *index,
                value: value.into(),
            },
            VectorDiff::Set { index, value } => UiTimelineDiff::Set {
                index: *index,
                value: value.into(),
            },
            VectorDiff::Remove { index } => UiTimelineDiff::Remove { index: *index },
            VectorDiff::Truncate { length } => UiTimelineDiff::Truncate { length: *length },
            VectorDiff::Reset { values } => UiTimelineDiff::Reset {
                values: values.iter().map(|v| v.into()).collect(),
            },
        }
    }
}
