pub mod breadcrumbs;
pub mod server_order;

pub trait AccountData {
    const DATA_KEY: &'static str;
}

pub use breadcrumbs::Breadcrumbs;
pub use server_order::ServerOrder;
