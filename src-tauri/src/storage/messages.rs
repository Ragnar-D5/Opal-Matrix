use std::collections::{HashMap, HashSet};

use ego_tree::NodeRef;
use linkify::LinkFinder;
use ruma::events::room::member::MembershipState;
use ruma::events::Mentions;
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
    EncryptedFileInfo, MembershipAction, MessageContent, MessageKind, MessageState, Reactor,
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
    pub last_edited_id: Option<String>,
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
                    last_edited_id TEXT,
                    FOREIGN KEY (last_edited_id) REFERENCES message_edits(event_id),
                    FOREIGN KEY (room_id) REFERENCES rooms(room_id)
                );
            ",
        )?;
        Ok(())
    }
}

pub fn get_messsages_to_id(
    conn: &Connection,
    room_id: &String,
    oldest_id: Option<String>,
    limit: usize,
) -> Result<Vec<MessageRow>, TauriError> {
    let mut messages = Vec::new();

    match oldest_id {
        Some(id) => {
            let mut stmt = conn.prepare(
                "SELECT
                m.event_id,
                m.room_id,
                m.sender,
                m.msg_type,
                COALESCE(e.new_json, m.raw_json) AS raw_json,
                m.timestamp,
                m.state,
                m.last_edited_id
                FROM messages m
                LEFT JOIN message_edits e ON m.last_edited_id = e.event_id
                WHERE m.room_id = ?1
                AND (
                    m.timestamp < (SELECT timestamp FROM MESSAGES WHERE event_id = ?2)
                    OR (
                        m.timestamp = (SELECT timestamp FROM MESSAGES WHERE event_id = ?2)
                        AND m.event_id < ?2
                    )
                )
                ORDER BY m.timestamp DESC
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
                    last_edited_id: row.get(7)?,
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT
                m.event_id,
                m.room_id,
                m.sender,
                m.msg_type,
                COALESCE(e.new_json, m.raw_json) AS raw_json,
                m.timestamp,
                m.state,
                m.last_edited_id
                FROM messages m
                LEFT JOIN message_edits e ON m.last_edited_id = e.event_id
                WHERE m.room_id = ?1
                ORDER BY m.timestamp DESC
                LIMIT ?2",
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
                    last_edited_id: row.get(7)?,
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?);
            }
        }
    }

    Ok(messages)
}

pub fn get_messages_by_id_with_reactions(
    conn: &Connection,
    event_ids: Vec<String>,
) -> Result<Vec<(MessageRow, HashMap<String, HashSet<Reactor>>)>, TauriError> {
    let mut stmt = conn.prepare(
        "SELECT
            m.event_id AS event_id,
            m.room_id,
            m.sender,
            m.msg_type,
            COALESCE(e.new_json, m.raw_json) AS raw_json,
            m.timestamp,
            m.state,
            m.last_edited_id,
            r.sender_id AS reaction_user_id,
            r.event_id AS reaction_event_id,
            r.reaction
        FROM messages m
        LEFT JOIN message_edits e ON m.last_edited_id = e.event_id
        LEFT JOIN reactions r ON m.event_id = r.target_event_id
        WHERE m.event_id IN (SELECT value FROM json_each(?))",
    )?;

    let mut message_map: HashMap<String, (MessageRow, HashMap<String, HashSet<Reactor>>)> =
        HashMap::new();

    let json_array = serde_json::to_string(&event_ids)?;
    let rows = stmt.query_map(rusqlite::params![json_array], |row| {
        let state: String = row.get(6)?;
        let message_row = MessageRow {
            event_id: row.get(0)?,
            room_id: row.get(1)?,
            sender: row.get(2)?,
            msg_type: row.get(3)?,
            raw_json: row.get(4)?,
            timestamp: row.get(5)?,
            state: state.into(),
            last_edited_id: row.get(7)?,
        };

        let user_id: Option<String> = row.get(8)?;
        let reaction_event_id: Option<String> = row.get(9)?;
        let reaction: Option<String> = row.get(10)?;

        Ok((message_row, user_id, reaction_event_id, reaction))
    })?;

    for row_res in rows {
        let (msg, user_id, reaction_event_id, reaction_key) = row_res?;

        let entry = message_map
            .entry(msg.event_id.clone())
            .or_insert((msg, HashMap::new()));

        if let (Some(u_id), Some(r_id), Some(r_key)) = (user_id, reaction_event_id, reaction_key) {
            entry
                .1
                .entry(r_key)
                .or_insert_with(HashSet::new)
                .insert(Reactor {
                    user_id: u_id,
                    event_id: r_id,
                });
        }
    }

    Ok(message_map.into_values().collect())
}

