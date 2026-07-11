use chrono_tz::Tz as TimeZone;
use std::collections::HashMap;

use crate::{
    call_tauri,
    hooks::{setup_update_effect, use_tauri_event},
};
use leptos::prelude::*;
use macros::{matrix_settings, EnumVariants};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub trait EnumVariants: Sized + Serialize + DeserializeOwned {
    fn variants() -> impl Iterator<Item = (Self, &'static str)>;
}

#[derive(Clone, PartialEq, Deserialize, Serialize, EnumVariants)]
pub enum HourFormat {
    #[serde(rename = "12-hour")]
    TwelveHour,
    #[serde(rename = "24-hour")]
    TwentyFourHour,
}

#[derive(Clone, PartialEq, Deserialize, Serialize, EnumVariants)]
pub enum DateFormat {
    #[serde(rename = "DD/MM/YYYY")]
    DayMonthYear,
    #[serde(rename = "MM/DD/YYYY")]
    MonthDayYear,
    #[serde(rename = "YYYY/MM/DD")]
    YearMonthDay,
}

#[derive(Clone, PartialEq, Deserialize, Serialize, EnumVariants)]
pub enum DayOfWeek {
    #[serde(rename = "Monday")]
    Monday,
    #[serde(rename = "Tuesday")]
    Tuesday,
    #[serde(rename = "Wednesday")]
    Wednesday,
    #[serde(rename = "Thursday")]
    Thursday,
    #[serde(rename = "Friday")]
    Friday,
    #[serde(rename = "Saturday")]
    Saturday,
    #[serde(rename = "Sunday")]
    Sunday,
}

#[derive(Clone, PartialEq, Deserialize, Serialize, EnumVariants)]
pub enum DataSizeUnit {
    Bytes,
    Bits,
    Mibibytes,
}

impl EnumVariants for TimeZone {
    fn variants() -> impl Iterator<Item = (Self, &'static str)> {
        chrono_tz::TZ_VARIANTS.iter().map(|tz| (*tz, tz.name()))
    }
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
        "Url Previews per room",
        "The number of URL previews to show per room",
        true
    )]
    pub url_previews: HashMap<String, bool>,
    #[setting(
        "Show url perviews by default",
        "The default number of URL previews to show per room",
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
}
