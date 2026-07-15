use matrix_sdk::{
    Client, OwnedServerName, Result,
    room_directory_search::RoomDescription,
    ruma::{
        api::client::directory::get_public_rooms_filtered::v3::Request as PublicRoomsFilterRequest,
        directory::{Filter, RoomTypeFilter},
    },
};
use uuid::Uuid;

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
    results: Vec<RoomDescription>,
    id: Uuid,
}

impl RoomDirectorySearch {
    pub fn new(client: Client, room_types: Vec<RoomTypeFilter>, id: Uuid) -> Self {
        Self {
            client,
            filter: None,
            room_types,
            batch_size: 0,
            server: None,
            state: SearchState::Start,
            results: Vec::new(),
            id,
        }
    }

    pub async fn search(
        &mut self,
        filter: Option<String>,
        batch_size: u32,
        via_server: Option<OwnedServerName>,
    ) -> Result<()> {
        self.filter = filter;
        self.batch_size = batch_size;
        self.server = via_server;
        self.state = SearchState::Start;
        self.results.clear();
        self.next_page().await
    }

    pub async fn next_page(&mut self) -> Result<()> {
        if matches!(self.state, SearchState::End) {
            return Ok(());
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

        let response = self.client.public_rooms_filtered(request).await?;

        self.state = match response.next_batch {
            Some(token) => SearchState::Next(token),
            None => SearchState::End,
        };
        self.results
            .extend(response.chunk.into_iter().map(RoomDescription::from));
        Ok(())
    }

    pub fn results(&self) -> &[RoomDescription] {
        &self.results
    }

    pub fn is_at_last_page(&self) -> bool {
        matches!(self.state, SearchState::End)
    }
}
