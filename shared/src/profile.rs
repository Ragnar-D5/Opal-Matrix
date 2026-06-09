use csscolorparser::Color;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum PresenceStatus {
    Online,
    #[default]
    Offline,
    Unavailable,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PresenceInfo {
    pub status: PresenceStatus,
    pub status_msg: Option<String>,
    pub last_active_ago: Option<u64>,
}

impl PresenceInfo {
    pub fn is_offline(&self) -> bool {
        matches!(self.status, PresenceStatus::Offline)
    }

    pub fn new_online() -> Self {
        Self {
            status: PresenceStatus::Online,
            status_msg: None,
            last_active_ago: None,
        }
    }

    pub fn new_offline() -> Self {
        Self {
            status: PresenceStatus::Offline,
            status_msg: None,
            last_active_ago: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub display_name: Option<String>,

    pub has_avatar: bool,

    pub custom_properties: CustomProperties,
}

impl UserProfile {
    pub fn get_name(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| self.user_id.clone())
    }

    pub fn get_avatar_url(&self, room_id: &str) -> String {
        format!("mxc://user/{}/room/{room_id}", self.user_id)
    }

    pub fn get_sonic_signature(&self) -> SonicSignature {
        self.custom_properties.sonic_signature.clone()
    }

    pub fn name_color(&self) -> Color {
        self.custom_properties.name_color.clone()
    }

    pub fn banner_color(&self) -> Color {
        self.custom_properties.banner_color.clone()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberProfile {
    pub room_id: String,
    pub profile: UserProfile,
}

impl MemberProfile {
    pub fn get_name(&self) -> String {
        self.profile.get_name()
    }

    pub fn get_avatar_url(&self) -> String {
        self.profile.get_avatar_url(&self.room_id)
    }

    pub fn user_id(&self) -> &str {
        &self.profile.user_id
    }

    pub fn get_sonic_signature(&self) -> SonicSignature {
        self.profile.get_sonic_signature()
    }

    pub fn name_color(&self) -> Color {
        self.profile.name_color()
    }

    pub fn banner_color(&self) -> Color {
        self.profile.banner_color()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomProfile {
    pub room_id: String,
    pub aliases: Vec<String>,
    pub canonical_alias: Option<String>,
    pub name: Option<String>,
}

impl RoomProfile {
    pub fn get_name(&self) -> String {
        self.name
            .clone()
            .map(|n| format!("#{n}"))
            .or_else(|| self.canonical_alias.clone())
            .or_else(|| self.aliases.first().cloned())
            .unwrap_or_else(|| self.room_id.clone())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    #[default]
    MajorPentatonic,
    MinorPentatonic,
    Dorian,
    Mixolydian,
    Lydian,
    NaturalMinor,
}

impl Scale {
    pub fn intervals(&self) -> &'static [u8] {
        match self {
            Scale::MajorPentatonic => &[0, 2, 4, 7, 9],
            Scale::MinorPentatonic => &[0, 3, 5, 7, 10],
            Scale::Dorian => &[0, 2, 3, 5, 7, 9, 10],
            Scale::Mixolydian => &[0, 2, 4, 5, 7, 9, 10],
            Scale::Lydian => &[0, 2, 4, 6, 7, 9, 11],
            Scale::NaturalMinor => &[0, 2, 3, 5, 7, 8, 10],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rhythm {
    #[default]
    Even,
    Tresillo,
    Gallop,
    Dotted,
    LongShort,
    Syncopated,
    Cascade,
}

impl Rhythm {
    pub fn pattern(&self) -> &'static [u8] {
        match self {
            Rhythm::Even => &[2, 2, 2, 2],
            Rhythm::Tresillo => &[3, 3, 2],
            Rhythm::Gallop => &[1, 1, 2, 2, 2],
            Rhythm::Dotted => &[3, 1, 3, 1],
            Rhythm::LongShort => &[4, 2, 2],
            Rhythm::Syncopated => &[2, 3, 3],
            Rhythm::Cascade => &[1, 1, 1, 1, 4],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    #[default]
    Synth,
    Pluck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SonicSignature {
    pub root: u8,
    pub scale: Scale,
    pub progression: Vec<i8>,
    pub rhythm: Rhythm,
    pub instrument: Instrument,
}

const SCALES: [Scale; 6] = [
    Scale::MajorPentatonic,
    Scale::MinorPentatonic,
    Scale::Dorian,
    Scale::Mixolydian,
    Scale::Lydian,
    Scale::NaturalMinor,
];
const RHYTHMS: [Rhythm; 7] = [
    Rhythm::Even,
    Rhythm::Tresillo,
    Rhythm::Gallop,
    Rhythm::Dotted,
    Rhythm::LongShort,
    Rhythm::Syncopated,
    Rhythm::Cascade,
];
const PROGS: [&[i8]; 7] = [
    &[0, 4],
    &[0, 5],
    &[5, 0],
    &[0, 3],
    &[3, 4],
    &[0, 4, 5],
    &[0, 5, 3],
];

impl SonicSignature {
    pub fn from_user_id(s: &str) -> Self {
        let hash = Sha256::digest(s.as_bytes());

        Self {
            scale: SCALES[hash[0] as usize % SCALES.len()],
            root: hash[1] % 12,
            progression: PROGS[hash[2] as usize % PROGS.len()].to_vec(),
            rhythm: RHYTHMS[hash[6] as usize % RHYTHMS.len()],
            instrument: if hash[7] & 1 == 0 {
                Instrument::Synth
            } else {
                Instrument::Pluck
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomProperties {
    pub banner_color: Color,
    pub name_color: Color,
    pub sonic_signature: SonicSignature,
}

impl CustomProperties {
    pub fn from_user_id(user_id: &str) -> Self {
        let hash = Sha256::digest(user_id.as_bytes());
        let h = hash[0] as f32 / 255.0 * 360.0;
        let color = Color::from_hsla(h, 0.9, 0.7, 1.0);

        Self {
            banner_color: color.clone(),
            name_color: color,
            sonic_signature: SonicSignature::from_user_id(user_id),
        }
    }
}

impl Default for CustomProperties {
    fn default() -> Self {
        let default_color = Color::from_hsla(0.0, 1.0, 1.0, 1.0);

        Self {
            banner_color: default_color.clone(),
            name_color: default_color,
            sonic_signature: SonicSignature::default(),
        }
    }
}
