use std::fmt::Debug;

use super::DataBaseModel;
use ruma::events::room::member::MembershipState as RumaMembershipState;
use rusqlite::ToSql;

#[derive(Debug, Clone)]
pub enum MembershipState {
    Join,
    Invite,
    Leave,
    Ban,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct MemberRow {
    pub room_id: String,
    pub user_id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub membership: MembershipState,
}

impl From<RumaMembershipState> for MembershipState {
    fn from(state: RumaMembershipState) -> Self {
        match state {
            RumaMembershipState::Join => MembershipState::Join,
            RumaMembershipState::Invite => MembershipState::Invite,
            RumaMembershipState::Leave => MembershipState::Leave,
            RumaMembershipState::Ban => MembershipState::Ban,
            _ => MembershipState::Unknown,
        }
    }
}

impl ToSql for MembershipState {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let s = match self {
            MembershipState::Join => "join",
            MembershipState::Invite => "invite",
            MembershipState::Leave => "leave",
            MembershipState::Ban => "ban",
            _ => "leave", // Default to 'leave' for unknown states
        };
        Ok(s.into())
    }
}

impl DataBaseModel for MemberRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS members (
                room_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                display_name TEXT,
                avatar_url TEXT,
                membership TEXT NOT NULL CHECK(membership IN ('join', 'invite', 'leave', 'ban')),
                PRIMARY KEY (room_id, user_id),
                FOREIGN KEY (room_id) REFERENCES rooms(room_id)
            );

            CREATE INDEX IF NOT EXISTS idx_members_user_id ON members(user_id);
            ",
        )?;
        Ok(())
    }
}
