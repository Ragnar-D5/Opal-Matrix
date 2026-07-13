use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use matrix_sdk::{
    ruma::{
        events::{
            poll::start::PollKind,
            receipt::ReceiptThread,
            room::{
                guest_access::GuestAccess,
                history_visibility::HistoryVisibility,
                message::{MessageFormat, MessageType, Relation},
                MediaSource,
            },
            rtc::notification::CallIntent,
            AnySyncMessageLikeEvent, AnySyncTimelineEvent, StateEventContentChange, StateEventType,
            SyncMessageLikeEvent,
        },
        room::JoinRuleSummary,
        serde::Raw,
        EventId, MilliSecondsSinceUnixEpoch, OwnedEventId,
    },
    Room,
};
use matrix_sdk_ui::{
    eyeball_im::VectorDiff,
    timeline::{
        AnyOtherStateEventContentChange, BeaconInfo, EmbeddedEvent, EventSendState,
        EventTimelineItem, InReplyToDetails, MemberProfileChange, MembershipChange, MsgLikeKind,
        ReactionsByKeyBySender, TimelineDetails, TimelineItem, TimelineItemContent,
        TimelineItemKind, VirtualTimelineItem,
    },
};
use shared::{
    parsing::{parse_html_to_spans, parse_plain_text_to_spans},
    timeline::{
        AbstractProgress, Change, DetailState, EventContent, EventFlags, EventState,
        MediaUploadProgress, MessageContent, ProfileChange, ReactionInfo, ReplyInfo, ReplyPreview,
        RichTextSpan, SystemMessage, TimelineEvent, UiBeaconInfo, UiCallIntent, UiGuestAccess,
        UiHistoryVisibility, UiJoinRule, UiMediaSource, UiMembershipChange, UiMessageType,
        UiPollKind, UiPollResult, UiTimelineDiff, UiTimelineItem, UiTimelineItemKind,
    },
};
use uuid::Uuid;

