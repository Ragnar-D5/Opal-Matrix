use serde::{Deserialize, Serialize};

pub mod errors;

#[derive(Serialize, Deserialize)]
pub enum RestoreResponse {
    NoSession,
    Success { user_id: String },
    Failed { home_server: String },
}
