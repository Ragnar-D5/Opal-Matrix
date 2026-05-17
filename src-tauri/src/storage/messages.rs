use ego_tree::NodeRef;
use linkify::LinkFinder;
use ruma::events::room::history_visibility::HistoryVisibility;
use ruma::events::room::member::MembershipState;
use ruma::events::{
    room::{
        message::{MessageFormat, MessageType, Relation},
        MediaSource,
    },
    AnyStateEventContent, AnySyncMessageLikeEvent, AnySyncTimelineEvent,
};
use rusqlite::Connection;
use scraper::{Html, Node};
use serde_json::Value;
use shared::messages::{
    EncryptedFileInfo, MembershipAction, Mentions, MessageContent, MessageKind, MessageState,
    RichTextSpan, SystemMessage, UiMessage, UserMessage,
};

use crate::TauriError;

use super::DataBaseModel;

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub msg_type: String,
    pub raw_json: String,
    pub timestamp: u64,
    pub state: MessageState,
}

impl DataBaseModel for MessageRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS messages (
                    event_id TEXT PRIMARY KEY,
                    room_id TEXT NOT NULL,
                    sender TEXT NOT NULL,
                    msg_type TEXT NOT NULL,
                    raw_json TEXT NOT NULL,
                    timestamp INTEGER NOT NULL,
                    state TEXT NOT NULL,
                    FOREIGN KEY (room_id) REFERENCES rooms(room_id)
                );
            ",
        )?;
        Ok(())
    }
}

