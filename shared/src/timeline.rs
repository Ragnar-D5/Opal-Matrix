use std::collections::HashMap;

use macros::{EnumHashMap, TauriEvent};
use ruma::{
    EventEncryptionAlgorithm, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedRoomOrAliasId,
    OwnedServerName, OwnedSpaceChildOrder, OwnedUserId,
    events::{
        poll::start::PollKind,
        room::{MediaSource, guest_access::GuestAccess, history_visibility::HistoryVisibility},
        rtc::notification::CallIntent,
        sticker::StickerMediaSource,
    },
    room::{JoinRule, JoinRuleSummary, RoomType},
};
use serde::{Deserialize, Serialize};

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
        event_id: OwnedEventId,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct EventFlags {
    pub is_editable: bool,
    pub is_deletable: bool,
    pub is_reactable: bool,
    pub is_highlighted: bool,
    pub can_be_replied_to: bool,
    pub contains_only_emojis: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Change<T> {
    pub old: T,
    pub new: T,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RichTextSpan {
    Plain(String),
    UserMention {
        user_id: OwnedUserId,
        display_name: String,
    },
    RoomMention {
        room_id: OwnedRoomOrAliasId,
        display_name: String,
    },
    Link {
        url: String,
        text: Option<String>,
    },
    Newline,
    Highlight(String),
    Empty,
}

/// Splits the `Plain` spans so that every case-insensitive occurrence of one
/// of `words` becomes a `Highlight` span. `words` must be lowercase.
pub fn highlight_words(spans: Vec<RichTextSpan>, words: &[String]) -> Vec<RichTextSpan> {
    if words.is_empty() {
        return spans;
    }

    spans
        .into_iter()
        .flat_map(|span| match span {
            RichTextSpan::Plain(text) => highlight_plain(&text, words),
            other => vec![other],
        })
        .collect()
}

fn highlight_plain(text: &str, words: &[String]) -> Vec<RichTextSpan> {
    let mut lower = String::with_capacity(text.len());
    let mut offsets = Vec::with_capacity(text.len());
    for (i, c) in text.char_indices() {
        for lc in c.to_lowercase() {
            lower.push(lc);
            offsets.extend(std::iter::repeat_n(i, lc.len_utf8()));
        }
    }

    let mut spans = Vec::new();
    let mut pos = 0;
    let mut emitted = 0;

    while pos < lower.len() {
        // Earliest match of any word; on ties prefer the longest word.
        let next_match = words
            .iter()
            .enumerate()
            .filter(|(_, w)| !w.is_empty())
            .filter_map(|(word_index, w)| {
                lower[pos..]
                    .find(w.as_str())
                    .map(|i| (pos + i, std::cmp::Reverse(w.len()), word_index))
            })
            .min();

        let Some((start, std::cmp::Reverse(len), word_index)) = next_match else {
            break;
        };

        // Merge the words following the matched one in the query into the
        // highlight, as long as they also directly follow it in the text.
        let mut end = start + len;
        for next_word in &words[word_index + 1..] {
            let Some(extended) = extend_run(&lower, end, next_word) else {
                break;
            };
            end = extended;
        }

        let match_start = offsets[start];
        let mut match_end = offsets.get(end).copied().unwrap_or(text.len());
        if match_end <= match_start {
            let c = text[match_start..].chars().next();
            match_end = match_start + c.map_or(0, char::len_utf8);
        }

        if match_start > emitted {
            spans.push(RichTextSpan::Plain(text[emitted..match_start].into()));
        }
        if match_end > emitted {
            spans.push(RichTextSpan::Highlight(
                text[emitted.max(match_start)..match_end].into(),
            ));
            emitted = match_end;
        }

        pos = end;
    }

    if emitted < text.len() {
        spans.push(RichTextSpan::Plain(text[emitted..].into()));
    }

    spans
}

/// If `word` follows position `end` in `lower` with only whitespace in
/// between, returns the position right after it.
fn extend_run(lower: &str, end: usize, word: &str) -> Option<usize> {
    if word.is_empty() {
        return None;
    }

    let whitespace: usize = lower[end..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    if whitespace == 0 {
        return None;
    }

    let after = end + whitespace;
    lower[after..].starts_with(word).then(|| after + word.len())
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

/// Standard scale-method thumbnail sizes. Many homeservers (Synapse with the
/// default `dynamic_thumbnails: false`, notably) only generate thumbnails at a
/// fixed, admin-configured set of sizes and reject arbitrary dimensions with a
/// 400 `M_UNKNOWN`. Snapping requests up to the smallest preset that covers the
/// desired size keeps thumbnail requests working on servers that restrict this,
/// at the cost of occasionally fetching a larger-than-needed thumbnail (the
/// server's "scale" method preserves the source aspect ratio regardless, so the
/// extra bytes don't change the display proportions, just the resolution).
const THUMBNAIL_PRESETS: &[(u64, u64)] = &[(320, 240), (640, 480), (800, 600)];

pub fn snap_thumbnail_size(w: u64, h: u64) -> (u64, u64) {
    THUMBNAIL_PRESETS
        .iter()
        .find(|&&(pw, ph)| pw >= w && ph >= h)
        .copied()
        .unwrap_or(*THUMBNAIL_PRESETS.last().unwrap())
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct UiPollResult {
    pub question: String,
    pub kind: PollKind,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UiMediaSource(pub MediaSource);

impl From<MediaSource> for UiMediaSource {
    fn from(source: MediaSource) -> Self {
        UiMediaSource(source)
    }
}

impl From<StickerMediaSource> for UiMediaSource {
    fn from(source: StickerMediaSource) -> Self {
        UiMediaSource(source.into())
    }
}

impl UiMediaSource {
    pub fn inner(&self) -> &MediaSource {
        &self.0
    }
}

impl PartialEq for UiMediaSource {
    fn eq(&self, _: &Self) -> bool {
        true
    }
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
        blurhash: Option<String>,
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
    pub sender_id: OwnedUserId,
    pub content: Vec<RichTextSpan>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReplyInfo {
    pub event_id: OwnedEventId,
    pub event: DetailState<ReplyPreview>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReactionInfo {
    pub sender_id: OwnedUserId,
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
    pub url_previes: Vec<String>,

    pub msg_type: UiMessageType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ProfileChange {
    pub user_id: OwnedUserId,
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

pub trait JoinRuleExt {
    fn to_string(&self) -> String;
}

impl JoinRuleExt for JoinRuleSummary {
    fn to_string(&self) -> String {
        match self {
            JoinRuleSummary::Public => "Public".to_string(),
            JoinRuleSummary::Knock => "Knock".to_string(),
            JoinRuleSummary::Invite => "Invite".to_string(),
            JoinRuleSummary::Private => "Private".to_string(),
            JoinRuleSummary::Restricted(_) => "Restricted".to_string(),
            JoinRuleSummary::KnockRestricted(_) => "Knock Restricted".to_string(),
            _ => "Unknown".to_string(),
        }
    }
}

impl JoinRuleExt for JoinRule {
    fn to_string(&self) -> String {
        JoinRuleSummary::from(self.clone()).to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, EnumHashMap, Eq)]
pub enum SystemMessage {
    MembershipChange {
        user_id: OwnedUserId,
        change: Option<UiMembershipChange>,
    },
    ProfileChange(ProfileChange),
    CallInvite,
    CallMember,
    RtcNotification {
        call_intent: Option<CallIntent>,
        declined_by: Vec<OwnedUserId>,
    },
    PolicyRuleRoom,
    PolicyRuleServer,
    PolicyRuleUser,
    RoomAvatar {
        url: Option<OwnedMxcUri>,
    },
    RoomCanonicalAlias {
        alias: Option<String>,
    },
    RoomCreate {
        additional_creators: Vec<OwnedUserId>,
        room_type: Option<RoomType>,
    },
    RoomEncryption {
        algorithm: EventEncryptionAlgorithm,
    },
    RoomGuestAccess {
        guest_access: GuestAccess,
    },
    RoomHistoryVisibility {
        visibility: HistoryVisibility,
    },
    RoomJoinRules {
        join_rule: JoinRule,
    },
    RoomName {
        name: String,
    },
    RoomPinnedEvents {
        pinned_events: Vec<OwnedEventId>,
    },
    RoomPowerLevels,
    RoomServerAcl,
    RoomThirdPartyInvite {
        display_name: String,
    },
    RoomTombstone {
        body: String,
        replacement_room: OwnedRoomId,
    },
    RoomTopic {
        topic: String,
    },
    SpaceChild {
        via: Vec<OwnedServerName>,
        order: Option<OwnedSpaceChildOrder>,
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

impl EventContent {
    pub fn to_timeline_item(
        self,
        id: OwnedEventId,
        sender: OwnedUserId,
        timestamp: u64,
    ) -> UiTimelineItem {
        UiTimelineItem {
            id: id.to_string(),
            kind: UiTimelineItemKind::Event(Box::new(TimelineEvent {
                state: None,
                timestamp,
                flags: EventFlags::default(),
                sender_id: sender,
                event_id: Some(id),
                content: self,
                receipts: Vec::new(),
            })),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DetailState<T> {
    Unavailable,
    Pending,
    Ready(T),
    Error(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TimelineEvent {
    pub state: Option<EventState>,
    pub timestamp: u64,
    pub flags: EventFlags,
    pub sender_id: OwnedUserId,

    pub receipts: Vec<OwnedUserId>,

    pub event_id: Option<OwnedEventId>,

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

    pub fn get_sender_avatar_url(&self) -> String {
        format!("mxc://avatar/{}", self.sender_id)
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
        self.flags.is_deletable = is_own;
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum UiTimelineItemKind {
    Event(Box<TimelineEvent>),
    DateDivider(u64),
    ReadMarker,
    TimelineStart,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, TauriEvent)]
pub struct UiTimelineItem {
    pub id: String,

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

    pub fn render_key(&self) -> String {
        format!(
            "timeline-event-{}",
            if let UiTimelineItemKind::Event(event) = &self.kind
                && let Some(event_id) = &event.event_id
            {
                event_id.to_string()
            } else {
                self.id.to_string()
            }
        )
    }

    pub fn event_id(&self) -> Option<OwnedEventId> {
        if let UiTimelineItemKind::Event(event) = &self.kind {
            event.event_id.clone()
        } else {
            None
        }
    }

    pub fn as_system_message(&self) -> Option<&SystemMessage> {
        if let UiTimelineItemKind::Event(event) = &self.kind
            && let EventContent::SystemMessage(system_message) = &event.content
        {
            Some(system_message)
        } else {
            None
        }
    }

    /// Returns a copy of this item with every occurrence of one of `words`
    /// in the message body turned into a `Highlight` span. `words` must be
    /// lowercase.
    pub fn with_highlights(&self, words: &[String]) -> Self {
        let mut item = self.clone();
        if let UiTimelineItemKind::Event(event) = &mut item.kind
            && let EventContent::MsgLike(content) = &mut event.content
        {
            content.body = highlight_words(std::mem::take(&mut content.body), words);
        }
        item
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, TauriEvent)]
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
