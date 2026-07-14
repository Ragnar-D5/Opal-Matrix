use std::str::FromStr;

use matrix_sdk::{
    Client, RoomState,
    ruma::{OwnedRoomId, api::client::room::get_summary},
};
use shared::{
    sidebar::{RoomExtraInfo, UiMembership},
    timeline::UiHistoryVisibility,
};
use tauri::{State, command};
use tokio::sync::RwLock;

use crate::{
    TauriError,
    frontend::timeline::{history_visibility_to_ui, join_rule_to_ui},
};

#[command(rename_all = "snake_case")]
pub async fn get_extra_room_info(
    client: State<'_, RwLock<Client>>,
    room_id: String,
) -> Result<RoomExtraInfo, TauriError> {
    let room_id = OwnedRoomId::from_str(&room_id)?;
    let client = client.read().await.clone();

    if let Some(room) = client.get_room(&room_id) {
        let info = room.clone_info();

        let membership = match room.state() {
            RoomState::Banned => UiMembership::Ban,
            RoomState::Invited => UiMembership::Invite,
            RoomState::Joined => UiMembership::Join,
            RoomState::Knocked => UiMembership::Knock,
            RoomState::Left => UiMembership::Leave,
        };

        Ok(RoomExtraInfo {
            membership,
            join_rule: info
                .join_rule()
                .cloned()
                .map(|r| join_rule_to_ui(r.into()))
                .unwrap_or_default(),
            history_visibility: info
                .history_visibility()
                .map(history_visibility_to_ui)
                .unwrap_or_default(),
            encrypted: info.encryption_state().is_encrypted(),
            version: info.room_version().map(|v| v.to_string()),
            num_joined_users: info.active_members_count(),
        })
    } else {
        let request = get_summary::v1::Request::new(room_id.into(), vec![]);
        let response = client.send(request).await?;

        let summary = response.summary;

        Ok(RoomExtraInfo {
            membership: UiMembership::Leave,
            join_rule: join_rule_to_ui(summary.join_rule),
            history_visibility: if summary.world_readable {
                UiHistoryVisibility::WorldReadable
            } else {
                UiHistoryVisibility::default()
            },
            encrypted: summary.encryption.is_some(),
            version: summary.room_version.map(|v| v.to_string()),
            num_joined_users: summary.num_joined_members.into(),
        })
    }
}
