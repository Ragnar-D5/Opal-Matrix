use matrix_sdk::{
    Client, OwnedServerName, Result,
    ruma::{
        api::client::directory::get_public_rooms_filtered::v3::Request as PublicRoomsFilterRequest,
        directory::{Filter, PublicRoomsChunk, RoomTypeFilter},
    },
};
use tauri::{State, command, ipc::Channel};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    TauriError,
    state::{RoomSearchManager, TaskManager},
};

#[derive(Default, Debug)]
enum SearchState {
    #[default]
    Start,
    Next(String),
    End,
}

pub struct RoomDirectorySearch {
    client: Client,
    filter: Option<String>,
    room_types: Vec<RoomTypeFilter>,
    batch_size: u32,
    server: Option<OwnedServerName>,
    state: SearchState,
    results: Vec<PublicRoomsChunk>,
    channel: Channel<Vec<PublicRoomsChunk>>,
}

impl RoomDirectorySearch {
    pub fn new(
        client: Client,
        room_types: Vec<RoomTypeFilter>,
        channel: Channel<Vec<PublicRoomsChunk>>,
    ) -> Self {
        Self {
            client,
            filter: None,
            room_types,
            batch_size: 0,
            server: None,
            state: SearchState::Start,
            results: Vec::new(),
            channel,
        }
    }

    pub async fn search(
        &mut self,
        filter: Option<String>,
        batch_size: u32,
        via_server: Option<OwnedServerName>,
        cancel: &CancellationToken,
    ) -> Result<bool> {
        self.filter = filter;
        self.batch_size = batch_size;
        self.server = via_server;
        self.state = SearchState::Start;
        self.results.clear();
        self.next_page(cancel).await
    }

    pub async fn next_page(&mut self, cancel: &CancellationToken) -> Result<bool> {
        if matches!(self.state, SearchState::End) {
            return Ok(true);
        }

        let since = if let SearchState::Next(token) = &self.state {
            Some(token.clone())
        } else {
            None
        };

        let mut filter = Filter::new();
        filter.generic_search_term = self.filter.clone();
        filter.room_types = self.room_types.clone();

        let mut request = PublicRoomsFilterRequest::new();
        request.filter = filter;
        request.server = self.server.clone();
        request.limit = Some(self.batch_size.into());
        request.since = since;

        // A newer `search`/`next_page` call for this session cancels this
        // token, so a superseded request drops here instead of finishing its
        // round-trip and briefly flashing stale results before the newer
        // one's own response arrives.
        let response = tokio::select! {
            _ = cancel.cancelled() => {
                log::trace!("Room directory search superseded, dropping stale response");
                return Ok(true);
            }
            response = self.client.public_rooms_filtered(request) => response?,
        };

        self.state = match response.next_batch {
            Some(token) => SearchState::Next(token),
            None => SearchState::End,
        };
        self.results.extend(response.chunk);
        self.notify_results();

        Ok(self.is_at_last_page())
    }

    fn notify_results(&self) {
        if let Err(e) = self.channel.send(self.results.clone()) {
            log::warn!("Failed to send room search results over channel: {e}");
        }
    }

    pub fn is_at_last_page(&self) -> bool {
        matches!(self.state, SearchState::End)
    }
}

const ROOM_SEARCH_BATCH_SIZE: u32 = 50;

fn cancellation_key(id: Uuid) -> String {
    format!("room_search_{id}")
}

#[command(rename_all = "snake_case")]
pub async fn open_room_search(
    client: State<'_, RwLock<Client>>,
    manager: State<'_, RoomSearchManager>,
    id: Uuid,
    room_types: Vec<RoomTypeFilter>,
    channel: Channel<Vec<PublicRoomsChunk>>,
) -> Result<(), TauriError> {
    let client = client.read().await.clone();
    manager.create(id, client, room_types, channel).await;
    Ok(())
}

#[command(rename_all = "snake_case")]
pub async fn search_room_directory(
    manager: State<'_, RoomSearchManager>,
    task_manager: State<'_, TaskManager>,
    id: Uuid,
    term: Option<String>,
) -> Result<bool, TauriError> {
    let Some(search) = manager.get(id).await else {
        return Err(TauriError::from("No room search session open for this id"));
    };

    let token = CancellationToken::new();
    task_manager
        .replace_task(&cancellation_key(id), token.clone())
        .await;

    search
        .write()
        .await
        .search(term, ROOM_SEARCH_BATCH_SIZE, None, &token)
        .await
        .map_err(TauriError::from)
}

#[command(rename_all = "snake_case")]
pub async fn load_more_room_search_results(
    manager: State<'_, RoomSearchManager>,
    task_manager: State<'_, TaskManager>,
    id: Uuid,
) -> Result<bool, TauriError> {
    let Some(search) = manager.get(id).await else {
        return Err(TauriError::from("No room search session open for this id"));
    };

    let token = CancellationToken::new();
    task_manager
        .replace_task(&cancellation_key(id), token.clone())
        .await;

    search
        .write()
        .await
        .next_page(&token)
        .await
        .map_err(TauriError::from)
}

#[command(rename_all = "snake_case")]
pub async fn close_room_search(
    manager: State<'_, RoomSearchManager>,
    id: Uuid,
) -> Result<(), TauriError> {
    manager.remove(id).await;
    Ok(())
}
