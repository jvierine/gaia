//! Calibration, masking and 100 km projection for isolated event-study media.
//!
//! These records never enter `sources`, `images`, `calibrations` or the
//! realtime publisher.  An event photograph is calibrated as one observation;
//! its assets are prepared on demand and consumed only by the event viewer.
use crate::{db, internal, projection, publish_layers, ApiResult, AppState};
use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, Response, StatusCode},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path as FsPath, PathBuf};

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventRecord {
    id: String,
    source: String,
    kind: String,
    title: Option<String>,
    creator: Option<String>,
    location: String,
    latitude: f64,
    longitude: f64,
    captured_at: String,
    preview_url: String,
    source_url: String,
    license: Option<String>,
    rights_note: Option<String>,
}
#[derive(Deserialize)]
struct EventManifest {
    items: Vec<EventRecord>,
}

fn valid_event(id: &str) -> bool {
    id.len() == 8 && id.bytes().all(|b| b.is_ascii_digit())
}
fn valid_asset(name:&str)->bool{name.rsplit_once('.').is_some_and(|(stem,extension)|{
    stem.len()==64&&stem.bytes().all(|b|b.is_ascii_hexdigit())&&matches!(extension,"bin"|"jpg")
})}
fn manifest_path(s: &AppState, event: &str) -> ApiResult<PathBuf> {
    if !valid_event(event) {
        return Err((StatusCode::BAD_REQUEST, "invalid event id".into()));
    }
    Ok(s.archive_root
        .join("public/events")
        .join(event)
        .join("manifest.json"))
}
fn records(s: &AppState, event: &str) -> ApiResult<Vec<EventRecord>> {
    let path = manifest_path(s, event)?;
    let bytes = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            (StatusCode::NOT_FOUND, "event not found".into())
        } else {
            internal(e)
        }
    })?;
    Ok(serde_json::from_slice::<EventManifest>(&bytes)
        .map_err(internal)?
        .items)
}
fn record(s: &AppState, event: &str, id: &str) -> ApiResult<EventRecord> {
    records(s, event)?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or((StatusCode::NOT_FOUND, "event record not found".into()))
}
fn preview_path(s: &AppState, event: &str, r: &EventRecord) -> ApiResult<PathBuf> {
    let prefix = format!("/gaia/public/events/{event}/previews/");
    let name = r
        .preview_url
        .strip_prefix(&prefix)
        .filter(|v| !v.is_empty() && !v.contains('/') && !v.contains(".."))
        .ok_or((
            StatusCode::BAD_REQUEST,
            "event preview path is invalid".into(),
        ))?;
    Ok(s.archive_root
        .join("public/events")
        .join(event)
        .join("previews")
        .join(name))
}
fn content_type(path: &FsPath) -> &'static str {
    match path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
}
fn parse_settings(
    crop: Option<String>,
    mask: Option<String>,
    enabled: bool,
) -> anyhow::Result<([f64; 4], Vec<Vec<[f64; 2]>>, Value, Value)> {
    let crop_value = crop
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or(json!({"left":0.,"top":0.,"right":1.,"bottom":1.}));
    let rect = [
        crop_value["left"].as_f64().unwrap_or(0.),
        crop_value["top"].as_f64().unwrap_or(0.),
        crop_value["right"].as_f64().unwrap_or(1.),
        crop_value["bottom"].as_f64().unwrap_or(1.),
    ];
    anyhow::ensure!(
        rect.iter().all(|v| v.is_finite())
            && rect[0] >= 0.
            && rect[1] >= 0.
            && rect[2] <= 1.
            && rect[3] <= 1.
            && rect[0] < rect[2]
            && rect[1] < rect[3],
        "invalid crop rectangle"
    );
    let mask_value = mask
        .as_deref()
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or(json!({"coordinate_system":"normalized_image","polygons":[]}));
    anyhow::ensure!(
        mask_value["coordinate_system"] == "normalized_image",
        "unsupported mask coordinate system"
    );
    let polygons: Vec<Vec<[f64; 2]>> = if enabled {
        serde_json::from_value(mask_value["polygons"].clone())?
    } else {
        vec![]
    };
    anyhow::ensure!(
        polygons.iter().all(|p| p.len() >= 3
            && p.len() <= 10_000
            && p.iter()
                .flatten()
                .all(|v| v.is_finite() && (-0.25..=1.25).contains(v))),
        "invalid mask polygon"
    );
    Ok((rect, polygons, crop_value, mask_value))
}
fn atomic(path: &FsPath, bytes: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension(format!("pending-{}", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

/// Build or reuse the immutable texture and full magnetic-weighted 100 km mesh.
fn prepare(s: &AppState, event: &str, r: &EventRecord) -> anyhow::Result<Value> {
    let conn = db::open_readonly(&s.db_path)?;
    let (calibration_id,hdf5,crop,mask,enabled):(String,String,Option<String>,Option<String>,bool)=conn.query_row(
        "SELECT c.id,c.hdf5_path,ms.crop_json,ms.mask_json,COALESCE(ms.mask_enabled,1) FROM event_calibrations c LEFT JOIN event_media_settings ms ON ms.event_id=c.event_id AND ms.record_id=c.record_id WHERE c.event_id=?1 AND c.record_id=?2 ORDER BY (c.id=COALESCE(ms.selected_calibration_id,'')) DESC,julianday(c.created_utc) DESC LIMIT 1",
        rusqlite::params![event,r.id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    let (crop, polygons, crop_value, mask_value) = parse_settings(crop, mask, enabled)?;
    let image_path = preview_path(s, event, r).map_err(|(_, e)| anyhow::anyhow!(e))?;
    let stamp = std::fs::metadata(&hdf5)?.modified()?;
    let key = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&json!([
            "event-projection-v1",
            event,
            r.id,
            image_path,
            hdf5,
            format!("{stamp:?}"),
            r.latitude,
            r.longitude,
            crop_value,
            mask_value,
            enabled,
            s.igrf_year
        ]))?)
    );
    let dir = s.archive_root.join("event-projection-cache").join(event);
    std::fs::create_dir_all(&dir)?;
    let geometry_name = format!("{key}.bin");
    let texture_name = format!("{key}.jpg");
    let geometry_path = dir.join(&geometry_name);
    let texture_path = dir.join(&texture_name);
    if !geometry_path.exists() {
        let projected = projection::project_image(
            &r.id,
            &r.captured_at,
            &image_path,
            &hdf5,
            r.latitude,
            r.longitude,
            0.,
            crop,
            &polygons,
        )?;
        let compact = projection::compact_geometry(&projected);
        let raw = [
            projection::numeric(&hdf5, "image_width", true)?[0],
            projection::numeric(&hdf5, "image_height", true)?[0],
        ];
        let weighted = publish_layers::weight_mesh_bytes(
            s,
            &compact,
            r.latitude,
            r.longitude,
            0.,
            crop,
            &polygons,
            raw,
            &publish_layers::Rules::load()?,
        )?;
        atomic(&geometry_path, &weighted)?;
    }
    if !texture_path.exists() {
        let image = image::open(&image_path)?.thumbnail(256, 256).to_rgb8();
        let mut bytes = vec![];
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80).encode_image(&image)?;
        atomic(&texture_path, &bytes)?;
    }
    Ok(
        json!({"source_id":r.id,"at":r.captured_at,"observation_at":r.captured_at,
        "geometry_url":format!("/gaia/api/events/{event}/assets/{geometry_name}"),
        "texture_url":format!("/gaia/api/events/{event}/assets/{texture_name}"),
        "vertex_count":std::fs::metadata(&geometry_path)?.len()/24,"calibration_id":calibration_id}),
    )
}

pub async fn record_image(
    Path((event, id)): Path<(String, String)>,
    State(s): State<AppState>,
) -> ApiResult<Response<Body>> {
    let r = record(&s, &event, &id)?;
    let path = preview_path(&s, &event, &r)?;
    let bytes = tokio::fs::read(&path).await.map_err(internal)?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, content_type(&path))
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-GAIA-Observation-UTC", r.captured_at)
        .header("X-GAIA-Latitude-Deg", r.latitude.to_string())
        .header("X-GAIA-Longitude-Deg", r.longitude.to_string())
        .header("X-GAIA-Altitude-M", "0")
        .body(Body::from(bytes))
        .unwrap())
}

