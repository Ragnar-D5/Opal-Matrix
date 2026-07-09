use serde::{Deserialize, Serialize};
use shared::{
    account_data::{Breadcrumbs, ServerOrder},
    api::events::{RecentEmoji, RecentEmojies},
};
use tauri::{command, AppHandle};

use crate::{send_event, MatrixClientState, TauriError};

use matrix_sdk::{
    event_handler::Ctx,
    ruma::{
        events::{macros::EventContent, GlobalAccountDataEvent},
        UInt,
    },
};

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

#[derive(Clone, Debug, Serialize, Deserialize, EventContent)]
#[ruma_event(type = "io.element.recent_emoji", kind = GlobalAccountData)]
pub struct ElementRecentEmojiEventContent {
    pub recent_emoji: Vec<(String, UInt)>,
}

pub async fn on_recent_emoji_update(
    update: GlobalAccountDataEvent<ElementRecentEmojiEventContent>,
    handle: Ctx<AppHandle>,
) {
    let all_by_recency: Vec<RecentEmoji> = update
        .content
        .recent_emoji
        .iter()
        .map(|(e, t)| RecentEmoji {
            emoji: e.clone(),
            total: (*t).into(),
        })
        .collect();

    let mut top = all_by_recency.clone();

    top.sort_by_key(|e| std::cmp::Reverse(e.total));

    log::debug!("Sending recent emoji update");
    send_event(
        &handle,
        &RecentEmojies {
            top: top.into_iter().take(5).collect(),
            all_by_recency,
        },
    );
}