pub fn get_messages(
    conn: &Connection,
    room_id: &String,
    oldest_id: Option<String>,
    limit: usize,
) -> Result<Vec<MessageRow>, TauriError> {
    let mut messages = Vec::new();

    match oldest_id {
        Some(id) => {
            let mut stmt = conn.prepare(
                "SELECT event_id, room_id, sender, msg_type, raw_json, timestamp, state
                     FROM MESSAGES
                     WHERE room_id = ?1
                       AND (
                           timestamp < (SELECT timestamp FROM MESSAGES WHERE event_id = ?2)
                           OR (
                               timestamp = (SELECT timestamp FROM MESSAGES WHERE event_id = ?2)
                               AND event_id < ?2
                           )
                       )
                     ORDER BY timestamp DESC, event_id DESC
                     LIMIT ?3",
            )?;

            let rows = stmt.query_map(rusqlite::params![room_id, id, limit], |row| {
                let state: String = row.get(6)?;
                Ok(MessageRow {
                    event_id: row.get(0)?,
                    room_id: row.get(1)?,
                    sender: row.get(2)?,
                    msg_type: row.get(3)?,
                    raw_json: row.get(4)?,
                    timestamp: row.get(5)?,
                    state: state.into(),
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT event_id, room_id, sender, msg_type, raw_json, timestamp, state
            FROM MESSAGES
            WHERE room_id = ?
            ORDER BY timestamp DESC
            LIMIT ?",
            )?;

            let rows = stmt.query_map(rusqlite::params![room_id, limit], |row| {
                let state: String = row.get(6)?;
                Ok(MessageRow {
                    event_id: row.get(0)?,
                    room_id: row.get(1)?,
                    sender: row.get(2)?,
                    msg_type: row.get(3)?,
                    raw_json: row.get(4)?,
                    timestamp: row.get(5)?,
                    state: state.into(),
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?);
            }
        }
    }

    Ok(messages)
}

pub fn save_messages(conn: &mut Connection, messages: Vec<MessageRow>) -> Result<(), TauriError> {
    let tx = conn.transaction()?;

    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO messages (event_id, room_id, sender, msg_type, raw_json, timestamp, state)
            VALUES (?, ?, ?, ?, ?, ?, ?)",
        )?;

        for msg in messages {
            stmt.execute(rusqlite::params![
                msg.event_id,
                msg.room_id,
                msg.sender,
                msg.msg_type,
                msg.raw_json,
                msg.timestamp,
                msg.state.to_string(),
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn message_exists(conn: &Connection, event_id: &str) -> Result<bool, TauriError> {
    let mut stmt = conn.prepare("SELECT 1 FROM messages WHERE event_id = ? LIMIT 1")?;
    let exists = stmt.exists(rusqlite::params![event_id])?;
    Ok(exists)
}

fn walk_node(node: NodeRef<'_, Node>, spans: &mut Vec<RichTextSpan>) {
    match node.value() {
        Node::Text(text) => {
            let content = text.text.to_string();
            if !content.is_empty() {
                spans.push(RichTextSpan::Plain(content));
            }
        }

        Node::Element(elem) => {
            let tag_name = elem.name();

            if tag_name == "br" {
                spans.push(RichTextSpan::Plain("\n".to_string()));
                return;
            }

            if tag_name == "a" {
                if let Some(href) = elem.attr("href") {
                    if href.starts_with("https://matrix.to/#/@") || href.starts_with("matrix:u/") {
                        let user_id = extract_mxid(href);
                        let display_name = extract_inner_text(node);

                        spans.push(RichTextSpan::UserMention {
                            user_id,
                            display_name,
                        });
                        return; // Stop recursing; we consumed the children for the display name
                    } else if href.starts_with("https://matrix.to/#/#") {
                        spans.push(RichTextSpan::RoomMention {});
                        return;
                    } else {
                        spans.push(RichTextSpan::Link {
                            url: href.to_string(),
                            text: None,
                        });
                        return;
                    }
                }
            }

            for child in node.children() {
                walk_node(child, spans);
            }
        }

        _ => {
            for child in node.children() {
                walk_node(child, spans);
            }
        }
    }
}

fn extract_inner_text(node: NodeRef<'_, Node>) -> String {
    let mut text = String::new();
    for child in node.children() {
        if let Node::Text(t) = child.value() {
            text.push_str(&t.text);
        } else {
            text.push_str(&extract_inner_text(child));
        }
    }
    text
}

fn extract_mxid(href: &str) -> String {
    if let Some(idx) = href.find('@') {
        href[idx..].to_string()
    } else {
        href.to_string()
    }
}

fn parse_html_to_spans(html: &str, fallback_body: &str) -> Vec<RichTextSpan> {
    let document = Html::parse_fragment(html);
    let mut spans = Vec::new();

    for node in document.tree.root().children() {
        walk_node(node, &mut spans);
    }

    if spans.is_empty() {
        vec![RichTextSpan::Plain(fallback_body.to_string())]
    } else {
        spans
    }
}

pub fn parse_plain_text_to_spans(text: &str) -> Vec<RichTextSpan> {
    let mut spans = Vec::new();
    let mut finder = LinkFinder::new();
    finder.kinds(&[linkify::LinkKind::Url]);

    let mut last_end = 0;

    for link in finder.links(text) {
        if link.start() > last_end {
            spans.push(RichTextSpan::Plain(
                text[last_end..link.start()].to_string(),
            ));
        }

        spans.push(RichTextSpan::Link {
            url: link.as_str().to_string(),
            text: None,
        });

        last_end = link.end();
    }

    if last_end < text.len() {
        spans.push(RichTextSpan::Plain(text[last_end..].to_string()));
    }

    if spans.is_empty() {
        vec![RichTextSpan::Plain(text.to_string())]
    } else {
        spans
    }
}

impl TryInto<UiMessage> for MessageRow {
    type Error = TauriError;

    fn try_into(self) -> Result<UiMessage, Self::Error> {
        let value: Value = serde_json::from_str(&self.raw_json)?;
        let content = value
            .get("content")
            .ok_or(format!("Missing content: {:?}", value))?;

        let state = self.state;

        let mut msg = UiMessage {
            event_id: self.event_id,
            state: state,
            timestamp: self.timestamp,
            sender_id: self.sender.clone(),
            kind: MessageKind::SystemMessage(SystemMessage::Unknown),
        };

        let event: AnySyncTimelineEvent = serde_json::from_str(&self.raw_json)?;

        let message_kind = match event {
            AnySyncTimelineEvent::MessageLike(ev) => match ev {
                AnySyncMessageLikeEvent::RoomMessage(ev) => {
                    if let Some(or) = ev.as_original() {
                        let mentions: Mentions = or
                            .content
                            .mentions
                            .clone()
                            .map(|v| Mentions {
                                room: v.room,
                                user_ids: v.user_ids.iter().map(|x| x.to_string()).collect(),
                            })
                            .unwrap_or_default();

                        // let replies = or.content.relates_to.clone().map(|v| {
                        //     if let Relation::Reply {
                        //         in_reply_to: event_id,
                        //     } = v
                        //     {
                        //         RepliesTo {
                        //             event_id: event_id.to_string(),
                        //             text: None,
                        //             sender_id: None,
                        //         }
                        //     } else {
                        //         RepliesTo {
                        //             event_id: "".to_string(),
                        //             text: None,
                        //             sender_id: None,
                        //         }
                        //     }
                        // });

                        let mut user_message = UserMessage::new();

                        if let Some(Relation::Reply { in_reply_to }) = or.content.relates_to.clone()
                        {
                            user_message.set_replies_to(in_reply_to.event_id.to_string());
                        };

                        user_message.set_mentions(mentions);

                        user_message.content = match or.content.msgtype.clone() {
                            MessageType::Text(text_content) => {
                                let body = text_content.body.clone();
                                let spans = if let Some(formatted) = text_content.formatted.clone()
                                {
                                    match formatted.format {
                                        MessageFormat::Html => {
                                            parse_html_to_spans(&formatted.body, &body)
                                        }
                                        _ => parse_plain_text_to_spans(&body),
                                    }
                                } else {
                                    parse_plain_text_to_spans(&body)
                                };

                                MessageContent::Text {
                                    spans,
                                    is_edited: false,
                                }
                            }
                            MessageType::Image(image_content) => {
                                // TODO: Use the actual content instead of raw json for this
                                let encryption_info = if let Some(file_obj) = content.get("file") {
                                    Some(EncryptedFileInfo {
                                        key: file_obj
                                            .get("key")
                                            .and_then(|k| k.get("k"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        iv: file_obj
                                            .get("iv")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                        hash: file_obj
                                            .get("hashes")
                                            .and_then(|h| h.get("sha256"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string(),
                                    })
                                } else {
                                    None
                                };
                                let url = match image_content.source.clone() {
                                    MediaSource::Plain(url) => url.to_string(),
                                    MediaSource::Encrypted(file) => file.url.to_string(),
                                };

                                let info = image_content.info.clone();

                                MessageContent::Image {
                                    name: image_content.filename().to_string(),
                                    width: info
                                        .clone()
                                        .map(|v| v.height.map(|x| x.try_into().unwrap_or(0)))
                                        .unwrap_or_default(),
                                    height: info
                                        .map(|v| v.width.map(|x| x.try_into().unwrap_or(0)))
                                        .unwrap_or_default(),
                                    url,
                                    encryption_info,
                                }
                            }
                            MessageType::VerificationRequest(ver_content) => {
                                msg.kind = MessageKind::SystemMessage(
                                    SystemMessage::VerificationRequest {
                                        methods: ver_content
                                            .methods
                                            .iter()
                                            .map(|v| v.to_string())
                                            .collect(),
                                        from_user_id: ver_content.to.to_string(),
                                    },
                                );

                                return Ok(msg);
                            }
                            other => {
                                return Err(format!("Unsupported message type: {:?}", other).into());
                            }
                        };
                        MessageKind::UserMessage(user_message)
                    } else {
                        MessageKind::UserMessage(UserMessage::deleted())
                    }
                }
                AnySyncMessageLikeEvent::Reaction(react_ev) => {
                    if let Some(or) = react_ev.as_original() {
                        let annotation = or.content.relates_to.clone();

                        MessageKind::SystemMessage(SystemMessage::MessageReacted {
                            event_id: annotation.event_id.to_string(),
                            reaction: annotation.key,
                        })
                    } else {
                        MessageKind::SystemMessage(SystemMessage::Unknown)
                    }
                }
                AnySyncMessageLikeEvent::RoomRedaction(redact_ev) => {
                    if let Some(or) = redact_ev.as_original() {
                        if let Some(event_id) = &or.content.redacts {
                            MessageKind::SystemMessage(SystemMessage::MessageRedacted {
                                event_id: event_id.to_string(),
                                reason: or.content.reason.clone(),
                            })
                        } else {
                            MessageKind::SystemMessage(SystemMessage::Unknown)
                        }
                    } else {
                        MessageKind::SystemMessage(SystemMessage::Unknown)
                    }
                }
                AnySyncMessageLikeEvent::RoomEncrypted(_) => {
                    MessageKind::UserMessage(UserMessage {
                        mentions: Mentions::default(),
                        reactions: Vec::new(),
                        replies_to: None,
                        content: MessageContent::Encrypted,
                    })
                }
                AnySyncMessageLikeEvent::RtcNotification(_)
                | AnySyncMessageLikeEvent::CallInvite(_)
                | AnySyncMessageLikeEvent::CallNotify(_)
                | AnySyncMessageLikeEvent::CallAnswer(_)
                | AnySyncMessageLikeEvent::CallSelectAnswer(_)
                | AnySyncMessageLikeEvent::CallNegotiate(_)
                | AnySyncMessageLikeEvent::CallCandidates(_)
                | AnySyncMessageLikeEvent::KeyVerificationMac(_)
                | AnySyncMessageLikeEvent::KeyVerificationKey(_)
                | AnySyncMessageLikeEvent::KeyVerificationStart(_)
                | AnySyncMessageLikeEvent::KeyVerificationAccept(_)
                | AnySyncMessageLikeEvent::KeyVerificationDone(_)
                | AnySyncMessageLikeEvent::KeyVerificationCancel(_)
                | AnySyncMessageLikeEvent::KeyVerificationReady(_)
                | AnySyncMessageLikeEvent::CallHangup(_)
                | AnySyncMessageLikeEvent::RtcDecline(_) => {
                    return Err(TauriError::silent());
                }
                _ => return Err(format!("Unsupported message event type: {:?}", ev).into()),
            },
            AnySyncTimelineEvent::State(ev) => {
                if let Some(or) = ev.original_content() {
                    let state_key = ev.state_key().to_string();

                    let message = match or {
                        AnyStateEventContent::RoomMember(member_ev) => match member_ev.membership {
                            MembershipState::Ban => {
                                SystemMessage::MembershipChange(MembershipAction::Banned {
                                    target_id: state_key,
                                    reason: member_ev.reason.clone(),
                                })
                            }
                            MembershipState::Invite => SystemMessage::MembershipChange(
                                MembershipAction::Invited(state_key),
                            ),
                            MembershipState::Join => {
                                SystemMessage::MembershipChange(MembershipAction::Joined)
                            }
                            MembershipState::Leave => {
                                if &state_key == &self.sender {
                                    SystemMessage::MembershipChange(MembershipAction::Left)
                                } else {
                                    SystemMessage::MembershipChange(MembershipAction::Kicked {
                                        target_id: state_key,
                                        reason: member_ev.reason.clone(),
                                    })
                                }
                            }
                            _ => {
                                return Err(format!(
                                    "Unsupported membership state: {:?}; raw: {}",
                                    member_ev.membership, self.raw_json
                                )
                                .into())
                            }
                        },
                        // TODO: Actually use the event
                        AnyStateEventContent::CallMember(_) => {
                            if content.as_object().map_or(true, |obj| obj.is_empty()) {
                                SystemMessage::CallLeft
                            } else {
                                let intent = content
                                    .get("m.call.intent")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("audio")
                                    .to_string();

                                SystemMessage::CallJoined { intent }
                            }
                        }
                        AnyStateEventContent::RoomAvatar(av_ev) => {
                            SystemMessage::RoomAvatarChange {
                                new_avatar_url: av_ev.url.clone().map(|u| u.to_string()),
                            }
                        }
                        AnyStateEventContent::RoomTopic(topic_ev) => SystemMessage::TopicChange {
                            new_topic: topic_ev.topic.clone(),
                        },
                        AnyStateEventContent::RoomName(name_ev) => SystemMessage::RoomNameChange {
                            new_name: name_ev.name.clone(),
                        },
                        AnyStateEventContent::RoomJoinRules(join_ev) => {
                            SystemMessage::JoinRuleChange {
                                new_rule: join_ev.join_rule,
                            }
                        }
                        AnyStateEventContent::RoomHistoryVisibility(vis_ev) => {
                            let his_vis: HistoryVisibility = vis_ev.history_visibility.clone();
                            SystemMessage::HistoryVisibilityChange {
                                new_visibility: his_vis,
                            }
                        }
                        AnyStateEventContent::RoomGuestAccess(guest_ev) => {
                            SystemMessage::GuestAccessChange {
                                new_access: guest_ev.guest_access,
                            }
                        }
                        AnyStateEventContent::RoomCreate(_) => SystemMessage::RoomCreated {
                            creator_id: ev.sender().to_string(),
                        },
                        AnyStateEventContent::SpaceChild(_)
                        | AnyStateEventContent::RoomEncryption(_)
                        | AnyStateEventContent::SpaceParent(_)
                        | AnyStateEventContent::RoomPowerLevels(_)
                        | AnyStateEventContent::RoomCanonicalAlias(_) => {
                            return Err(TauriError::silent());
                        }
                        _ => match self.msg_type.as_str() {
                            "org.matrix.room.preview_urls" => {
                                return Err(TauriError::silent());
                            }
                            _ => {
                                return Err(format!(
                                    "Unsupported state event: {:?}, {:?}",
                                    or, self.msg_type
                                )
                                .into())
                            }
                        },
                    };

                    MessageKind::SystemMessage(message)
                } else {
                    MessageKind::SystemMessage(SystemMessage::Unknown)
                }
            }
        };

        msg.kind = message_kind;

        // let message_kind = match self.msg_type.as_str() {
        //     "m.room.message" | "m.room.message.failed" => {
        //         let mut user_message = UserMessage::new();

        //         if let Some(relates_to) = content.get("m.relates_to") {
        //             if relates_to.get("rel_type").and_then(|v| v.as_str()) == Some("m.replace") {
        //                 let event_id = relates_to
        //                     .get("event_id")
        //                     .and_then(|v| v.as_str())
        //                     .unwrap_or("")
        //                     .to_string();

        //                 let new_text = content
        //                     .get("m.new_content")
        //                     .and_then(|nc| nc.get("body"))
        //                     .and_then(|v| v.as_str())
        //                     .unwrap_or("[Empty Edit]")
        //                     .to_string();

        //                 return Ok(UiMessage {
        //                     event_id: self.event_id,
        //                     state: state,
        //                     timestamp: self.timestamp,
        //                     sender_id: self.sender,
        //                     kind: MessageKind::SystemMessage(SystemMessage::MessageEdited {
        //                         event_id,
        //                         new_spans: vec![RichTextSpan::Plain(new_text)],
        //                     }),
        //                 });
        //             }

        //             if let Some(in_reply_to) = relates_to.get("m.in_reply_to") {
        //                 if let Some(event_id) = in_reply_to.get("event_id").and_then(|v| v.as_str())
        //                 {
        //                     user_message.set_replies_to(event_id.to_string());
        //                 }
        //             }
        //         };

        //         let Some(msg_type) = content
        //             .get("msgtype")
        //             .and_then(|v| v.as_str()) else {
        //                 return Ok(UiMessage::deleted(self.event_id, self.timestamp, self.sender));
        //         };
        //         let mentions = content
        //             .get("m.mentions")
        //             .and_then(|v| serde_json::from_value(v.clone()).ok())
        //             .unwrap_or_default();
        //         user_message.set_mentions(mentions);

        //         match msg_type {
        //             "m.text" => {
        //                 let body = content
        //                     .get("body")
        //                     .and_then(|v| v.as_str())
        //                     .ok_or(format!("Missing body: {:?}", value))?
        //                     .to_string();

        //                 let format = content.get("format").and_then(|v| v.as_str());
        //                 let formatted_body = content
        //                     .get("formatted_body")
        //                     .and_then(|v| v.as_str())
        //                     .map(|s| s.to_string());

        //                 let spans = if format == Some("org.matrix.custom.html")
        //                     && let Some(html) = formatted_body
        //                 {
        //                     log::info!("html: {html}");
        //                     parse_html_to_spans(&html, &body)
        //                 } else {
        //                     parse_plain_text_to_spans(&body)
        //                 };

        //                 user_message.set_content(MessageContent::Text {
        //                     spans,
        //                     is_edited: false,
        //                 });

        //                 MessageKind::UserMessage(user_message)
        //             }
        //             "m.image" => {
        //                 let info = content
        //                     .get("info")
        //                     .ok_or(format!("Missing info: {:?}", value))?;
        //                 let url = content
        //                     .get("url")
        //                     .and_then(|v| v.as_str())
        //                     .or_else(|| {
        //                         content
        //                             .get("file")
        //                             .and_then(|f| f.get("url"))
        //                             .and_then(|v| v.as_str())
        //                     })
        //                     .ok_or_else(|| format!("Missing url in image: {:?}", content))?;

        //                 let encryption_info = if let Some(file_obj) = content.get("file") {
        //                     Some(EncryptedFileInfo {
        //                         key: file_obj
        //                             .get("key")
        //                             .and_then(|k| k.get("k"))
        //                             .and_then(|v| v.as_str())
        //                             .unwrap_or("")
        //                             .to_string(),
        //                         iv: file_obj
        //                             .get("iv")
        //                             .and_then(|v| v.as_str())
        //                             .unwrap_or("")
        //                             .to_string(),
        //                         hash: file_obj
        //                             .get("hashes")
        //                             .and_then(|h| h.get("sha256"))
        //                             .and_then(|v| v.as_str())
        //                             .unwrap_or("")
        //                             .to_string(),
        //                     })
        //                 } else {
        //                     None
        //                 };

        //                 user_message.set_content(MessageContent::Image {
        //                     name: content
        //                         .get("body")
        //                         .and_then(|v| v.as_str())
        //                         .unwrap_or("image")
        //                         .to_string(),
        //                     url: url.to_string(),
        //                     width: info.get("w").and_then(|v| v.as_u64()).map(|n| n as u32),
        //                     height: info.get("h").and_then(|v| v.as_u64()).map(|n| n as u32),

        //                     encryption_info: encryption_info,
        //                 });

        //                 MessageKind::UserMessage(user_message)
        //             }
        //             _ => {
        //                 debug!("Unsupported msgtype: {}", msg_type);

        //                 user_message.set_content(MessageContent::Text {
        //                     spans: vec![RichTextSpan::Plain(
        //                         content
        //                             .get("body")
        //                             .and_then(|v| v.as_str())
        //                             .unwrap_or("[Unsupported message type]")
        //                             .to_string(),
        //                     )],
        //                     is_edited: false,
        //                 });

        //                 MessageKind::UserMessage(user_message)
        //             }
        //         }
        //     }
        //     "m.room.member" => {
        //         let membership = content
        //             .get("membership")
        //             .and_then(|v| v.as_str())
        //             .ok_or(format!("Missing membership: {:?}", value))?;
        //         let state_key = value
        //             .get("state_key")
        //             .and_then(|v| v.as_str())
        //             .ok_or(format!("Missing state key: {:?}", value))?
        //             .to_string();

        //         MessageKind::SystemMessage(SystemMessage::MembershipChange(match membership {
        //             "join" => MembershipAction::Joined,
        //             "invite" => MembershipAction::Invited(state_key),
        //             "leave" => {
        //                 if state_key == self.sender {
        //                     MembershipAction::Left
        //                 } else {
        //                     MembershipAction::Kicked {
        //                         target_id: state_key,
        //                         reason: content
        //                             .get("reason")
        //                             .and_then(|v| v.as_str())
        //                             .map(|s| s.to_string()),
        //                     }
        //                 }
        //             }
        //             "ban" => MembershipAction::Banned {
        //                 target_id: state_key,
        //                 reason: content
        //                     .get("reason")
        //                     .and_then(|v| v.as_str())
        //                     .map(|s| s.to_string()),
        //             },
        //             _ => {
        //                 return Err(format!("Unknown membership: {membership}; {:?}", value).into());
        //             }
        //         }))
        //     }
        //     "m.room.create" => MessageKind::SystemMessage(SystemMessage::RoomCreation),
        //     "m.room.name" => MessageKind::SystemMessage(SystemMessage::RoomNameChange {
        //         new_name: content
        //             .get("name")
        //             .and_then(|v| v.as_str())
        //             .ok_or(format!("Missing name: {:?}", value))?
        //             .to_string(),
        //     }),
        //     "m.room.topic" => MessageKind::SystemMessage(SystemMessage::TopicChange {
        //         new_topic: content
        //             .get("topic")
        //             .and_then(|v| v.as_str())
        //             .ok_or(format!("Missing topic: {:?}", value))?
        //             .to_string(),
        //     }),
        //     "m.room.encryption" => MessageKind::SystemMessage(SystemMessage::EncryptionEnabled {
        //         algorithm: content
        //             .get("algorithm")
        //             .and_then(|v| v.as_str())
        //             .unwrap_or("unknown")
        //             .to_string(),
        //     }),
        //     "m.room.power_levels" => MessageKind::SystemMessage(SystemMessage::PowerlevelChange),
        //     "m.room.join_rules" => MessageKind::SystemMessage(SystemMessage::JoinRuleChange {
        //         new_rule: content
        //             .get("join_rule")
        //             .and_then(|v| v.as_str())
        //             .unwrap_or("unknown")
        //             .to_string(),
        //     }),
        //     "m.room.history_visibility" => {
        //         MessageKind::SystemMessage(SystemMessage::HistoryVisibilityChange {
        //             new_visibility: content
        //                 .get("history_visibility")
        //                 .and_then(|v| v.as_str())
        //                 .unwrap_or("unknown")
        //                 .to_string(),
        //         })
        //     }
        //     "m.room.guest_access" => MessageKind::SystemMessage(SystemMessage::GuestAccessChange {
        //         new_access: content
        //             .get("guest_access")
        //             .and_then(|v| v.as_str())
        //             .unwrap_or("unknown")
        //             .to_string(),
        //     }),
        //     "m.room.encrypted" => MessageKind::UserMessage(UserMessage {
        //         mentions: Mentions::default(),
        //         reactions: Vec::new(),
        //         replies_to: None,

        //         content: MessageContent::Encrypted,
        //     }),
        //     "org.matrix.msc3401.call.member" => {
        //         if content.as_object().map_or(true, |obj| obj.is_empty()) {
        //             MessageKind::SystemMessage(SystemMessage::CallLeft)
        //         } else {
        //             let intent = content
        //                 .get("m.call.intent")
        //                 .and_then(|v| v.as_str())
        //                 .unwrap_or("audio")
        //                 .to_string();

        //             MessageKind::SystemMessage(SystemMessage::CallJoined { intent })
        //         }
        //     }
        //     "m.reaction" => {
        //         let event_id = content
        //             .get("m.relates_to")
        //             .and_then(|r| r.get("event_id"))
        //             .and_then(|v| v.as_str())
        //             .unwrap_or("")
        //             .to_string();

        //         let reaction = content
        //             .get("m.relates_to")
        //             .and_then(|r| r.get("key"))
        //             .and_then(|v| v.as_str())
        //             .unwrap_or("")
        //             .to_string();

        //         MessageKind::SystemMessage(SystemMessage::MessageReacted { event_id, reaction })
        //     }
        //     "m.room.redaction" => {
        //         let event_id = value
        //             .get("redacts")
        //             .and_then(|v| v.as_str())
        //             .unwrap_or("")
        //             .to_string();

        //         MessageKind::SystemMessage(SystemMessage::MessageRedacted { event_id })
        //     }
        //     "m.room.avatar" => MessageKind::SystemMessage(SystemMessage::RoomAvatarChange { new_avatar_url: content.get("url").map(|v| v.to_string()) }
        //     ),
        //     "m.call.invite"
        //     | "org.matrix.msc4075.call.notify"
        //     | "org.matrix.msc4075.rtc.notification" => {
        //         return Err(TauriError::silent());
        //     }
        //     _ => {
        //         warn!("Unsupported message type: {}; {:?}", self.msg_type, value);

        //         return Err(TauriError::silent());
        //     }
        // };

        return Ok(msg);
    }
}

pub fn delete_message(conn: &Connection, event_id: &str) -> Result<(), TauriError> {
    conn.execute(
        "DELETE FROM messages WHERE event_id = ?",
        rusqlite::params![event_id],
    )?;
    Ok(())
}
pub fn set_message_state(
    conn: &Connection,
    event_id: &str,
    state: MessageState,
) -> Result<(), TauriError> {
    let string = state.to_string();

    conn.execute(
        "UPDATE messages SET state = ? WHERE event_id = ?",
        rusqlite::params![string, event_id],
    )?;
    Ok(())
}