pub async fn settings(
    Path((event, id)): Path<(String, String)>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    record(&s, &event, &id)?;
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
    let row:(Option<String>,Option<String>,bool,Option<String>)=conn.query_row("SELECT crop_json,mask_json,COALESCE(mask_enabled,1),selected_calibration_id FROM event_media_settings WHERE event_id=?1 AND record_id=?2",rusqlite::params![event,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap_or((None,None,true,None));
    let (_, _, crop, mask) = parse_settings(row.0, row.1, row.2).map_err(internal)?;
    Ok(Json(
        json!({"crop":crop,"mask":mask,"mask_enabled":row.2,"calibrated":row.3.is_some(),"selected_calibration_id":row.3}),
    ))
}

#[derive(Deserialize)]
pub struct SettingsInput {
    crop: Option<Value>,
    mask: Option<Value>,
    mask_enabled: Option<bool>,
}
pub async fn save_settings(
    Path((event, id)): Path<(String, String)>,
    State(s): State<AppState>,
    Json(input): Json<SettingsInput>,
) -> ApiResult<Json<Value>> {
    let r = record(&s, &event, &id)?;
    for v in [&input.crop, &input.mask] {
        if v.as_ref().is_some_and(|x| x.to_string().len() > 100_000) {
            return Err((StatusCode::BAD_REQUEST, "crop or mask is too large".into()));
        }
    }
    let conn = db::open(&s.db_path).map_err(internal)?;
    let stored:(Option<String>,Option<String>,bool)=conn.query_row("SELECT crop_json,mask_json,COALESCE(mask_enabled,1) FROM event_media_settings WHERE event_id=?1 AND record_id=?2",rusqlite::params![event,id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap_or((None,None,true));
    let crop = input.crop.map(|v| v.to_string()).or(stored.0);
    let mask = input.mask.map(|v| v.to_string()).or(stored.1);
    let enabled = input.mask_enabled.unwrap_or(stored.2);
    parse_settings(crop.clone(), mask.clone(), enabled)
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    conn.execute("INSERT INTO event_media_settings(event_id,record_id,updated_utc,crop_json,mask_json,mask_enabled) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(event_id,record_id) DO UPDATE SET updated_utc=excluded.updated_utc,crop_json=excluded.crop_json,mask_json=excluded.mask_json,mask_enabled=excluded.mask_enabled",rusqlite::params![event,id,Utc::now().to_rfc3339(),crop,mask,enabled]).map_err(internal)?;
    let projection = prepare(&s, &event, &r).ok();
    Ok(Json(
        json!({"state":"saved","projected":projection.is_some()}),
    ))
}

pub async fn calibration(
    Path(event): Path<String>,
    State(s): State<AppState>,
    mut mp: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    if !valid_event(&event) {
        return Err((StatusCode::BAD_REQUEST, "invalid event id".into()));
    }
    let (mut record_id, mut residual_px, mut submitter, mut star_count, mut bytes) =
        (None, None, None, None, None);
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "record_id" => record_id = Some(field.text().await.map_err(internal)?),
            "residual_px" => {
                residual_px = field.text().await.ok().and_then(|v| v.parse::<f64>().ok())
            }
            "submitted_by" => submitter = Some(field.text().await.map_err(internal)?),
            "star_count" => {
                star_count = field.text().await.ok().and_then(|v| v.parse::<i64>().ok())
            }
            "calibration" => bytes = Some(field.bytes().await.map_err(internal)?.to_vec()),
            _ => {}
        }
    }
    let record_id = record_id.ok_or((StatusCode::BAD_REQUEST, "record_id is required".into()))?;
    let r = record(&s, &event, &record_id)?;
    let data = bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "calibration HDF5 is required".into(),
    ))?;
    if data.len() < 8 || &data[..8] != b"\x89HDF\r\n\x1a\n" {
        return Err((
            StatusCode::BAD_REQUEST,
            "calibration is not an HDF5 file".into(),
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let dir = s.archive_root.join("event-calibrations").join(&event);
    std::fs::create_dir_all(&dir).map_err(internal)?;
    let path = dir.join(format!("{id}.h5"));
    atomic(&path, &data).map_err(internal)?;
    let stars = star_count.or_else(|| {
        projection::dataset_rows(&path.to_string_lossy(), "/selected_stars")
            .ok()
            .map(|v| v as i64)
    });
    let conn = db::open(&s.db_path).map_err(internal)?;
    let previous: Option<String> = conn
        .query_row(
            "SELECT selected_calibration_id FROM event_media_settings WHERE event_id=?1 AND record_id=?2",
            rusqlite::params![event, record_id],
            |row| row.get(0),
        )
        .unwrap_or(None);
    let tx = conn.unchecked_transaction().map_err(internal)?;
    tx.execute("INSERT INTO event_calibrations(id,event_id,record_id,created_utc,hdf5_path,residual_px,submitted_by,star_count) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",rusqlite::params![id,event,record_id,Utc::now().to_rfc3339(),path.to_string_lossy(),residual_px,submitter,stars]).map_err(internal)?;
    tx.execute("INSERT INTO event_media_settings(event_id,record_id,updated_utc,selected_calibration_id) VALUES(?1,?2,?3,?4) ON CONFLICT(event_id,record_id) DO UPDATE SET updated_utc=excluded.updated_utc,selected_calibration_id=excluded.selected_calibration_id",rusqlite::params![event,record_id,Utc::now().to_rfc3339(),id]).map_err(internal)?;
    tx.commit().map_err(internal)?;
    if let Err(error) = prepare(&s, &event, &r) {
        // Saving an event calibration promises a usable 100 km projection.
        // Restore the previous selection if an incomplete HDF5 slipped past
        // the signature check, rather than hiding a model that already worked.
        conn.execute("DELETE FROM event_calibrations WHERE id=?1", [&id])
            .map_err(internal)?;
        conn.execute(
            "UPDATE event_media_settings SET selected_calibration_id=?3,updated_utc=?4 WHERE event_id=?1 AND record_id=?2",
            rusqlite::params![event, record_id, previous, Utc::now().to_rfc3339()],
        )
        .map_err(internal)?;
        let _ = std::fs::remove_file(&path);
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("calibration could not be projected: {error}"),
        ));
    }
    Ok((
        StatusCode::CREATED,
        Json(
            json!({"id":id,"state":"calibrated","event_id":event,"record_id":record_id,"star_count":stars,"residual_px":residual_px,"projection_error":Value::Null}),
        ),
    ))
}

pub async fn projection_manifest(
    Path(event): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let all = records(&s, &event)?;
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
    let mut cameras = vec![];
    for r in all {
        let calibrated:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM event_calibrations WHERE event_id=?1 AND record_id=?2)",rusqlite::params![event,r.id],|row|row.get(0)).unwrap_or(false);
        if !calibrated {
            continue;
        }
        let Ok(frame) = prepare(&s, &event, &r) else {
            continue;
        };
        let index = cameras.len() + 1;
        let rights = r
            .license
            .clone()
            .or(r.rights_note.clone())
            .unwrap_or_default();
        cameras.push(json!({"source_id":r.id,"name":r.title.clone().unwrap_or_else(||r.location.clone()),"producer":r.creator.clone().unwrap_or_else(||r.source.clone()),"institution":r.source,"website_url":r.source_url,"latitude_deg":r.latitude,"longitude_deg":r.longitude,"altitude_m":0,"acknowledgement":rights.clone(),"copyright":rights,"calibrated":true,"map_index":index,"quality_exponent":0,"kind":r.kind,"projection":{"stride":24,"images":[frame]}}));
    }
    let rules = publish_layers::Rules::load().map_err(internal)?;
    Ok(Json(
        json!({"schema":"gaia-event-projections-v1","composition":"browser-layers-v1","event_id":event,"generated_utc":Utc::now().to_rfc3339(),"stitching":{"rules":rules},"cameras":cameras}),
    ))
}

