use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};

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
pub mod signals;

#[derive(Serialize, Deserialize)]
pub enum RestoreResponse {
    NoSession,
    Success { user_id: String },
    Failed { home_server: String },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinkPreviewResponse {
    #[serde(rename = "og:title")]
    pub title: String,
    #[serde(rename = "og:site_name")]
    pub site_name: Option<String>,
    #[serde(rename = "og:description")]
    pub description: Option<String>,
    #[serde(rename = "og:image")]
    pub image_url: Option<String>,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AudioDeviceInfos {
    pub output_devices: Vec<AudioDevice>,
    pub input_devices: Vec<AudioDevice>,

    pub default_output_device_id: Option<String>,
    pub default_input_device_id: Option<String>,

    pub active_output_devive_id: Option<String>,
    pub active_input_device_id: Option<String>,
}
