use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum LoginError {
    BackendError,
    InvalidCredentials,
    NetworkError,
}
