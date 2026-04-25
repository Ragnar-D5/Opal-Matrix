use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone, Default, Deserialize)]
pub struct ServerOrder {
    #[serde(default)]
    pub servers: Vec<String>,
}

impl super::AccountData for ServerOrder {
    const DATA_KEY: &'static str = "org.opal-matrix.server_order";
}
