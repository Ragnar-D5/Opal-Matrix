use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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
    RoomCreation,
    MembershipChange(MembershipAction),
    RoomNameChange { new_name: String },
    TopicChange { new_topic: String },
    EncryptionEnabled { algorithm: String },
    PowerlevelChange,
    JoinRuleChange { new_rule: String },
    HistoryVisibilityChange { new_visibility: String },
    GuestAccessChange { new_access: String },

    CallJoined { intent: String },
    CallLeft,

    MessageEdited { event_id: String, new_text: String },
    MessageReacted { event_id: String, reaction: String },
    MessageRedacted { event_id: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EncryptedFileInfo {
    pub key: String,
    pub iv: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MessageContent {
    Text {
        text: String,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Mentions {
    pub room: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Reaction {
    pub sender_id: String,
    pub reaction: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UserMessage {
    pub mentions: Option<Mentions>,
    pub reactions: Vec<Reaction>,

    pub content: MessageContent,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MessageKind {
    UserMessage(UserMessage),
    SystemMessage(SystemMessage),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UiMessage {
    pub event_id: String,
    pub timestamp: i64,
    pub sender_id: String,

    pub kind: MessageKind,
}

impl UiMessage {
    pub fn delete(&mut self) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.content = MessageContent::Deleted;
            user_message.mentions = None;
            user_message.reactions.clear();
        }
    }

    pub fn edit(&mut self, new_text: String) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.content = MessageContent::Text {
                text: new_text,
                is_edited: true,
            };
        }
    }

    pub fn add_reaction(&mut self, reaction: Reaction) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.reactions.push(reaction);
        }
    }
}
