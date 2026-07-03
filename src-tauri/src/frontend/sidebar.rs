use matrix_sdk::deserialized_responses::{SyncOrStrippedState};
use matrix_sdk::ruma::events::direct::DirectEventContent;
use matrix_sdk::ruma::events::space::child::{SpaceChildEventContent};
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::OwnedRoomId;
use shared::api::events::CallMemberUpdate;
use shared::get_color;
use std::collections::{HashMap, HashSet};

use futures::{StreamExt, TryFutureExt, };
use matrix_sdk::{Client, Room, RoomMemberships, RoomState };

use matrix_sdk::room::ParentSpace;
use matrix_sdk::ruma::events::call::member::CallMemberEventContent;
use matrix_sdk::ruma::events::{AnyGlobalAccountDataEvent, AnySyncStateEvent, AnySyncTimelineEvent, OriginalSyncStateEvent};
use matrix_sdk::sync::RoomUpdates;
use shared::sidebar::{DmRoomNode, NotificationCounts, RoomMapUpdate, RoomNode, RoomNodeInfo, ServerList, ServerRoomNode, SingleRoomNode, SpaceRoomNode, TextChannelRoomNode, UserDevice, VoiceChannelRoomNode};
use tauri::AppHandle;

use crate::{TauriError, send_event};

async fn get_child_room_ids(room: &Room) -> Result<Vec<OwnedRoomId>, TauriError> {
    let child_events = room
        .get_state_events_static::<SpaceChildEventContent>()
        .await?;

    let child_room_ids = child_events
        .into_iter()
        .filter_map(|raw| {
            let ev = raw.deserialize().ok()?;
            let child = ev.as_sync()?.as_original()?;
            if child.content.via.is_empty() {
                return None;
            }
            Some(child.state_key.clone())
        })
        .collect();

    Ok(child_room_ids)
}

async fn get_all_child_room_ids(room: &Room) -> Result<Vec<OwnedRoomId>, TauriError> {
    let mut all_children = Vec::new();
    let mut queue = vec![room.room_id().to_owned()];

    while let Some(current_room_id) = queue.pop() {
        if let Some(current_room) = room.client().get_room(&current_room_id) {
            let child_ids = get_child_room_ids(&current_room).await?;
            for child_id in child_ids {
                if !all_children.contains(&child_id) {
                    all_children.push(child_id.clone());
                    queue.push(child_id);
                }
            }
        }
    }

    Ok(all_children)
}

pub async fn convert_room_to_node(room: &Room) -> Option<RoomNode> {
    let info = room.clone_info();

    let name = room.display_name().await.ok()?.to_string();
    let has_avatar = info.avatar_url().is_some();
    let canonical_alias = info.canonical_alias().map(|a| a.to_string());
    let aliases = info.alt_aliases().iter().map(|a| a.to_string()).collect();
    let room_id = room.room_id().to_string();
    let topic = info.topic().map(|t| t.to_string());
    let color = get_color(&room_id);

    let info = RoomNodeInfo {
        name,
        has_avatar,
        canonical_alias,
        aliases,
        room_id: room_id.clone(),
        topic,
        color,
    };

    if room.compute_is_dm().map_err(|e| log::error!("Failed to compite if room is dm for room {}: {e}", &room_id)).await.unwrap_or(false) {
        let other_user_id = room.direct_targets().into_iter().find_map(|id| id.into_user_id());

        let other_user_id = match other_user_id {
            Some(id) => Some(id),
            None => room.joined_user_ids().await.map_err(|e| log::error!("Failed to get joined user ids for room {}: {e}", &room_id)).ok().and_then(|ids| ids.into_iter().find(|id| id != room.own_user_id())),
        };

        if let Some(other_user_id) = other_user_id {
            return Some(RoomNode::Dm(DmRoomNode { info, other_user_id: other_user_id.to_string() }));
        }

        log::warn!("Room {} looked like a DM but no DM node could be built; falling back to a normal room", &room_id);
    }

    if room.is_space() {
        let children = get_child_room_ids(room).await.unwrap_or_default().iter().map(|id| id.to_string()).collect();

        let is_top_level = match room.parent_spaces().await {
            Ok(stream) => {
                stream.filter_map(|res| futures::future::ready(res.ok()))
                            .filter(|p| futures::future::ready(matches!(p, ParentSpace::Reciprocal(_))))
                            .next()
                            .await
                            .is_none()
            }
            Err(_) => true,
        };

        return if is_top_level {
            let all_children = get_all_child_room_ids(room).await.unwrap_or_default().iter().map(|id| id.to_string()).collect();

            Some(RoomNode::Server(ServerRoomNode {
                info,
                children,
                all_children,
            }))
        } else {
            Some(RoomNode::Space(SpaceRoomNode {
                info,
                children,
            }))
        }
    }

    if room.is_call() {
        return Some(RoomNode::VoiceChannel(VoiceChannelRoomNode {
            info
        }));
    }

    // Check if room has parents
    if let Ok(stream) = room.parent_spaces().await {
        let has_reciprocal_parent = stream
            .collect::<Vec<_>>()
            .await
            .iter()
            .any(|res| matches!(res, Ok(ParentSpace::Reciprocal(_))));

        if has_reciprocal_parent {
            return Some(RoomNode::TextChannel(TextChannelRoomNode {
                info,
            }));
        } else {
            let other_user_ids = room.members(RoomMemberships::ACTIVE).await.ok()?.iter().filter_map(|m| if m.user_id() != room.own_user_id() {
                Some(m.user_id().to_string())
            } else {
                None
            }).collect();

            return Some(RoomNode::Single(SingleRoomNode {
                info,
                other_user_ids,
            }))
        }
    }

    Some(RoomNode::TextChannel(TextChannelRoomNode {
        info,
    }))
}

