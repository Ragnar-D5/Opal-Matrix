use std::fmt::Display;

use super::DataBaseModel;
pub use ruma::events::room::guest_access::GuestAccess;
pub use ruma::events::room::history_visibility::HistoryVisibility;

#[derive(Debug)]
pub enum JoinRule {
    Public,
    Invite,
    Knock,
    Private,
    Restricted,
    KnockRestricted,
}

impl From<ruma::events::room::join_rules::JoinRule> for JoinRule {
    fn from(join_rule: ruma::events::room::join_rules::JoinRule) -> Self {
        match join_rule {
            ruma::events::room::join_rules::JoinRule::Public => JoinRule::Public,
            ruma::events::room::join_rules::JoinRule::Invite => JoinRule::Invite,
            ruma::events::room::join_rules::JoinRule::Knock => JoinRule::Knock,
            ruma::events::room::join_rules::JoinRule::Private => JoinRule::Private,
            ruma::events::room::join_rules::JoinRule::Restricted(_) => JoinRule::Restricted,
            ruma::events::room::join_rules::JoinRule::KnockRestricted(_) => {
                JoinRule::KnockRestricted
            }
            _ => JoinRule::Private,
        }
    }
}

impl Display for JoinRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            JoinRule::Public => "public",
            JoinRule::Invite => "invite",
            JoinRule::Knock => "knock",
            JoinRule::Private => "private",
            JoinRule::Restricted => "restricted",
            JoinRule::KnockRestricted => "knock_restricted",
        };
        write!(f, "{s}")
    }
}

#[derive(Debug)]
pub struct RoomRow {
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,

    pub algorithm: Option<String>,

    pub is_direct: bool,
    pub join_rule: JoinRule,
    pub history_visibility: HistoryVisibility,
    pub guest_access: GuestAccess,

    pub power_levels: Option<String>,
}

impl DataBaseModel for RoomRow {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS rooms (
                room_id TEXT PRIMARY KEY,
                name TEXT,
                topic TEXT,
                avatar_url TEXT,

                algorithm TEXT,

                is_direct BOOLEAN NOT NULL DEFAULT 0,
                join_rule TEXT NOT NULL CHECK(join_rule IN ('public', 'invite', 'knock', 'private', 'restricted', 'knock_restricted')) DEFAULT 'private',
                history_visibility TEXT NOT NULL CHECK(history_visibility IN ('world_readable', 'shared', 'invited', 'joined')) DEFAULT 'shared',
                guest_access TEXT NOT NULL CHECK(guest_access IN ('can_join', 'forbidden')) DEFAULT 'forbidden',

                power_levels TEXT
            )",
        )?;
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct RoomUpdate {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub power_levels: Option<String>,
    pub guest_access: Option<String>,
    pub history_visibility: Option<String>,
    pub join_rule: Option<String>,
    pub algorithm: Option<String>,
}