fn membership_change_to_ui(value: MembershipChange) -> UiMembershipChange {
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

fn from_embedded_event_to_ui(value: &EmbeddedEvent) -> ReplyPreview {
    ReplyPreview {
        sender_id: value.sender.to_string(),
        content: reply_preview_spans(&value.content),
    }
}

fn reply_preview_spans(content: &TimelineItemContent) -> Vec<RichTextSpan> {
    match content.clone() {
        TimelineItemContent::CallInvite => vec![RichTextSpan::Plain("Call invite".to_string())],
        TimelineItemContent::FailedToParseMessageLike { event_type, error } => {
            vec![RichTextSpan::Plain(format!(
                "Failed to parse message like content of type {event_type}: {error}"
            ))]
        }
        TimelineItemContent::FailedToParseState {
            event_type,
            state_key,
            error,
        } => vec![RichTextSpan::Plain(format!(
            "Failed to parse state content of type {event_type} with state key {state_key}: {error}"
        ))],
        TimelineItemContent::MembershipChange(change) => vec![RichTextSpan::Plain(format!(
            "Membership change: {}",
            change
                .change()
                .map(membership_change_to_ui)
                .unwrap_or(UiMembershipChange::None)
                .display_string()
        ))],
        TimelineItemContent::OtherState(state) => vec![RichTextSpan::Plain(format!(
            "State event of type {:?}",
            state
        ))],
        TimelineItemContent::MsgLike(msglike) => match msglike.kind {
            MsgLikeKind::Message(msg) => match msg.msgtype() {
                MessageType::Audio(_) => vec![RichTextSpan::Plain("Audio message".to_string())],
                MessageType::Emote(content) => parse_plain_text_to_spans(&content.body),
                MessageType::File(content) => {
                    vec![RichTextSpan::Plain(content.filename().to_string())]
                }
                MessageType::Image(content) => {
                    vec![RichTextSpan::Plain(content.filename().to_string())]
                }
                MessageType::Location(content) => {
                    vec![RichTextSpan::Plain(content.body.clone())]
                }
                MessageType::Notice(content) => parse_plain_text_to_spans(&content.body),
                MessageType::ServerNotice(content) => parse_plain_text_to_spans(&content.body),
                MessageType::Text(content) => {
                    let formatted = content.formatted.clone();

                    if let Some(formatted) = formatted {
                        match formatted.format {
                            MessageFormat::Html => {
                                parse_html_to_spans(&formatted.body, &content.body)
                            }
                            _ => parse_plain_text_to_spans(&content.body),
                        }
                    } else {
                        parse_plain_text_to_spans(&content.body)
                    }
                }
                MessageType::Video(content) => {
                    vec![RichTextSpan::Plain(content.filename().to_string())]
                }
                _ => parse_plain_text_to_spans(msg.body()),
            },
            MsgLikeKind::Sticker(sticker) => {
                parse_plain_text_to_spans(sticker.content().body.as_str())
            }
            MsgLikeKind::Poll(poll) => vec![RichTextSpan::Plain(
                poll.fallback_text().unwrap_or("Poll".to_string()),
            )],
            MsgLikeKind::Redacted => vec![RichTextSpan::Plain("[redacted]".to_string())],
            MsgLikeKind::UnableToDecrypt(_) => {
                vec![RichTextSpan::Plain("[unable to decrypt]".to_string())]
            }
            MsgLikeKind::Other(_) => vec![RichTextSpan::Plain("Other event type".to_string())],
            MsgLikeKind::LiveLocation(loc) => vec![RichTextSpan::Plain(
                loc.latest_location()
                    .map(|l| l.description().unwrap_or("a location update").to_string())
                    .unwrap_or("Live location".to_string()),
            )],
        },
        TimelineItemContent::ProfileChange(change) => {
            let change = member_profile_change_to_ui(change);

            vec![RichTextSpan::Plain(change.display_string())]
        }
        TimelineItemContent::RtcNotification {
            call_intent,
            declined_by,
        } => {
            let intent_str = match call_intent {
                Some(intent) => format!("Call intent: {:?}", intent),
                None => "Call intent: None".to_string(),
            };

            let declined_by_str = if declined_by.is_empty() {
                "Declined by: None".to_string()
            } else {
                format!(
                    "Declined by: {}",
                    declined_by
                        .iter()
                        .map(|u| u.to_string())
                        .collect::<Vec<String>>()
                        .join(", ")
                )
            };

            vec![RichTextSpan::Plain(format!(
                "RTC Notification - {}. {}",
                intent_str, declined_by_str
            ))]
        }
    }
}

pub fn join_rule_to_ui(rule: JoinRuleSummary) -> UiJoinRule {
    match rule {
        JoinRuleSummary::Invite => UiJoinRule::Invite,
        JoinRuleSummary::Knock => UiJoinRule::Knock,
        JoinRuleSummary::KnockRestricted(_) => UiJoinRule::KnockRestricted,
        JoinRuleSummary::Private => UiJoinRule::Private,
        JoinRuleSummary::Public => UiJoinRule::Public,
        JoinRuleSummary::Restricted(_) => UiJoinRule::Restricted,
        _ => UiJoinRule::default(),
    }
}

pub fn history_visibility_to_ui(visibility: &HistoryVisibility) -> UiHistoryVisibility {
    match visibility {
        HistoryVisibility::Invited => UiHistoryVisibility::Invited,
        HistoryVisibility::Joined => UiHistoryVisibility::Joined,
        HistoryVisibility::Shared => UiHistoryVisibility::Shared,
        HistoryVisibility::WorldReadable => UiHistoryVisibility::WorldReadable,
        _ => UiHistoryVisibility::default(),
    }
}

pub async fn load_reply_info(room: &Room, raw: &Raw<AnySyncTimelineEvent>) -> Option<ReplyInfo> {
    let AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
        SyncMessageLikeEvent::Original(message),
    )) = raw.deserialize().ok()?
    else {
        return None;
    };

    let reply_event_id = match message.content.relates_to? {
        Relation::Reply(in_reply_to) => in_reply_to.in_reply_to.event_id,
        Relation::Thread(thread) => {
            if thread.is_falling_back {
                return None;
            }
            thread.in_reply_to?.event_id
        }
        _ => return None,
    };

    let unavailable = |event_id: &EventId| {
        Some(ReplyInfo {
            event_id: event_id.to_string(),
            event: DetailState::Unavailable,
        })
    };

    let replied = match room.event(&reply_event_id, None).await {
        Ok(event) => event,
        Err(e) => {
            log::warn!("Failed to fetch replied-to event {reply_event_id}: {e:?}");
            return unavailable(&reply_event_id);
        }
    };

    let Some(sender_id) = replied.sender().map(|id| id.to_string()) else {
        return unavailable(&reply_event_id);
    };
    let Some(content) = TimelineItemContent::from_event(room, replied).await else {
        return unavailable(&reply_event_id);
    };

    Some(ReplyInfo {
        event_id: reply_event_id.to_string(),
        event: DetailState::Ready(ReplyPreview {
            sender_id,
            content: reply_preview_spans(&content),
        }),
    })
}