fn should_notification_update(room_updates: &RoomUpdates) -> bool {
    for update in room_updates.joined.values() {
        if !update.timeline.events.is_empty() {
            return true;
        }
        for raw_event in &update.ephemeral {
            if let Ok(Some(event_type)) = raw_event.get_field::<String>("type")
                && event_type == "m.receipt"
            {
                return true;
            }
        }
        for raw_event in &update.account_data {
            if let Ok(Some(event_type)) = raw_event.get_field::<String>("type")
                && event_type == "m.fully_read"
            {
                return true;
            }
        }
    }
    false
}

pub async fn handle_room_updates(
    room_updates: &RoomUpdates,
    client: &Client,
    handle: &AppHandle,
    known_room_map: &mut HashMap<OwnedRoomId, RoomNode>,
    prev_seen_servers: &mut HashSet<OwnedRoomId>,
) {
    if should_notification_update(room_updates) {
        let counts = get_notification_counts(client).await;
        send_event(handle, &counts);
    }

    if let Some(call_member_updates) = extract_call_member_updates(room_updates, client).await {
        send_event(handle, &call_member_updates);
    };

    let mut updates = Vec::new();

    let mut visited: HashSet<OwnedRoomId> = HashSet::new();
    let mut queue: Vec<OwnedRoomId> =
        room_updates.joined.keys().chain(room_updates.left.keys()).cloned().collect();

    let mut servers_chaned = false;

    while let Some(room_id) = queue.pop() {
        if !visited.insert(room_id.clone()) {
            continue;
        }
        let Some(room) = client.get_room(&room_id) else { continue };

        if matches!(room.state(), RoomState::Left | RoomState::Banned) {
            if known_room_map.remove(&room_id).is_some() {
                updates.push(RoomMapUpdate::Remove { key: room_id.to_string() });
            }

            servers_chaned = prev_seen_servers.remove(&room_id);

            continue;
        }

        let is_new = !known_room_map.contains_key(&room_id);
        let Some(node) = convert_room_to_node(&room).await else {
            continue;
        };

        if is_new && matches!(node, RoomNode::Server(_)) {
            prev_seen_servers.insert(room_id.clone());
            let servers: Vec<String> = prev_seen_servers.iter().map(|id| id.to_string()).collect();
            send_event(handle, &ServerList(servers));
        }

        if known_room_map.get(&room_id) != Some(&node) {
            updates.push(RoomMapUpdate::Insert { key: room_id.to_string(), value: node.clone() });
            servers_chaned = known_room_map.insert(room_id.clone(), node).is_none();
        }

        if is_new && let Ok(stream) = room.parent_spaces().await {
            for res in stream.collect::<Vec<_>>().await {
                if let Ok(ParentSpace::Reciprocal(parent)) = res {
                    queue.push(parent.room_id().to_owned());
                }
            }
        }
    }

    if servers_chaned {
        let servers: Vec<String> = prev_seen_servers.iter().map(|id| id.to_string()).collect();
        send_event(handle, &ServerList(servers));
    }

    if !updates.is_empty() {
        send_event(handle, &updates);
    }
}

pub fn compute_dm_order(client: &Client, dm_map: &Option<DirectEventContent>) -> Vec<String> {
    let Some(dm_map) = dm_map else { return Vec::new() };
    let mut dms: Vec<_> = dm_map.keys().filter_map(|id| {
        let id = id.as_user_id()?;
        let room = client.get_dm_room(id)?;
        (room.state() == RoomState::Joined).then(|| (room.room_id().to_owned(), room.latest_event_timestamp()))
    }).collect();
    dms.sort_by(|(id1, ts1), (id2, ts2)| ts2.cmp(ts1).then_with(|| id1.cmp(id2)));
    dms.into_iter().map(|(id, _)| id.to_string()).collect()
}

