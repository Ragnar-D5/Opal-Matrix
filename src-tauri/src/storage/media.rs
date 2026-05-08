//! This will contain the logic for storing already downloaded media
//! in order to serve it more quickly instead of lazy loading everything

use std::{collections::HashMap, sync::Arc};

use log::info;
use rusqlite::params;
use tauri::State;

use crate::{
    AppState, AsInfo, TauriError,
    storage::{DataBaseModel, media},
};

/// A Media item identified by it's URI.
pub struct Media {
    pub identifier: String,
    pub content: Vec<u8>,
}

impl DataBaseModel for Media {
    fn create_table(conn: &rusqlite::Connection) -> Result<(), crate::TauriError> {
        conn.execute_batch(
            "
                CREATE TABLE IF NOT EXISTS media_cache (
                    identifier TEXT PRIMARY KEY,
                    content BLOB
                )
            ",
        )?;
        Ok(())
    }
}

/// Retrieves a cached media item by its URI from the database.
///
/// Takes a `mxc://` URI and returns the cached media item if found.
/// TODO: Should we clone it? Does this already clone it?
pub async fn get_cached_media_by_uri(
    uri: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Media, TauriError> {
    let conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_ref()
        .ok_or("Database connection not available")?;

    let mut stmt = conn.prepare("SELECT content FROM media_cache WHERE identifier = ?1")?;
    let mut rows = stmt.query([uri.clone()])?;
    if let Some(row) = rows.next()? {
        let content = row.get::<_, Vec<u8>>(0)?;
        Ok(Media {
            identifier: uri,
            content,
        })
    } else {
        Err("Media not found".as_info())
    }
}

/// Caches a media item in the database.
///
/// Consumes the `Media` struct and stores it in the database.
pub async fn cache_media(media: Media, state: State<'_, Arc<AppState>>) -> Result<(), TauriError> {
    let conn_guard = state.connection.lock().await;
    let conn = conn_guard
        .as_ref()
        .ok_or("Database connection not available")?;

    conn.execute(
        "INSERT INTO media_cache (identifier, content) VALUES (?1, ?2)",
        params![media.identifier, media.content],
    )?;
    Ok(())
}

use ffmpeg_next as ffmpeg;
use std::io::{Read, Write};
use tempfile::NamedTempFile;

pub fn add_faststart_to_video(bytes: &[u8]) -> Result<Vec<u8>, TauriError> {
    ffmpeg::init()?;

    let mut tmp_input = NamedTempFile::new()?;
    let tmp_output = NamedTempFile::new()?;
    let out_path = tmp_output.path().to_str().unwrap().to_string();
    tmp_input.write_all(bytes)?;

    let mut ictx = ffmpeg::format::input(&tmp_input.path())?;
    let mut octx = ffmpeg::format::output_as(&out_path, "mp4")?;

    let mut stream_mapping = HashMap::new();
    for stream in ictx.streams() {
        let index = stream.index();
        let mut out_stream = octx.add_stream(ffmpeg::encoder::find(stream.parameters().id()))?;
        out_stream.set_parameters(stream.parameters());
        stream_mapping.insert(index, out_stream.index());
    }

    let mut opts = ffmpeg::Dictionary::new();
    opts.set("movflags", "faststart");

    octx.write_header_with(opts)?;

    let time_bases: Vec<_> = ictx.streams().map(|s| s.time_base()).collect();

    for (stream, mut packet) in ictx.packets() {
        if let Some(&out_index) = stream_mapping.get(&stream.index()) {
            let in_time_base = time_bases[stream.index()]; // No borrow of ictx here
            let out_time_base = octx.stream(out_index).unwrap().time_base();

            packet.set_stream(out_index);
            packet.rescale_ts(in_time_base, out_time_base);
            packet.write_interleaved(&mut octx)?;
        }
    }
    octx.write_trailer()?;

    let mut result_bytes = Vec::new();
    let mut out_file = std::fs::File::open(out_path)?;
    out_file.read_to_end(&mut result_bytes)?;

    Ok(result_bytes)
}
