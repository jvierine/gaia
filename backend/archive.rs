use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub struct StoredImage {
    pub id: String,
    pub path: PathBuf,
    pub duplicate: bool,
}

pub fn store_image(
    conn: &Connection,
    root: &Path,
    source_id: &str,
    observation: DateTime<Utc>,
    downloaded: DateTime<Utc>,
    timestamp_basis: &str,
    original_url: Option<&str>,
    media_type: &str,
    bytes: &[u8],
) -> Result<StoredImage> {
    let sha = format!("{:x}", Sha256::digest(bytes));
    if let Ok((id, path)) = conn.query_row(
        "SELECT id,archive_path FROM images WHERE sha256=?1",
        [&sha],
        |r| Ok((r.get(0)?, r.get::<_, String>(1)?)),
    ) {
        return Ok(StoredImage {
            id,
            path: PathBuf::from(path),
            duplicate: true,
        });
    }
    let id = Uuid::new_v4().to_string();
    let day = observation.format("%Y-%m-%d").to_string();
    let ext = match media_type {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    };
    let dir = root.join(day).join(source_id).join(&id);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("original.{ext}"));
    let tmp = dir.join(format!(".original.{ext}.tmp"));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, &path)?;
    let decoded=image::load_from_memory(bytes).ok();
    let dimensions=decoded.as_ref().map(|im|(im.width() as i64,im.height() as i64));
    // Reuse the decode already needed for dimensions. New archive frames are
    // playback-ready before they become visible in the image catalogue.
    if let Some(im)=decoded{
        let key=format!("{:x}",Sha256::digest(format!("texture-v1-256:{}",path.to_string_lossy())));
        let cache=root.join("projection-cache");std::fs::create_dir_all(&cache)?;
        let tmp=cache.join(format!("{}.tmp",Uuid::new_v4()));
        im.thumbnail(256,256).to_rgb8().save_with_format(&tmp,image::ImageFormat::Png)?;
        std::fs::rename(tmp,cache.join(format!("{key}.png")))?;
    }
    conn.execute("INSERT INTO images(id,source_id,observation_utc,downloaded_utc,timestamp_basis,archive_path,original_url,sha256,media_type,width,height)
      VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",params![id,source_id,observation.to_rfc3339(),downloaded.to_rfc3339(),timestamp_basis,
      path.to_string_lossy(),original_url,sha,media_type,dimensions.map(|d|d.0),dimensions.map(|d|d.1)])
      .with_context(||format!("cataloguing {}",path.display()))?;
    let metadata = serde_json::json!({"gaia_image_id":id,"source_id":source_id,"observation_utc":observation,
      "downloaded_utc":downloaded,"timestamp_basis":timestamp_basis,"original_url":original_url,"sha256":sha,
      "media_type":media_type,"copyright_retained":true});
    std::fs::write(
        dir.join("metadata.json"),
        serde_json::to_vec_pretty(&metadata)?,
    )?;
    Ok(StoredImage {
        id,
        path,
        duplicate: false,
    })
}

pub fn validate_media_type(value: &str) -> Result<&str> {
    match value.split(';').next().unwrap_or("").trim() {
        "image/jpeg" | "image/jpg" => Ok("image/jpeg"),
        "image/png" => Ok("image/png"),
        "image/webp" => Ok("image/webp"),
        other => Err(anyhow!("unsupported content type {other}")),
    }
}