pub fn compute_single_order(client: &Client, known_room_map: &HashMap<OwnedRoomId, RoomNode>) -> Vec<String> {
    let mut singles: Vec<_> = known_room_map.iter().filter_map(|(room_id, node)| {
        if !matches!(node, RoomNode::Single(_)) {
            return None;
        }
        let room = client.get_room(room_id)?;
        (room.state() == RoomState::Joined).then(|| (room_id.clone(), room.latest_event_timestamp()))
    }).collect();
    singles.sort_by(|(id1, ts1), (id2, ts2)| ts2.cmp(ts1).then_with(|| id1.cmp(id2)));
    singles.into_iter().map(|(id, _)| id.to_string()).collect()
}

pub fn handle_account_data(client: &Client ,account_data: &Vec<Raw<AnyGlobalAccountDataEvent>>, dm_map: &mut Option<DirectEventContent>, prev_dm_ids: &mut Vec<String>) -> Option<Vec<String>> {
    for raw in account_data {
        let Ok(AnyGlobalAccountDataEvent::Direct(ev)) = raw.deserialize() else {
            continue;
        };
        *dm_map = Some(ev.content);

        let new_dm_ids = compute_dm_order(client, dm_map);

        if new_dm_ids != *prev_dm_ids {
            *prev_dm_ids = new_dm_ids.clone();
            return Some(new_dm_ids);
        }
    }
    None
}

pub async fn get_notification_counts(client: &Client) -> HashMap<String, NotificationCounts> {
    client
        .rooms()
        .iter()
        .map(|room| {
            let notification_count = room.num_unread_notifications();
            let highlight_count = room.num_unread_mentions();

            (
                room.room_id().to_string(),
                NotificationCounts {
                    notification_count,
                    highlight_count,
                },
            )
        })
        .collect()
}

/// Detects changes of call members and emits the update, for all rooms grouped by room_id.
/// Reads full current state from the room on any CallMember event so leaves are handled correctly.
pub async fn extract_call_member_updates(
    room_updates: &RoomUpdates,
    client: &Client,
) -> Option<CallMemberUpdate> {
    let mut updates = HashMap::new();

    for (room_id, update) in &room_updates.joined {
        let has_call_member_event = update.timeline.events.iter().any(|raw_event| {
            matches!(
                raw_event.raw().deserialize(),
                Ok(AnySyncTimelineEvent::State(AnySyncStateEvent::CallMember(
                    _
                )))
            )
        });

        if !has_call_member_event {
            continue;
        }

        let Some(room) = client.get_room(room_id) else {
            continue;
        };

        let mut user_devices = Vec::new();
        let events = room
            .get_state_events_static::<CallMemberEventContent>()
            .await
            .unwrap_or_default();

        for raw in events {
            let Ok(SyncOrStrippedState::Sync(ev)) = raw.deserialize() else {
                continue;
            };

            let Some(OriginalSyncStateEvent {
                content: CallMemberEventContent::SessionContent(content),
                sender,
                ..
            }) = ev.as_original()
            else {
                continue;
            };

            user_devices.push(UserDevice {
                user_id: sender.to_string(),
                device_id: content.device_id.to_string(),
            });
        }

        // Always insert, even when empty — an empty vec signals that everyone left this room.
        updates.insert(room_id.to_string(), user_devices);
    }

    (!updates.is_empty()).then_some(updates)
}

pub async fn extract_call_memberships(rooms: &[Room]) -> Option<HashMap<String, Vec<UserDevice>>> {
    let mut memberships = HashMap::new();

    for room in rooms {
        if !room.is_call() {
            continue;
        }

        let mut user_devices = Vec::new();
        let events = room
            .get_state_events_static::<CallMemberEventContent>()
            .await
            .unwrap_or_default();

        for raw in events {
            let Ok(SyncOrStrippedState::Sync(ev)) = raw.deserialize() else {
                continue;
            };

            let Some(OriginalSyncStateEvent {
                content: CallMemberEventContent::SessionContent(content),
                sender,
                ..
            }) = ev.as_original()
            else {
                continue;
            };

            user_devices.push(UserDevice {
                user_id: sender.to_string(),
                device_id: content.device_id.to_string(),
            });
        }

        if !user_devices.is_empty() {
            memberships.insert(room.room_id().to_string(), user_devices);
        }
    }

    (!memberships.is_empty()).then_some(memberships)
}
