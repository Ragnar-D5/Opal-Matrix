use std::collections::HashMap;

use macros::EnumVariants;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const AUTO_UPDATE_SETTINGS_KEY: &str = "auto_update_settings";

pub trait EnumVariants: Sized + Serialize + DeserializeOwned {
    fn variants() -> impl Iterator<Item = (Self, &'static str)>;
}

pub trait EnumHashMap: Sized + Serialize + DeserializeOwned + PartialEq + Eq {
    /// Mirror of this enum with the same variants but no data.
    type Dataless: Copy + Eq + std::hash::Hash + 'static;

    /// The dataless variant matching this value, ignoring any payload.
    fn dataless(&self) -> Self::Dataless;

    /// Whether this value is the given dataless variant, ignoring any payload.
    fn is_valid(&self, valid_map: &HashMap<Self::Dataless, bool>) -> bool {
        valid_map.get(&self.dataless()).copied().unwrap_or(false)
    }

    fn all_variants() -> &'static [Self::Dataless];

    /// All dataless variants paired with their display names.
    fn dataless_variants() -> impl Iterator<Item = (Self::Dataless, &'static str)>;

    /// Every dataless variant mapped to `false`.
    fn init_map() -> std::collections::HashMap<Self::Dataless, bool> {
        Self::dataless_variants()
            .map(|(variant, _)| (variant, false))
            .collect()
    }
}

impl EnumVariants for chrono_tz::Tz {
    fn variants() -> impl Iterator<Item = (Self, &'static str)> {
        chrono_tz::TZ_VARIANTS.iter().map(|tz| (*tz, tz.name()))
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    Profile,
    General,
    Appearance,
    Audio,
    Chats,
    Updates,
    Divider,
}

impl SettingsSection {
    pub fn id(&self) -> &'static str {
        match self {
            SettingsSection::Profile => "profile",
            SettingsSection::General => "general",
            SettingsSection::Appearance => "appearance",
            SettingsSection::Audio => "audio",
            SettingsSection::Chats => "chats",
            SettingsSection::Updates => "updates",
            SettingsSection::Divider => "divider",
        }
    }
}
