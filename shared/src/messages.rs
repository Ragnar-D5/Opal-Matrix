use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use ruma::events::room::{guest_access::GuestAccess, history_visibility::HistoryVisibility};
use ruma::events::Mentions;
use ruma::room::JoinRule;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub enum MembershipAction {
    Joined,
    Left,
    Invited(String),
    Kicked {
        target_id: String,
        reason: Option<String>,
    },
    Banned {
        target_id: String,
        reason: Option<String>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum SystemMessage {
    RoomCreated {
        creator_id: String,
    },
    MembershipChange(MembershipAction),
    RoomNameChange {
        new_name: String,
    },
    RoomAvatarChange {
        new_avatar_url: Option<String>,
    },
    TopicChange {
        new_topic: String,
    },
    EncryptionEnabled {
        algorithm: String,
    },
    PowerlevelChange,
    JoinRuleChange {
        new_rule: JoinRule,
    },
    HistoryVisibilityChange {
        new_visibility: HistoryVisibility,
    },
    GuestAccessChange {
        new_access: GuestAccess,
    },
    CallJoined {
        intent: String,
    },
    CallLeft,
    MessageEdited {
        event_id: String,
        new_spans: Vec<RichTextSpan>,
    },
    MessageReacted {
        event_id: String,
        reaction: String,
    },
    MessageRedacted {
        event_id: String,
        reason: Option<String>,
    },
    VerificationRequest {
        from_user_id: String,
        methods: Vec<String>,
    },
    Unknown,
    /// Used to delete messages only in the ui (for example pending messages)
    RemoveMessage {
        event_id: String,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct EncryptedFileInfo {
    pub key: String,
    pub iv: String,
    pub hash: String,
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
pub enum MessageContent {
    Text {
        spans: Vec<RichTextSpan>,
        is_edited: bool,
    },
    Image {
        name: String,
        url: String,
        width: Option<u32>,
        height: Option<u32>,

        encryption_info: Option<EncryptedFileInfo>,
    },
    File {
        url: String,
        filename: String,
        size: u64,
    },
    Encrypted,
    Deleted,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct RepliesTo {
    pub text: Option<Vec<RichTextSpan>>,
    pub sender_id: Option<String>,
    pub event_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserMessage {
    pub mentions: Mentions,
    pub reactions: HashMap<String, HashSet<String>>,
    pub replies_to: Option<RepliesTo>,

    pub content: MessageContent,
}

impl Hash for UserMessage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.mentions.room.hash(state);
        self.mentions.user_ids.hash(state);
        for (key, value) in &self.reactions {
            key.hash(state);
            for v in value {
                v.hash(state);
            }
        }
        self.replies_to.hash(state);
        self.content.hash(state);
    }
}

impl PartialEq for UserMessage {
    fn eq(&self, other: &Self) -> bool {
        self.mentions.room == other.mentions.room
            && self.mentions.user_ids == other.mentions.user_ids
            && self.reactions == other.reactions
            && self.replies_to == other.replies_to
            && self.content == other.content
    }
}

impl UserMessage {
    pub fn mentions_user(&self, user_id: &String) -> bool {
        self.mentions
            .user_ids
            .iter()
            .map(|v| v.to_string())
            .any(|v| v == *user_id)
    }

    pub fn new() -> Self {
        Self {
            mentions: Mentions::default(),
            reactions: HashMap::new(),
            replies_to: None,
            content: MessageContent::Deleted,
        }
    }

    pub fn set_content(&mut self, content: MessageContent) {
        self.content = content;
    }

    pub fn set_replies_to(&mut self, event_id: String) {
        self.replies_to = Some(RepliesTo {
            text: None,
            sender_id: None,
            event_id,
        });
    }

    pub fn set_reply_text(&mut self, spans: Vec<RichTextSpan>) {
        if let Some(replies_to) = &mut self.replies_to {
            replies_to.text = Some(spans);
        }
    }

    pub fn set_reply_sender(&mut self, sender_id: String) {
        if let Some(replies_to) = &mut self.replies_to {
            replies_to.sender_id = Some(sender_id);
        }
    }

    pub fn deleted() -> Self {
        Self {
            mentions: Mentions::default(),
            reactions: HashMap::new(),
            replies_to: None,
            content: MessageContent::Deleted,
        }
    }

    pub fn display_string(&self) -> String {
        match &self.content {
            MessageContent::Text { spans, .. } => spans
                .iter()
                .filter_map(|span| match span {
                    RichTextSpan::Plain(text) => Some(text.clone()),
                    RichTextSpan::Link { url, text } => {
                        Some(text.clone().unwrap_or_else(|| url.clone()))
                    }
                    RichTextSpan::RoomMention => Some("@room".to_string()),
                    RichTextSpan::UserMention { display_name, .. } => {
                        Some(format!("@{}", display_name.clone()))
                    }
                    RichTextSpan::Newline => Some("\n".to_string()),
                })
                .collect(),
            MessageContent::File { filename, size, .. } => {
                format!("File: {} ({} bytes)", filename, size)
            }
            MessageContent::Image {
                name,
                width,
                height,
                ..
            } => {
                let dimensions = match (width, height) {
                    (Some(w), Some(h)) => format!("{}x{}", w, h),
                    _ => "unknown dimensions".to_string(),
                };
                format!("Image: {} ({})", name, dimensions)
            }
            MessageContent::Encrypted => "Encrypted message".to_string(),
            MessageContent::Deleted => "Message deleted".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MessageKind {
    UserMessage(UserMessage),
    SystemMessage(SystemMessage),
}

impl Hash for MessageKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            MessageKind::UserMessage(user_message) => {
                user_message.hash(state);
            }
            MessageKind::SystemMessage(_) => {
                "systemmessage".hash(state);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default, Hash)]
pub enum MessageState {
    #[default]
    Pending,
    Sent,
    Failed,
}

impl ToString for MessageState {
    fn to_string(&self) -> String {
        match self {
            MessageState::Failed => "failed".to_string(),
            MessageState::Pending => "pending".to_string(),
            MessageState::Sent => "sent".to_string(),
        }
    }
}

impl From<String> for MessageState {
    fn from(s: String) -> Self {
        match s.as_str() {
            "pending" => MessageState::Pending,
            "sent" => MessageState::Sent,
            "failed" => MessageState::Failed,
            _ => MessageState::Pending, // Default to pending for unknown states
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct UiMessage {
    pub event_id: String,
    pub timestamp: u64,
    pub sender_id: String,

    pub state: MessageState,
    pub kind: MessageKind,
}

impl UiMessage {
    pub fn delete(&mut self) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.content = MessageContent::Deleted;
            user_message.mentions = Mentions::default();
            user_message.reactions.clear();
        }
    }

    pub fn edit(&mut self, new_spans: Vec<RichTextSpan>) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.content = MessageContent::Text {
                spans: new_spans,
                is_edited: true,
            };
        }
    }

    pub fn add_reactions(&mut self, reactions: &HashMap<String, HashSet<String>>) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.reactions = reactions.clone()
        }
    }

    pub fn is_user_message(&self) -> bool {
        matches!(self.kind, MessageKind::UserMessage(_))
    }

    pub fn deleted(event_id: String, timestamp: u64, sender_id: String) -> Self {
        Self {
            event_id,
            timestamp,
            sender_id,
            state: MessageState::default(),
            kind: MessageKind::UserMessage(UserMessage::deleted()),
        }
    }
}
