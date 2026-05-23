use serde::{Deserialize, Serialize};
use shared::account_data::{Breadcrumbs, ServerOrder};
use tauri::command;

use crate::{MatrixClientState, TauriError};

use ruma::events::macros::EventContent;

#[derive(Clone, Debug, Serialize, Deserialize, EventContent)]
#[ruma_event(type = "org.opal-matrix.breadcrumbs", kind = GlobalAccountData)]
pub struct BreadcrumbsEventContent(pub Breadcrumbs);

#[derive(Clone, Debug, Serialize, Deserialize, EventContent)]
#[ruma_event(type = "org.opal-matrix.server_order", kind = GlobalAccountData)]
pub struct ServerOrderEventContent(pub ServerOrder);

#[command]
pub async fn get_breadcrumbs(client: MatrixClientState<'_>) -> Result<Breadcrumbs, TauriError> {
    log::debug!("Getting breadcrumbs");
    let client = client.read().await;

    let res = client
        .account()
        .account_data::<BreadcrumbsEventContent>()
        .await?;

    let breadcumbs: Breadcrumbs = if let Some(event) = res {
        event.deserialize()?.0
    } else {
        Breadcrumbs::default()
    };

    Ok(breadcumbs)
}

#[command]
pub async fn get_server_order(client: MatrixClientState<'_>) -> Result<ServerOrder, TauriError> {
    log::debug!("Getting server order");
    let client = client.read().await;

    let res = client
        .account()
        .account_data::<ServerOrderEventContent>()
        .await?;

    let server_order: ServerOrder = if let Some(event) = res {
        event.deserialize()?.0
    } else {
        ServerOrder::default()
    };

    Ok(server_order)
}

#[command]
pub async fn set_breadcrumbs(
    client: MatrixClientState<'_>,
    breadcrumbs: Breadcrumbs,
) -> Result<(), TauriError> {
    log::debug!("Setting breadcrumbs");
    let client = client.read().await;

    let content = BreadcrumbsEventContent(breadcrumbs);

    client.account().set_account_data(content).await?;

    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn set_server_order(
    client: MatrixClientState<'_>,
    server_order: ServerOrder,
) -> Result<(), TauriError> {
    log::debug!("Setting server order");
    let client = client.read().await;

    let content = ServerOrderEventContent(server_order);

    client.account().set_account_data(content).await?;

    Ok(())
}
