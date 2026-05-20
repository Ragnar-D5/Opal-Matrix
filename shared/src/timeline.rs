use std::collections::HashMap;

use matrix_sdk::ruma::{
    MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedMxcUri, OwnedUserId,
    events::{
        MessageLikeEventType, StateEventType, room::message::MessageType,
        rtc::notification::CallIntent,
    },
};
use matrix_sdk_ui::timeline::{
    EventSendState, EventTimelineItem, MembershipChange, ReactionsByKeyBySender, TimelineItem,
    TimelineItemContent, TimelineItemKind, VirtualTimelineItem,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AbstractProgress {
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MediaUploadProgress {
    pub index: u64,
    pub progress: AbstractProgress,
}

/// State for messages which haven't been sent yet, or failed to send. This is used to show progress indicators for media uploads, and error messages for failed sends.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum EventState {
    NotSentYet {
        progress: Option<MediaUploadProgress>,
    },
    SendingFailed {
        error: String,
        is_recoverable: bool,
    },
    Sent {
        event_id: OwnedEventId,
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
                event_id: event_id.clone(),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sender {
    pub id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EventFlags {
    is_editable: bool,
    is_highlighted: bool,
    can_be_replied_to: bool,
    contains_only_emojis: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Change<T> {
    pub old: T,
    pub new: T,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemberProfileChange {
    pub display_name_change: Option<Change<Option<String>>>,
    pub avatar_url_changed: Option<Change<Option<OwnedMxcUri>>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InReplyToDetails {
    pub event_id: OwnedEventId,
    pub sender: Sender,
    pub content: Box<EventContent>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageContent {
    reactions: HashMap<String, Vec<OwnedUserId>>,
    in_reply_to: Option<OwnedEventId>,
    thread_root: Option<OwnedEventId>,

    msg_type: MessageType,
}

fn get_reactions(reactions: ReactionsByKeyBySender) -> HashMap<String, Vec<OwnedUserId>> {
    reactions
        .iter()
        .map(|(key, by_sender)| {
            let reactors: Vec<OwnedUserId> =
                by_sender.iter().map(|(sender, _)| sender.clone()).collect();
            (key.clone(), reactors)
        })
        .collect()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum EventContent {
    MsgLike(MessageContent),
    MembershipChange {
        user_id: OwnedUserId,
        change: Option<UiMembershipChange>,
    },
    ProfileChange(MemberProfileChange),
    FailedToParseMessageLike {
        event_type: MessageLikeEventType,
        error: String,
    },
    FailedToParseState {
        event_type: StateEventType,
        state_key: String,
        error: String,
    },
    CallInvite,
    OtherEvent,
    RtcNotification {
        call_intent: Option<CallIntent>,
        declined_by: Vec<OwnedUserId>,
    },
}

impl From<&TimelineItemContent> for EventContent {
    fn from(value: &TimelineItemContent) -> Self {
        match value.clone() {
            TimelineItemContent::MsgLike(content) => EventContent::MsgLike(MessageContent {
                reactions: get_reactions(content.clone().reactions),
                in_reply_to: content.clone().in_reply_to.map(|v| v.event_id),
                thread_root: content.clone().thread_root,
                msg_type: content.clone().as_message().unwrap().msgtype().clone(),
            }),
            TimelineItemContent::MembershipChange(change) => EventContent::MembershipChange {
                user_id: change.user_id().into(),
                change: change.change().map(|v| v.into()),
            },
            TimelineItemContent::CallInvite => EventContent::CallInvite,
            TimelineItemContent::RtcNotification {
                call_intent,
                declined_by,
            } => EventContent::RtcNotification {
                call_intent,
                declined_by,
            },
            TimelineItemContent::ProfileChange(change) => {
                EventContent::ProfileChange(MemberProfileChange {
                    display_name_change: change.displayname_change().map(|c| Change {
                        old: c.old.clone(),
                        new: c.new.clone(),
                    }),
                    avatar_url_changed: change.avatar_url_change().map(|c| Change {
                        old: c.old.clone(),
                        new: c.new.clone(),
                    }),
                })
            }
            TimelineItemContent::FailedToParseMessageLike { event_type, error } => {
                EventContent::FailedToParseMessageLike {
                    event_type,
                    error: error.to_string(),
                }
            }
            TimelineItemContent::FailedToParseState {
                event_type,
                state_key,
                error,
            } => EventContent::FailedToParseState {
                event_type,
                state_key,
                error: error.to_string(),
            },
            TimelineItemContent::OtherState(_) => EventContent::OtherEvent,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TimelineEvent {
    state: Option<EventState>,
    timestamp: MilliSecondsSinceUnixEpoch,
    flags: EventFlags,

    content: EventContent,
}

impl From<&EventTimelineItem> for TimelineEvent {
    fn from(item: &EventTimelineItem) -> Self {
        TimelineEvent {
            state: item.send_state().map(|v| v.into()),
            timestamp: item.timestamp(),
            flags: EventFlags {
                is_editable: item.is_editable(),
                is_highlighted: item.is_highlighted(),
                can_be_replied_to: item.can_be_replied_to(),
                contains_only_emojis: item.contains_only_emojis(),
            },

            content: item.content().into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum UiTimelineItemKind {
    Event(Box<TimelineEvent>),
    DateDivider(MilliSecondsSinceUnixEpoch),
    ReadMarker,
    TimelineStart,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiTimelineItem {
    pub id: String,

    pub kind: UiTimelineItemKind,
}

impl From<TimelineItem> for UiTimelineItem {
    fn from(item: TimelineItem) -> Self {
        let kind = match item.kind() {
            TimelineItemKind::Virtual(event) => match event {
                VirtualTimelineItem::ReadMarker => UiTimelineItemKind::ReadMarker,
                VirtualTimelineItem::DateDivider(timestamp) => {
                    UiTimelineItemKind::DateDivider(*timestamp)
                }
                VirtualTimelineItem::TimelineStart => UiTimelineItemKind::TimelineStart,
            },
            TimelineItemKind::Event(event) => UiTimelineItemKind::Event(Box::new(event.into())),
        };

        UiTimelineItem {
            id: item.unique_id().clone().0,

            kind,
        }
    }
}