pub async fn projection_asset(
    Path((event, name)): Path<(String, String)>,
    State(s): State<AppState>,
) -> ApiResult<Response<Body>> {
    if !valid_event(&event) || !valid_asset(&name) {
        return Err((StatusCode::BAD_REQUEST, "invalid event asset".into()));
    }
    let path = s
        .archive_root
        .join("event-projection-cache")
        .join(event)
        .join(&name);
    let bytes = tokio::fs::read(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            (StatusCode::NOT_FOUND, "event asset not found".into())
        } else {
            internal(e)
        }
    })?;
    Ok(Response::builder()
        .header(
            header::CONTENT_TYPE,
            if name.ends_with(".bin") {
                "application/octet-stream"
            } else {
                "image/jpeg"
            },
        )
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(bytes))
        .unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_ids_are_narrow() {
        assert!(valid_event("20251111"));
        assert!(!valid_event("../events"));
        assert!(!valid_event("2025-111"));
        assert!(valid_asset(&format!("{}.bin", "a".repeat(64))));
        assert!(valid_asset(&format!("{}.jpg", "0".repeat(64))));
        assert!(!valid_asset("../secret.bin"));
    }

    #[test]
    fn event_crop_and_masks_are_normalized_and_bounded() {
        let crop = Some(r#"{"left":0.1,"top":0.2,"right":0.9,"bottom":0.8}"#.into());
        let mask = Some(
            r#"{"coordinate_system":"normalized_image","polygons":[[[0.1,0.1],[0.2,0.1],[0.2,0.2]]]}"#
                .into(),
        );
        let (rect, polygons, _, _) = parse_settings(crop, mask, true).unwrap();
        assert_eq!(rect, [0.1, 0.2, 0.9, 0.8]);
        assert_eq!(polygons.len(), 1);
        assert!(parse_settings(Some(r#"{"left":0.9,"right":0.1}"#.into()), None, true).is_err());
        assert!(parse_settings(
            None,
            Some(r#"{"coordinate_system":"pixels","polygons":[]}"#.into()),
            true
        )
        .is_err());
    }

    #[test]
    fn disabling_a_mask_keeps_its_saved_shape_but_projects_none_of_it() {
        let mask = Some(
            r#"{"coordinate_system":"normalized_image","polygons":[[[0.1,0.1],[0.2,0.1],[0.2,0.2]]]}"#
                .into(),
        );
        let (_, polygons, _, saved) = parse_settings(None, mask, false).unwrap();
        assert!(polygons.is_empty());
        assert_eq!(saved["polygons"].as_array().unwrap().len(), 1);
    }
}
