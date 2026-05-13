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
    RoomNameChange {
        new_name: String,
    },
    TopicChange {
        new_topic: String,
    },
    EncryptionEnabled {
        algorithm: String,
    },
    PowerlevelChange,
    JoinRuleChange {
        new_rule: String,
    },
    HistoryVisibilityChange {
        new_visibility: String,
    },
    GuestAccessChange {
        new_access: String,
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
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EncryptedFileInfo {
    pub key: String,
    pub iv: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Mentions {
    #[serde(default)]
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
pub struct RepliesTo {
    pub text: Option<Vec<RichTextSpan>>,
    pub sender_id: Option<String>,
    pub event_id: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UserMessage {
    pub mentions: Mentions,
    pub reactions: Vec<Reaction>,
    pub replies_to: Option<RepliesTo>,

    pub content: MessageContent,
}

impl UserMessage {
    pub fn new() -> Self {
        Self {
            mentions: Mentions::default(),
            reactions: Vec::new(),
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

    pub fn set_mentions(&mut self, mentions: Mentions) {
        self.mentions = mentions;
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
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MessageKind {
    UserMessage(UserMessage),
    SystemMessage(SystemMessage),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UiMessage {
    pub event_id: String,
    pub timestamp: u64,
    pub sender_id: String,

    pub is_pending: bool,
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

    pub fn add_reaction(&mut self, reaction: Reaction) {
        if let MessageKind::UserMessage(user_message) = &mut self.kind {
            user_message.reactions.push(reaction);
        }
    }
}
