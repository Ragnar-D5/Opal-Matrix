use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::messages::UiMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagesUpdate {
    pub new_messages: HashMap<String, Vec<UiMessage>>,
    pub updated_messages: HashMap<String, Vec<UiMessage>>,
    pub messages_to_remove: HashSet<String>,
}
