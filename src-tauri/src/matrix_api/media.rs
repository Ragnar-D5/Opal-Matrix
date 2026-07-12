use base64::{engine::general_purpose, Engine};
use matrix_sdk::{
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{
        events::room::{
            EncryptedFile, EncryptedFileHashes, EncryptedFileInfo, MediaSource, V2EncryptedFileInfo,
        },
        media::Method,
        profile::ProfileFieldName,
        serde::{
            base64::{Standard, UrlSafe},
            Base64,
        },
        OwnedMxcUri, RoomId, UserId,
    },
    Client, RoomMemberships,
};
use shared::timeline::UiMediaSource;
use uuid::Uuid;

use crate::{state::MediaManager, TauriError};

pub async fn get_media(
    client: &Client,
    source: MediaSource,
    format: MediaFormat,
) -> Result<Vec<u8>, TauriError> {
    let params = MediaRequestParameters {
        source: source.clone(),
        format,
    };

    let media = client
        .media()
        .get_media_content(&params, true)
        .await
        .map_err(|e| format!("Failed to fetch media for {:?}: {:?}", source, e))?;
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

    drop(sources);

    let bytes = get_media(client, media_source, MediaFormat::File).await?;
    log::trace!("Fetched {} bytes for UUID {}", bytes.len(), uuid_str);
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

pub async fn get_media_bytes(
    client: &Client,
    source: UiMediaSource,
    media_manager: &MediaManager,
) -> Result<Vec<u8>, TauriError> {
    let format = MediaFormat::File;

    match source {
        UiMediaSource::Encrypted { url, k, iv } => {
            let Ok(k) = general_purpose::URL_SAFE.decode(k)?.try_into() else {
                return Err("Invalid key format".into());
            };
            let Ok(iv) = general_purpose::URL_SAFE.decode(iv)?.try_into() else {
                return Err("Invalid IV format".into());
            };

            let k: Base64<UrlSafe, [u8; 32]> = Base64::new(k);
            let iv: Base64<Standard, [u8; 16]> = Base64::new(iv);

            let request = MediaRequestParameters {
                source: MediaSource::Encrypted(Box::new(EncryptedFile::new(
                    OwnedMxcUri::from(url),
                    EncryptedFileInfo::V2(V2EncryptedFileInfo::new(k, iv)),
                    EncryptedFileHashes::new(),
                ))),
                format,
            };

            client
                .media()
                .get_media_content(&request, false)
                .await
                .map_err(|e| format!("Failed to fetch encrypted media: {:?}", e).into())
        }
        UiMediaSource::Plain(path) => {
            let request = MediaRequestParameters {
                source: MediaSource::Plain(OwnedMxcUri::from(path)),
                format,
            };

            client
                .media()
                .get_media_content(&request, false)
                .await
                .map_err(|e| format!("Failed to fetch media: {:?}", e).into())
        }
        UiMediaSource::Uuid(uuid) => {
            let sources = media_manager.sources.read().await;

            let media_source = sources
                .get(&uuid)
                .ok_or(format!("No media found for UUID: {}", uuid))?
                .clone();

            drop(sources);

            get_media(client, media_source, format).await
        }
    }
}

pub async fn get_user_avatar(
    client: &Client,
    user_id: &str,
) -> Result<Option<Vec<u8>>, TauriError> {
    let user_id = UserId::parse(user_id)?;
    let Some(value) = client
        .account()
        .fetch_profile_field_of(user_id, ProfileFieldName::AvatarUrl)
        .await
        .ok()
        .unwrap_or_default()
    else {
        return Ok(None);
    };

    let value = value.value();

    let mxc_str = match value.as_str() {
        Some(s) => s,
        None => return Err("Failed to convert url to string".into()),
    };

    log::debug!("Fetched avatar URL: {}", mxc_str);

    let request = MediaRequestParameters {
        source: MediaSource::Plain(OwnedMxcUri::from(mxc_str)),
        format: MediaFormat::File,
    };

    match client.media().get_media_content(&request, true).await {
        Ok(media) => Ok(Some(media)),
        Err(e) => Err(format!("Failed to fetch avatar media: {:?}", e).into()),
    }
}

/// Fetches media content directly from a literal `mxc://` content URI, without going
/// through local room/user state. Used for previews (e.g. unjoined room avatars) where
/// we only have the raw URI from a server response like the space hierarchy endpoint.
pub async fn get_direct_media(
    client: &Client,
    mxc_uri: &str,
) -> Result<Option<Vec<u8>>, TauriError> {
    let request = MediaRequestParameters {
        source: MediaSource::Plain(OwnedMxcUri::from(mxc_uri)),
        format: MediaFormat::File,
    };

    match client.media().get_media_content(&request, true).await {
        Ok(media) => Ok(Some(media)),
        Err(e) => Err(format!("Failed to fetch direct media {}: {:?}", mxc_uri, e).into()),
    }
}

pub async fn get_room_avatar(
    client: &Client,
    room_id: &str,
) -> Result<Option<Vec<u8>>, TauriError> {
    let room_id = RoomId::parse(room_id)?;

    let Some(room) = client.get_room(&room_id) else {
        return Ok(None);
    };

    room.avatar(MediaFormat::File)
        .await
        .map_err(|e| format!("Failed to fetch room avatar: {:?}", e).into())
}

pub async fn get_member_avatar(
    client: &Client,
    room_id: &str,
    user_id: &str,
) -> Result<Option<Vec<u8>>, TauriError> {
    let room_id = RoomId::parse(room_id)?;
    let user_id = UserId::parse(user_id)?;

    let Some(room) = client.get_room(&room_id) else {
        return Ok(None);
    };

    let members = room.members(RoomMemberships::JOIN).await?;
    let Some(member) = members.iter().find(|m| m.user_id() == user_id) else {
        return Ok(None);
    };

    member
        .avatar(MediaFormat::File)
        .await
        .map_err(|e| format!("Failed to get member avatar: {:?}", e).into())
}