fn in_reply_to_details_to_ui(
    value: InReplyToDetails,
    outer_event_id: Option<&EventId>,
    unknown_reply_event_ids: &mut HashSet<OwnedEventId>,
) -> ReplyInfo {
    ReplyInfo {
        event_id: value.event_id.to_string(),
        event: match value.event {
            TimelineDetails::Error(e) => DetailState::Error(e.to_string()),
            TimelineDetails::Pending => DetailState::Pending,
            TimelineDetails::Unavailable => {
                if let Some(id) = outer_event_id {
                    unknown_reply_event_ids.insert(id.to_owned());
                }
                DetailState::Unavailable
            }
            TimelineDetails::Ready(event) => DetailState::Ready(from_embedded_event_to_ui(&event)),
        },
    }
}

fn get_reactions(reactions: ReactionsByKeyBySender) -> HashMap<String, Vec<ReactionInfo>> {
    let mut reactions: Vec<(String, Vec<ReactionInfo>)> = reactions
        .iter()
        .map(|(key, by_sender)| {
            let mut reactors: Vec<ReactionInfo> = by_sender
                .iter()
                .map(|(sender, info)| ReactionInfo {
                    sender_id: sender.to_string(),
                    timestamp: info.timestamp.as_secs().into(),
                })
                .collect();

            reactors.sort_by_key(|r| r.timestamp);

            (key.clone(), reactors)
        })
        .collect();

    reactions
        .sort_by_key(|(_, reactors)| reactors.first().map(|r| r.timestamp).unwrap_or_default());

    reactions.into_iter().collect()
}

fn member_profile_change_to_ui(value: MemberProfileChange) -> ProfileChange {
    ProfileChange {
        user_id: value.user_id().to_string(),
        display_name_change: value.displayname_change().map(|c| Change {
            old: c.old.clone().map(|v| v.to_string()),
            new: c.new.clone().map(|v| v.to_string()),
        }),
        avatar_url_change: value.avatar_url_change().map(|c| Change {
            old: c.old.clone().map(|v| v.to_string()),
            new: c.new.clone().map(|v| v.to_string()),
        }),
    }
}

fn poll_kind_to_ui(value: PollKind) -> UiPollKind {
    match value {
        PollKind::Undisclosed => UiPollKind::Undisclosed,
        PollKind::Disclosed => UiPollKind::Disclosed,
        _ => UiPollKind::default(),
    }
}

fn beacon_info_to_ui(value: &BeaconInfo) -> UiBeaconInfo {
    UiBeaconInfo {
        geo_uri: value.geo_uri().to_string(),
        description: value.description().map(|d| d.to_string()),
        timestamp: value.ts().as_secs().into(),
    }
}

