use chrono_tz::Tz as TimeZone;
use shared::{
    settings::{DataSizeUnit, DateFormat, DayOfWeek, EnumHashMap, HourFormat},
    timeline::{SystemMessage, SystemMessageDataless},
};
use std::collections::HashMap;

use crate::{
    call_tauri,
    hooks::{setup_update_effect, use_tauri_event},
};
use leptos::prelude::*;
use macros::matrix_settings;

const DEFAULT_SYSTEM_MESSAGES: &[SystemMessageDataless] = &[
    SystemMessageDataless::MembershipChange,
    SystemMessageDataless::RoomCreate,
    SystemMessageDataless::RoomEncryption,
    SystemMessageDataless::RoomPinnedEvents,
    SystemMessageDataless::SpaceChild,
    SystemMessageDataless::SpaceParent,
    SystemMessageDataless::Redacted,
    SystemMessageDataless::Unknown,
    SystemMessageDataless::RoomImagePack,
];

pub fn system_message_modes() -> [(&'static str, &'static [SystemMessageDataless]); 2] {
    [
        ("Default", DEFAULT_SYSTEM_MESSAGES),
        ("Full", SystemMessage::all_variants()),
    ]
}

fn default_system_messages_to_show() -> HashMap<SystemMessageDataless, bool> {
    let mut map = SystemMessage::init_map();
    for message in DEFAULT_SYSTEM_MESSAGES {
        map.insert(*message, true);
    }
    map
}

#[matrix_settings]
pub struct Settings {
    #[setting(
        "Scaling",
        "The scaling factor for the application",
        false,
        default = 1.0
    )]
    pub scaling: f64,
    #[setting(
        "Enable Url Previews per room",
        "Whether to show URL previews per room",
        true
    )]
    pub url_previews: HashMap<String, bool>,
    #[setting(
        "Show url previews by default",
        "Whether to show URL previews by default when not specified per room",
        true,
        default = false
    )]
    pub url_previews_default: bool,
    #[setting(
        "Show image border",
        "Whether to show the image border",
        false,
        default = true
    )]
    pub show_image_border: bool,
    #[setting(
        "Automatically download updates",
        "Whether to automatically download updates when a new version is available",
        true,
        default = false
    )]
    pub auto_download_update: bool,
    #[setting(
        "Notify when an update is available",
        "Whether to notify the user when an update is available",
        true,
        default = true
    )]
    pub notify_update: bool,
    #[setting(
        "Show read markers",
        "Whether to show read markers in the chat",
        true,
        default = true
    )]
    pub show_read_markers: bool,
    #[setting(
        "Send read markers",
        "Whether to send read markers to the server",
        true,
        default = true
    )]
    pub send_read_markers: bool,
    #[setting(
        "Show typing indicators",
        "Whether to show typing indicators in the chat",
        true,
        default = true
    )]
    pub show_typing_indicators: bool,
    #[setting(
        "Send typing indicators",
        "Whether to send typing indicators to the server",
        true,
        default = true
    )]
    pub send_typing_indicators: bool,
    #[setting(
        "Timezone",
        "The timezone to use for the chat",
        false,
        default = chrono_tz::Tz::UTC
    )]
    pub timezone: TimeZone,
    #[setting(
        "Data size unit",
        "The unit to use for data size",
        true,
        default = DataSizeUnit::Mibibytes
    )]
    pub data_size_unit: DataSizeUnit,
    #[setting(
        "Hour format",
        "The hour format to use for timestamps",
        true,
        default = HourFormat::TwentyFourHour
    )]
    pub hour_format: HourFormat,
    #[setting(
        "Date format",
        "The date format to use for timestamps",
        true,
        default = DateFormat::DayMonthYear
    )]
    pub date_format: DateFormat,
    #[setting(
        "First day of week",
        "The first day of the week",
        true,
        default = DayOfWeek::Monday
    )]
    pub first_day_of_week: DayOfWeek,
    #[setting(
        "Mark pinned messages",
        "Whether to mark pinned messages visually in the chat",
        true,
        default = true
    )]
    pub mark_pinned_messages: bool,
    #[setting(
        "Which system messages to show",
        "Which system messages to show in the chat",
        true,
        default = default_system_messages_to_show()
    )]
    pub system_messages_to_show: HashMap<SystemMessageDataless, bool>,
    #[setting(
        "Minimize to tray",
        "Whether to minimize the window to the system tray",
        true,
        default = false
    )]
    pub minimize_to_tray: bool,
}
