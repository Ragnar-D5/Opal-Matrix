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
    },
    File {
        url: String,
        filename: String,
        size: u64,
    },
    Encrypted,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Mentions {
    pub room: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_ids: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UserMessage {
    pub mentions: Option<Mentions>,

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