pub fn save_messages(conn: &mut Connection, messages: Vec<MessageRow>) -> Result<(), TauriError> {
    let tx = conn.transaction()?;

    {
        let mut stmt = tx.prepare(
            "INSERT INTO messages (event_id, room_id, sender, msg_type, raw_json, timestamp, state, last_edited_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(event_id) DO UPDATE SET
                room_id = excluded.room_id,
                sender = excluded.sender,
                msg_type = excluded.msg_type,
                raw_json = excluded.raw_json,
                timestamp = excluded.timestamp,
                state = excluded.state,
                last_edited_id = excluded.last_edited_id
            WHERE messages.state = 'unknown'
               OR messages.raw_json = '{}'",
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
                msg.last_edited_id,
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

pub fn any_exists(conn: &Connection, event_ids: Vec<String>) -> Result<bool, TauriError> {
    let mut stmt = conn.prepare(
        "SELECT 1 FROM messages WHERE event_id IN (SELECT value FROM json_each(?)) LIMIT 1",
    )?;
    let json_array = serde_json::to_string(&event_ids)?;
    let exists = stmt.exists(rusqlite::params![json_array])?;

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

fn object_is_empty(obj: &Value) -> bool {
    match obj {
        Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

impl MessageRow {
    pub fn try_into_ui_message(self) -> Result<UiMessage, TauriError> {
        let value: Value = serde_json::from_str(&self.raw_json)?;

        if self.state == MessageState::Deleted {
            if self.msg_type == "m.room.message" && !object_is_empty(&value) {
                return Ok(UiMessage::deleted(
                    self.event_id,
                    self.room_id,
                    self.timestamp,
                    self.sender,
                ));
            } else {
                return Err(TauriError::silent());
            }
        } else if self.state == MessageState::Unknown {
            return Err(TauriError::silent());
        }

        let raw_content = value
            .get("content")
            .ok_or(format!("Missing content: {:?}", self))?;

        let state = self.state;

        let mut msg = UiMessage {
            event_id: self.event_id,
            room_id: self.room_id,
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
                        let mut user_message = UserMessage::new();

                        let content = if let Some(Relation::Replacement(replacement)) =
                            &or.content.relates_to
                        {
                            replacement.new_content.clone().into()
                        } else {
                            or.content.clone()
                        };

                        if let Some(Relation::Reply(reply)) = &or.content.relates_to {
                            user_message.set_replies_to(reply.in_reply_to.event_id.to_string());
                        };

                        // user_message.mentions = content.mentions.unwrap_or_default();

                        user_message.content = match content.msgtype {
                            MessageType::Text(text_content) => {
                                let body = text_content.body;
                                let spans = if let Some(formatted) = text_content.formatted {
                                    match formatted.format {
                                        MessageFormat::Html => {
                                            parse_html_to_spans(&formatted.body, &body)
                                        }
                                        _ => parse_plain_text_to_spans(&body),
                                    }
                                } else {
                                    parse_plain_text_to_spans(&body)
                                };

                                MessageContent::Text { spans }
                            }
                            MessageType::Image(image_content) => {
                                // TODO: Use the actual content instead of raw json for this
                                let encryption_info =
                                    if let Some(file_obj) = raw_content.get("file") {
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
                        return Err(TauriError::silent());
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
                            return Err(TauriError::silent());
                        }
                    } else {
                        return Err(TauriError::silent());
                    }
                }
                // AnySyncMessageLikeEvent::RoomEncrypted(_) => {
                //     MessageKind::UserMessage(UserMessage {
                //         mentions: Mentions::default(),
                //         reactions: HashMap::new(),
                //         replies_to: None,
                //         is_edited: false,
                //         content: MessageContent::Encrypted,
                //     })
                // }
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
                | AnySyncMessageLikeEvent::RtcDecline(_)
                | AnySyncMessageLikeEvent::CallSdpStreamMetadataChanged(_) => {
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
                            if object_is_empty(raw_content) {
                                SystemMessage::CallLeft
                            } else {
                                let intent = raw_content
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
                        // AnyStateEventContent::RoomJoinRules(join_ev) => {
                        //     SystemMessage::JoinRuleChange {
                        //         new_rule: join_ev.join_rule,
                        //     }
                        // }
                        // AnyStateEventContent::RoomHistoryVisibility(vis_ev) => {
                        //     SystemMessage::HistoryVisibilityChange {
                        //         new_visibility: vis_ev.history_visibility,
                        //     }
                        // }
                        // AnyStateEventContent::RoomGuestAccess(guest_ev) => {
                        //     SystemMessage::GuestAccessChange {
                        //         new_access: guest_ev.guest_access,
                        //     }
                        // }
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
                    return Err(TauriError::silent());
                }
            }
        };

        msg.kind = message_kind;
        msg.set_edited(self.last_edited_id.is_some());

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

pub fn redact_messages(conn: &mut Connection, event_ids: Vec<String>) -> Result<(), TauriError> {
    let tx = conn.transaction()?;

    {
        let mut stmt = tx
            .prepare("UPDATE messages SET raw_json = '{}', state = 'deleted' WHERE event_id = ?")?;

        for event_id in event_ids {
            stmt.execute(rusqlite::params![event_id])?;
        }
    }

    tx.commit()?;
    Ok(())
}
