use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::messages::UiMessage;

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
    #[serde(rename = "og:image:width")]
    pub image_width: Option<u32>,
    #[serde(rename = "og:image:height")]
    pub image_height: Option<u32>,
    #[serde(rename = "og:url")]
    pub url: Option<String>,
    #[serde(rename = "og:type")]
    pub content_type: Option<String>,
    pub color: Option<String>,
}

impl LinkPreviewResponse {
    pub fn resolve_color(&mut self, original_url: &String, color_map: HashMap<String, String>) {
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
            if host.ends_with(&domain) {
                self.color = Some(color);
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchMessagesResponse {
    pub messages: Vec<UiMessage>,
    pub has_more: bool,
}
