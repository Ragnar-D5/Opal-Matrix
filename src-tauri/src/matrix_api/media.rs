use matrix_sdk::{
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{events::room::MediaSource, media::Method},
    Client,
};
use uuid::Uuid;

use crate::{state::MediaManager, TauriError};

pub async fn get_media(
    client: &Client,
    source: MediaSource,
    format: MediaFormat,
) -> Result<Vec<u8>, TauriError> {
    let params = MediaRequestParameters { source, format };

    let media = client.media().get_media_content(&params, false).await?;
    Ok(media)
}

pub async fn get_media_from_uuid_str(
    client: &Client,
    uuid_str: &str,
    media_manager: &MediaManager,
) -> Result<Vec<u8>, TauriError> {
    let uuid = Uuid::parse_str(uuid_str)?;

    let sources = media_manager.sources.read().await;

    let media_source = sources
        .get(&uuid)
        .ok_or(format!("No media found for UUID: {}", uuid_str))?
        .clone();

    log::debug!("Fetching media for UUID {}: {:?}", uuid_str, media_source);
    drop(sources);

    let bytes = get_media(client, media_source, MediaFormat::File).await?;
    log::debug!("Fetched {} bytes for UUID {}", bytes.len(), uuid_str);
    Ok(bytes)
}

pub async fn get_media_from_uuid_thmubnail_str(
    client: &Client,
    str: &str,
    media_manager: &MediaManager,
) -> Result<Vec<u8>, TauriError> {
    let (uuid_str, param_str) = str.split_once("?").ok_or("Invalid media string format")?;

    let uuid = Uuid::parse_str(uuid_str)?;

    let mut settings = MediaThumbnailSettings::new(100u32.into(), 100u32.into());

    for param in param_str.split("&") {
        let (key, value) = param
            .split_once("=")
            .ok_or("Invalid media parameter format")?;
        match key {
            "width" => {
                let width: u32 = value.parse()?;
                if width == 0 {
                    return Err("Width must be greater than 0".into());
                }

                settings.width = width.into();
            }
            "height" => {
                let height: u32 = value.parse()?;
                if height == 0 {
                    return Err("Height must be greater than 0".into());
                }

                settings.height = height.into();
            }
            "method" => {
                if value != "crop" && value != "scale" {
                    return Err("Method must be 'crop' or 'scale'".into());
                }

                settings.method = Method::from(value);
            }
            "animated" => {
                let animated: bool = value.parse()?;
                settings.animated = animated;
            }
            _ => return Err(format!("Unknown media parameter: {}", key).into()),
        }
    }

    let sources = media_manager.sources.read().await;

    let media_source = sources
        .get(&uuid)
        .ok_or(format!("No media found for UUID: {}", uuid_str))?;

    get_media(
        client,
        media_source.clone(),
        MediaFormat::Thumbnail(settings),
    )
    .await
}
