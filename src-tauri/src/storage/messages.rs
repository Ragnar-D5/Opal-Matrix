use log::debug;
use rusqlite::Connection;
use serde_json::Value;
use shared::{MessageContent, MessageKind, SystemMessage, UiMessage, UserMessage};

use crate::TauriError;

use super::DataBaseModel;

#[derive(Debug, Clone)]
pub struct MessageRow {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub msg_type: String,
    pub raw_json: String,
    pub timestamp: i64,
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
                "SELECT event_id, room_id, sender, msg_type, raw_json, timestamp
            FROM MESSAGES
            WHERE room_id = ?
                AND timestamp < (SELECT timestamp FROM MESSAGES WHERE event_id = ?)
                AND event_id != ?
            ORDER BY timestamp DESC
            LIMIT ?",
            )?;

            let rows = stmt.query_map(rusqlite::params![room_id, id, id, limit], |row| {
                Ok(MessageRow {
                    event_id: row.get(0)?,
                    room_id: row.get(1)?,
                    sender: row.get(2)?,
                    msg_type: row.get(3)?,
                    raw_json: row.get(4)?,
                    timestamp: row.get(5)?,
                })
            })?;

            for msg_res in rows {
                messages.push(msg_res?.into());
            }
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT event_id, room_id, sender, msg_type, raw_json, timestamp
            FROM MESSAGES
            WHERE room_id = ?
            ORDER BY timestamp DESC
            LIMIT ?",
            )?;

            let rows = stmt.query_map(rusqlite::params![room_id, limit], |row| {
                Ok(MessageRow {
                    event_id: row.get(0)?,
                    room_id: row.get(1)?,
                    sender: row.get(2)?,
                    msg_type: row.get(3)?,
                    raw_json: row.get(4)?,
                    timestamp: row.get(5)?,
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
            "INSERT OR IGNORE INTO messages (event_id, room_id, sender, msg_type, raw_json, timestamp)
            VALUES (?, ?, ?, ?, ?, ?)",
        )?;

        for msg in messages {
            stmt.execute(rusqlite::params![
                msg.event_id,
                msg.room_id,
                msg.sender,
                msg.msg_type,
                msg.raw_json,
                msg.timestamp
            ])?;
        }
    }

    tx.commit()?;
    Ok(())
}

impl TryInto<UiMessage> for MessageRow {
    type Error = TauriError;

    fn try_into(self) -> Result<UiMessage, Self::Error> {
        let value: Value = serde_json::from_str(&self.raw_json)?;
        let content = value.get("content").ok_or("Missing content")?;

        let message_kind = match self.msg_type.as_str() {
            "m.room.message" => {
                let msg_type = content
                    .get("msgtype")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing msgtype")?;
                let mentions = content
                    .get("m.mentions")
                    .and_then(|v| serde_json::from_value(v.clone()).ok());

                match msg_type {
                    "m.text" => MessageKind::UserMessage(UserMessage {
                        mentions: mentions,
                        content: MessageContent::Text {
                            text: content
                                .get("body")
                                .and_then(|v| v.as_str())
                                .ok_or("Missing body")?
                                .to_string(),
                            is_edited: false,
                        },
                    }),
                    "m.image" => {
                        let file = content.get("file").ok_or("Missing file")?;
                        let info = content.get("info").ok_or("Missing info")?;

                        MessageKind::UserMessage(UserMessage {
                            mentions: mentions,
                            content: MessageContent::Image {
                                name: content
                                    .get("body")
                                    .and_then(|v| v.as_str())
                                    .ok_or("Missing body")?
                                    .to_string(),
                                url: file
                                    .get("url")
                                    .and_then(|v| v.as_str())
                                    .ok_or("Missing url")?
                                    .to_string(),
                                width: info.get("w").and_then(|v| v.as_u64()).map(|n| n as u32),
                                height: info.get("h").and_then(|v| v.as_u64()).map(|n| n as u32),
                            },
                        })
                    }
                    _ => {
                        debug!("Unsupported msgtype: {}", msg_type);
                        MessageKind::UserMessage(UserMessage {
                            mentions: mentions,
                            content: MessageContent::Text {
                                text: content
                                    .get("body")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("[Unsupported message type]")
                                    .to_string(),
                                is_edited: false,
                            },
                        })
                    }
                }
            }
            "m.room.member" => {
                let membership = content
                    .get("membership")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing membership")?;
                let state_key = value
                    .get("state_key")
                    .and_then(|v| v.as_str())
                    .ok_or(format!("Missing state key: {:?}", value))?
                    .to_string();

                MessageKind::SystemMessage(shared::SystemMessage::MembershipChange(
                    match membership {
                        "join" => shared::MembershipAction::Joined,
                        "invite" => shared::MembershipAction::Invited(state_key),
                        "leave" => {
                            if state_key == self.sender {
                                shared::MembershipAction::Left
                            } else {
                                shared::MembershipAction::Kicked {
                                    target_id: state_key,
                                    reason: content
                                        .get("reason")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s.to_string()),
                                }
                            }
                        }
                        "ban" => shared::MembershipAction::Banned {
                            target_id: state_key,
                            reason: content
                                .get("reason")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                        },
                        _ => {
                            return Err(format!("Unknown membership: {membership}").into());
                        }
                    },
                ))
            }
            "m.room.create" => MessageKind::SystemMessage(SystemMessage::RoomCreation),
            "m.room.name" => MessageKind::SystemMessage(SystemMessage::RoomNameChange {
                new_name: content
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing name")?
                    .to_string(),
            }),
            "m.room.topic" => MessageKind::SystemMessage(SystemMessage::TopicChange {
                new_topic: content
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing topic")?
                    .to_string(),
            }),
            _ => return Err(format!("Unsupported message type: {}", self.msg_type).into()),
        };

        let msg = UiMessage {
            event_id: self.event_id,
            timestamp: self.timestamp,
            sender_id: self.sender,
            kind: message_kind,
        };

        debug!("{:?}", msg);

        return Ok(msg);
    }
}
