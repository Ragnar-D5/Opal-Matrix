use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use macros::TauriEvent;
use ruma::{OwnedMxcUri, OwnedRoomId, OwnedUserId};
use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

fn deserialize_u32_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrU32 {
        Num(u32),
        Str(String),
    }

    Ok(match Option::<StringOrU32>::deserialize(d)? {
        Some(StringOrU32::Num(n)) => Some(n),
        Some(StringOrU32::Str(s)) => s.parse().ok(),
        None => None,
    })
}

pub mod errors;
pub mod events;

#[derive(Serialize, Deserialize)]
pub enum RestoreResponse {
    NoSession,
    Success { user_id: OwnedUserId },
    Failed { home_server: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkPreviewResponse {
    #[serde(rename = "og:title", default)]
    pub title: String,
    #[serde(rename = "og:site_name")]
    pub site_name: Option<String>,
    #[serde(rename = "og:description")]
    pub description: Option<String>,
    #[serde(rename = "og:image")]
    pub image_url: Option<OwnedMxcUri>,
    #[serde(
        rename = "og:image:width",
        deserialize_with = "deserialize_u32_or_string",
        default
    )]
    pub image_width: Option<u32>,
    #[serde(
        rename = "og:image:height",
        deserialize_with = "deserialize_u32_or_string",
        default
    )]
    pub image_height: Option<u32>,
    #[serde(rename = "og:url")]
    pub url: Option<String>,
    #[serde(rename = "og:type")]
    pub content_type: Option<String>,
    pub color: Option<String>,
}

impl LinkPreviewResponse {
    pub fn resolve_color(&mut self, original_url: &str, color_map: &HashMap<String, String>) {
        if self.color.is_some() {
            return;
        }

        let Ok(parsed_url) = url::Url::parse(original_url) else {
            return;
        };

        let Ok(host) = parsed_url.host_str().ok_or("URL has no host") else {
            return;
        };

        for (domain, color) in color_map {
            if host.ends_with(domain) {
                self.color = Some(color.clone());
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub source: UiAttachmentSource,
    pub file_name: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum UiAttachmentSource {
    LocalFile(String),
    RawBytes(Vec<u8>),
    Url(String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ScrollDirection {
    Up,
    Down,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GetTimelineResult {
    pub timeline_id: String,
    pub messages: Vec<crate::timeline::UiTimelineItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, TauriEvent)]
pub struct AudioDeviceInfos {
    pub output_devices: Vec<AudioDevice>,
    pub input_devices: Vec<AudioDevice>,

    pub default_output_device_id: Option<String>,
    pub default_input_device_id: Option<String>,

    pub active_output_device_id: Option<String>,
    pub active_input_device_id: Option<String>,
}

impl AudioDeviceInfos {
    pub fn get_active_device(&self, input: bool) -> Option<AudioDevice> {
        if input {
            self.active_input_device_id
                .as_ref()
                .and_then(|id| self.input_devices.iter().find(|d| &d.id == id))
                .cloned()
        } else {
            self.active_output_device_id
                .as_ref()
                .and_then(|id| self.output_devices.iter().find(|d| &d.id == id))
                .cloned()
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, TauriEvent)]
pub struct SearchParameters {
    pub search_id: Uuid,
    pub room_ids: Vec<OwnedRoomId>,
    pub text: String,
    pub senders: Vec<OwnedUserId>,
    pub after: Option<DateTime<Utc>>,
    pub before: Option<DateTime<Utc>>,
    pub has_link: bool,
}

impl SearchParameters {
    /// Builds a tantivy query string for the search index. The indexed fields
    /// are `body` (the default search field), `sender` and `date`.
    pub fn build_query(&self) -> String {
        let mut clauses = Vec::new();

        // Quote each word so tantivy treats it as a literal term instead of
        // query syntax. Matching is case-insensitive because the `body` field
        // is tokenized with the default (lowercasing) tokenizer.
        let words: Vec<_> = self
            .text
            .split_whitespace()
            .map(|w| format!("\"{}\"", w.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect();

        if !words.is_empty() {
            clauses.push(format!("({})", words.join(" OR ")));
        }

        if !self.senders.is_empty() {
            let senders_clause = self
                .senders
                .iter()
                .map(|s| format!("sender:\"{}\"", s))
                .collect::<Vec<_>>()
                .join(" OR ");
            clauses.push(format!("({})", senders_clause));
        }

        if let Some(after) = self.after {
            clauses.push(format!(
                "date:[{} TO *]",
                after.to_rfc3339_opts(SecondsFormat::Secs, true)
            ));
        }
        if let Some(before) = self.before {
            clauses.push(format!(
                "date:[* TO {}}}",
                before.to_rfc3339_opts(SecondsFormat::Secs, true)
            ));
        }

        if self.has_link {
            clauses.push("(http OR https)".to_string());
        }

        clauses.join(" AND ")
    }

    pub fn is_empty(&self, current_room_id: Option<OwnedRoomId>) -> bool {
        self.room_ids.is_empty()
            || (self.room_ids.first().cloned() == current_room_id)
                && self.text.is_empty()
                && self.senders.is_empty()
                && self.after.is_none()
                && self.before.is_none()
                && !self.has_link
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<u16>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, TauriEvent)]
pub enum UpdateDownloadProgress {
    #[default]
    Started,
    InProgress {
        progress: usize,
        total: Option<u64>,
    },
    Finished,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, TauriEvent)]
pub enum UpdateStatus {
    #[default]
    UpToDate,
    UpdateAvailable(UpdateInfo),
    Downloading(UpdateInfo),
    ReadyToInstall(UpdateInfo),
    Installing(UpdateInfo),
    Error {
        short: String,
        long: String,
    },
    CheckingForUpdates,
    RestartRequired,
}

impl UpdateStatus {
    pub fn needs_update_download(&self) -> bool {
        !matches!(
            self,
            UpdateStatus::UpdateAvailable(_)
                | UpdateStatus::Downloading(_)
                | UpdateStatus::CheckingForUpdates
        )
    }

    pub fn is_downloading(&self) -> bool {
        matches!(self, UpdateStatus::Downloading(_))
    }

    pub fn update_available(&self) -> bool {
        matches!(self, UpdateStatus::UpdateAvailable(_))
    }

    pub fn has_action(&self) -> bool {
        matches!(
            self,
            UpdateStatus::RestartRequired
                | UpdateStatus::ReadyToInstall(_)
                | UpdateStatus::UpdateAvailable(_)
                | UpdateStatus::UpToDate
        )
    }

    pub fn has_spinner(&self) -> bool {
        matches!(
            self,
            UpdateStatus::CheckingForUpdates
                | UpdateStatus::Downloading(_)
                | UpdateStatus::Installing(_)
        )
    }
}
