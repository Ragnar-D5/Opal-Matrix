use serde::{Deserialize, Serialize};

pub mod breadcrumbs;
pub mod server_order;

pub trait AccountData {
    const DATA_KEY: &'static str;
}

pub use breadcrumbs::Breadcrumbs;
pub use server_order::ServerOrder;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AccountDataPayload {
    Breadcrumbs(Breadcrumbs),
    ServerOrder(ServerOrder),
}

#[derive(Serialize)]
pub struct AccountDataArgs {
    pub payload: AccountDataPayload,
}

#[derive(Serialize, Deserialize)]
pub enum AccountDataType {
    Breadcrumbs,
    ServerOrder,
}

#[derive(Serialize)]
pub struct GetAccountDataArgs {
    pub data_type: AccountDataType,
}