fn event_send_state_to_ui(state: &EventSendState) -> EventState {
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

fn from_call_intent_to_ui(value: CallIntent) -> UiCallIntent {
    match value {
        CallIntent::Audio => UiCallIntent::Audio,
        CallIntent::Video => UiCallIntent::Video,
        _ => UiCallIntent::Unknown,
    }
}

pub fn timeline_item_content_to_ui(
    value: &TimelineItemContent,
    media_store: &mut HashMap<Uuid, MediaSource>,
    outer_event_id: Option<&EventId>,
    unknown_reply_event_ids: &mut HashSet<OwnedEventId>,
) -> EventContent {
    match value.clone() {
        TimelineItemContent::MsgLike(content) => {
            let mut is_redacted = false;
            let mut is_edited = false;

            let (body, msg_type) = match content.kind.clone() {
                MsgLikeKind::Message(msg) => {
                    is_edited = msg.is_edited();

                    match msg.msgtype().clone() {
                        MessageType::Audio(content) => {
                            let media_id = Uuid::new_v4();
                            media_store.insert(media_id, content.source.clone());
                            (
                                parse_plain_text_to_spans(&content.body),
                                UiMessageType::Audio {
                                    source: UiMediaSource::Uuid(media_id),
                                    filename: content.filename().to_string(),
                                    duration: content
                                        .info
                                        .map(|v| v.duration.map(|d| d.as_secs()))
                                        .unwrap_or_default(),
                                },
                            )
                        }
                        MessageType::Emote(content) => (
                            parse_plain_text_to_spans(&content.body),
                            UiMessageType::Emote,
                        ),
                        MessageType::File(content) => {
                            let info = content.info.clone().unwrap_or_default();

                            let media_id = Uuid::new_v4();
                            media_store.insert(media_id, content.source.clone());

                            (
                                parse_plain_text_to_spans(&content.body),
                                UiMessageType::File {
                                    source: UiMediaSource::Uuid(media_id),
                                    filename: content.filename().to_string(),
                                    mime_type: info.mimetype.map(|m| m.to_string()),
                                    size: info.size.map(|s| s.into()),
                                },
                            )
                        }
                        MessageType::Image(content) => {
                            let info = content.info.clone().unwrap_or_default();

                            let media_id = Uuid::new_v4();
                            media_store.insert(media_id, content.source.clone());

                            let body = if content.filename() == content.body {
                                Vec::new()
                            } else {
                                parse_plain_text_to_spans(&content.body)
                            };

                            (
                                body,
                                UiMessageType::Image {
                                    filename: content.filename().to_string(),
                                    source: UiMediaSource::Uuid(media_id),
                                    width: info.width.map(|w| w.into()),
                                    height: info.height.map(|h| h.into()),
                                    size: info.size.map(|s| s.into()),
                                    mime_type: info.mimetype.map(|m| m.to_string()),
                                    blurhash: info.blurhash,
                                },
                            )
                        }
                        MessageType::Location(content) => (
                            parse_plain_text_to_spans(&content.body),
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
                            parse_plain_text_to_spans(&content.body),
                            UiMessageType::Notice,
                        ),
                        MessageType::ServerNotice(content) => (
                            parse_plain_text_to_spans(&content.body),
                            UiMessageType::ServerNotice {
                                admin_contact: content.admin_contact.map(|c| c.to_string()),
                                limit_msg: content.limit_type.map(|m| m.to_string()),
                            },
                        ),
                        MessageType::Text(content) => {
                            let formatted = content.formatted;

                            let spans = if let Some(formatted) = formatted {
                                match formatted.format {
                                    MessageFormat::Html => {
                                        parse_html_to_spans(&formatted.body, &content.body)
                                    }
                                    _ => parse_plain_text_to_spans(&content.body),
                                }
                            } else {
                                parse_plain_text_to_spans(&content.body)
                            };

                            (spans, UiMessageType::Text)
                        }
                        MessageType::Video(content) => {
                            let info = content.info.clone().unwrap_or_default();

                            let media_id = Uuid::new_v4();
                            media_store.insert(media_id, content.source.clone());

                            (
                                parse_plain_text_to_spans(&content.body),
                                UiMessageType::Video {
                                    source: UiMediaSource::Uuid(media_id),
                                    filename: content.filename().to_string(),
                                    width: info.width.map(|w| w.into()),
                                    height: info.height.map(|h| h.into()),
                                    duration: info.duration.map(|d| d.as_secs()),
                                    size: info.size.map(|s| s.into()),
                                    mime_type: info.mimetype.map(|m| m.to_string()),
                                },
                            )
                        }
                        _ => (
                            parse_plain_text_to_spans(content.as_message().unwrap().body()),
                            UiMessageType::Text,
                        ),
                    }
                }
                MsgLikeKind::Sticker(sticker) => {
                    let content = sticker.content();
                    let info = content.info.clone();

                    let media_id = Uuid::new_v4();
                    media_store.insert(media_id, content.source.clone().into());

                    (
                        vec![RichTextSpan::Plain(content.body.clone())],
                        UiMessageType::Sticker {
                            source: UiMediaSource::Uuid(media_id),
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
                                kind: poll_kind_to_ui(result.kind),
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
                        locations: loc.locations().iter().map(beacon_info_to_ui).collect(),
                    },
                ),
            };

            EventContent::MsgLike(Box::new(MessageContent {
                reactions: get_reactions(content.clone().reactions),
                in_reply_to: content
                    .clone()
                    .in_reply_to
                    .map(|v| in_reply_to_details_to_ui(v, outer_event_id, unknown_reply_event_ids)),
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
                change: change.change().map(membership_change_to_ui),
            })
        }
        TimelineItemContent::CallInvite => EventContent::SystemMessage(SystemMessage::CallInvite),
        TimelineItemContent::RtcNotification {
            call_intent,
            declined_by,
        } => EventContent::SystemMessage(SystemMessage::RtcNotification {
            call_intent: call_intent.map(from_call_intent_to_ui),
            declined_by: declined_by.iter().map(|v| v.to_string()).collect(),
        }),
        TimelineItemContent::ProfileChange(change) => EventContent::SystemMessage(
            SystemMessage::ProfileChange(member_profile_change_to_ui(change)),
        ),
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
        TimelineItemContent::OtherState(other) => match other.content() {
            AnyOtherStateEventContentChange::PolicyRuleRoom(change) => match change {
                StateEventContentChange::Original { .. } => {
                    EventContent::SystemMessage(SystemMessage::PolicyRuleRoom)
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::PolicyRuleServer(change) => match change {
                StateEventContentChange::Original { .. } => {
                    EventContent::SystemMessage(SystemMessage::PolicyRuleServer)
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::PolicyRuleUser(change) => match change {
                StateEventContentChange::Original { .. } => {
                    EventContent::SystemMessage(SystemMessage::PolicyRuleUser)
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomAvatar(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomAvatar {
                        url: content.url.clone().map(|u| u.to_string()),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomCanonicalAlias(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomCanonicalAlias {
                        alias: content.alias.clone().map(|a| a.to_string()),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomCreate(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomCreate {
                        additional_creators: content
                            .additional_creators
                            .iter()
                            .map(|u| u.to_string())
                            .collect(),
                        room_type: content.room_type.clone().map(|t| t.to_string()),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomEncryption(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomEncryption {
                        algorithm: content.algorithm.to_string(),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomGuestAccess(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomGuestAccess {
                        guest_access: match content.guest_access {
                            GuestAccess::CanJoin => UiGuestAccess::CanJoin,
                            GuestAccess::Forbidden => UiGuestAccess::Forbidden,
                            _ => UiGuestAccess::Unknown,
                        },
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomHistoryVisibility(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomHistoryVisibility {
                        visibility: history_visibility_to_ui(&content.history_visibility),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomJoinRules(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomJoinRules {
                        join_rule: join_rule_to_ui(content.join_rule.clone().into()),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomName(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomName {
                        name: content.name.clone(),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomPinnedEvents(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomPinnedEvents {
                        pinned_events: content.pinned.iter().map(|e| e.to_string()).collect(),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomPowerLevels(change) => match change {
                StateEventContentChange::Original { .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomPowerLevels)
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomServerAcl(change) => match change {
                StateEventContentChange::Original { .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomServerAcl)
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomThirdPartyInvite(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomThirdPartyInvite {
                        display_name: content.display_name.clone(),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomTombstone(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomTombstone {
                        body: content.body.clone(),
                        replacement_room: content.replacement_room.to_string(),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::RoomTopic(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::RoomTopic {
                        topic: content.topic.clone(),
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::SpaceChild(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::SpaceChild {
                        via: content.via.iter().map(|u| u.to_string()).collect(),
                        order: content.order.clone().map(|o| o.to_string()),
                        suggested: content.suggested,
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            AnyOtherStateEventContentChange::SpaceParent(change) => match change {
                StateEventContentChange::Original { content, .. } => {
                    EventContent::SystemMessage(SystemMessage::SpaceParent {
                        via: content.via.iter().map(|u| u.to_string()).collect(),
                        canonical: content.canonical,
                    })
                }
                StateEventContentChange::Redacted(_) => {
                    EventContent::SystemMessage(SystemMessage::Redacted)
                }
            },
            other => match other.event_type() {
                StateEventType::CallMember => {
                    EventContent::SystemMessage(SystemMessage::CallMember)
                }
                StateEventType::BeaconInfo => {
                    EventContent::SystemMessage(SystemMessage::BeaconInfo)
                }
                StateEventType::MemberHints => {
                    EventContent::SystemMessage(SystemMessage::MemberHints)
                }
                // StateEventType::RoomImagePack => {
                //     EventContent::SystemMessage(SystemMessage::RoomImagePack)
                // }
                other => {
                    log::warn!("Unhandled state event content change: {:?}", other);
                    EventContent::SystemMessage(SystemMessage::Unknown)
                }
            },
        },
    }
}

fn event_timeline_item_to_ui(
    item: &EventTimelineItem,
    media_store: &mut HashMap<Uuid, MediaSource>,
    unknown_reply_event_ids: &mut HashSet<OwnedEventId>,
) -> TimelineEvent {
    let sender_id = item.sender();

    let receipts = item
        .read_receipts()
        .iter()
        .filter_map(|(user_id, receipt)| {
            if matches!(
                receipt.thread,
                ReceiptThread::Main | ReceiptThread::Unthreaded
            ) {
                Some(user_id.to_string())
            } else {
                None
            }
        })
        .collect();

    let mut event = TimelineEvent {
        state: item.send_state().map(event_send_state_to_ui),
        timestamp: item.timestamp().as_secs().into(),
        flags: EventFlags {
            is_reactable: true,
            is_deletable: item.is_own(),
            is_editable: item.is_editable(),
            is_highlighted: item.is_highlighted(),
            can_be_replied_to: item.can_be_replied_to(),
            contains_only_emojis: item.contains_only_emojis(),
        },

        event_id: item.event_id().map(|e| e.to_string()),

        receipts,

        sender_id: sender_id.to_string(),

        content: timeline_item_content_to_ui(
            item.content(),
            media_store,
            item.event_id(),
            unknown_reply_event_ids,
        ),
    };

    event.calculate_flags(item.is_own());

    event
}

pub fn timeline_item_to_ui(
    item: &TimelineItem,
    media_store: &mut HashMap<Uuid, MediaSource>,
    uknown_reply_event_ids: &mut HashSet<OwnedEventId>,
) -> UiTimelineItem {
    let kind = match item.kind() {
        TimelineItemKind::Virtual(event) => match event {
            VirtualTimelineItem::ReadMarker => UiTimelineItemKind::ReadMarker,
            VirtualTimelineItem::DateDivider(timestamp) => {
                UiTimelineItemKind::DateDivider(timestamp.as_secs().into())
            }
            VirtualTimelineItem::TimelineStart => UiTimelineItemKind::TimelineStart,
        },
        TimelineItemKind::Event(event) => UiTimelineItemKind::Event(Box::new(
            event_timeline_item_to_ui(event, media_store, uknown_reply_event_ids),
        )),
    };

    UiTimelineItem {
        id: item.unique_id().clone().0,

        kind,
    }
}

pub fn timeline_diff_to_ui(
    diff: &VectorDiff<Arc<TimelineItem>>,
    media_store: &mut HashMap<Uuid, MediaSource>,
    unknown_reply_event_ids: &mut HashSet<OwnedEventId>,
) -> UiTimelineDiff {
    let mut to_ui =
        |item: &Arc<TimelineItem>| timeline_item_to_ui(item, media_store, unknown_reply_event_ids);

    match diff {
        VectorDiff::Append { values } => UiTimelineDiff::Append {
            values: values.iter().map(to_ui).collect(),
        },
        VectorDiff::Clear => UiTimelineDiff::Clear,
        VectorDiff::PushFront { value } => UiTimelineDiff::PushFront {
            value: to_ui(value),
        },
        VectorDiff::PushBack { value } => UiTimelineDiff::PushBack {
            value: to_ui(value),
        },
        VectorDiff::PopFront => UiTimelineDiff::PopFront,
        VectorDiff::PopBack => UiTimelineDiff::PopBack,
        VectorDiff::Insert { index, value } => UiTimelineDiff::Insert {
            index: *index,
            value: to_ui(value),
        },
        VectorDiff::Set { index, value } => UiTimelineDiff::Set {
            index: *index,
            value: to_ui(value),
        },
        VectorDiff::Remove { index } => UiTimelineDiff::Remove { index: *index },
        VectorDiff::Truncate { length } => UiTimelineDiff::Truncate { length: *length },
        VectorDiff::Reset { values } => UiTimelineDiff::Reset {
            values: values.iter().map(to_ui).collect(),
        },
    }
}
