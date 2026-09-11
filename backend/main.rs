mod archive;
mod crawler;
mod db;
mod equalize;
mod geometry;
mod igrf_grid;
mod image_time;
mod meteor_backfill;
mod model;
mod norsk_meteor;
mod pixel_mask;
mod projection;
mod publish;
mod quality;
mod starpass;
mod starphot;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use model::{PipelineStage, SourceStatus, SystemStatus};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tower_http::{services::ServeDir, trace::TraceLayer};

#[derive(Clone)]
struct AppState {
    db_path: PathBuf,
    archive_root: PathBuf,
    sources: Arc<Vec<model::SourceConfig>>,
    igrf: Arc<Vec<u8>>,
    igrf_year: i32,
}
type ApiResult<T> = Result<T, (StatusCode, String)>;
fn internal(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

async fn health() -> Json<Value> {
    Json(json!({"service":"gaia","state":"ok","backend":"rust"}))
}
async fn igrf_maglat(State(s): State<AppState>) -> Response<Body> {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header("X-GAIA-IGRF-Model", "IGRF-14 full degree 13")
        .header("X-GAIA-IGRF-Year", s.igrf_year)
        .header(
            "X-GAIA-Grid-Size",
            format!("{}x{}", igrf_grid::WIDTH, igrf_grid::HEIGHT),
        )
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .body(Body::from(s.igrf.as_ref().clone()))
        .unwrap()
}
async fn status(State(s): State<AppState>) -> ApiResult<Json<SystemStatus>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let active: i64 = conn
        .query_row("SELECT count(*) FROM sources WHERE enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=sources.id)", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM images WHERE julianday(downloaded_utc) >= julianday('now','-1 day')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let latest: Option<String> = conn
        .query_row("SELECT max(observation_utc) FROM images WHERE julianday(observation_utc)<=julianday('now','+2 minutes')", [], |r| r.get(0))
        .unwrap_or(None);
    let published=std::fs::read(s.archive_root.join("publication-status.json")).ok()
        .and_then(|bytes|serde_json::from_slice::<Value>(&bytes).ok());
    let mosaic=published.as_ref().and_then(|v|v["latest_observation_utc"].as_str()).map(str::to_owned);
    let publication_fresh=mosaic.as_deref().and_then(|t|DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t|Utc::now().signed_duration_since(t).num_minutes()<10);
    Ok(Json(SystemStatus {
        service: "GAIA Data Center",
        now_utc: Utc::now().to_rfc3339(),
        emission_altitude_km: 100.0,
        active_sources: active,
        images_24h: count,
        latest_observation_utc: latest,
        latest_mosaic_utc: mosaic,
        pipeline: vec![
            PipelineStage {
                name: "Acquire",
                state: "running",
                detail: "Archive and snapshot crawlers".into(),
            },
            PipelineStage {
                name: "Calibrate",
                state: "waiting",
                detail: "AIDA/WISC HDF5 calibrations".into(),
            },
            PipelineStage {
                name: "Mask",
                state: "ready",
                detail: "Cloud and obstruction probability".into(),
            },
            PipelineStage {
                name: "Project",
                state: "ready",
                detail: "100 km shell intersection".into(),
            },
            PipelineStage {
                name: "Tessellate",
                state: "ready",
                detail: "Magnetic-zenith weighted mosaic".into(),
            },
            PipelineStage {
                name: "Publish",
                state: if publication_fresh {"ready"} else {"delayed"},
                detail: published.as_ref().and_then(|v|v["latest_observation_utc"].as_str())
                    .map(|t|format!("Verified on juha.no through {t}"))
                    .unwrap_or_else(||"Waiting for a verified public delivery".into()),
            },
        ],
    }))
}

async fn credits(State(s): State<AppState>) -> ApiResult<Json<Vec<Value>>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut query = conn
        .prepare("SELECT name,website,acknowledgement,copyright FROM producers ORDER BY name")
        .map_err(internal)?;
    let rows=query.query_map([],|r|Ok(json!({"name":r.get::<_,String>(0)?,"website":r.get::<_,Option<String>>(1)?,"acknowledgement":r.get::<_,String>(2)?,"copyright":r.get::<_,String>(3)?}))).map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}

async fn sources(State(s): State<AppState>) -> ApiResult<Json<Vec<SourceStatus>>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut q=conn.prepare("SELECT s.id,s.name,p.name,s.timestamp_mode,s.last_success_utc,s.last_error,(SELECT max(observation_utc) FROM images i WHERE i.source_id=s.id),(SELECT max(downloaded_utc) FROM images i WHERE i.source_id=s.id),(SELECT count(*) FROM images i WHERE i.source_id=s.id AND i.downloaded_utc >= datetime('now','-1 day')),s.latitude_deg,s.longitude_deg,EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id),s.enabled,COALESCE(cs.quality_exponent,0) FROM sources s JOIN producers p ON p.id=s.producer_id LEFT JOIN camera_settings cs ON cs.source_id=s.id WHERE NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY s.name").map_err(internal)?;
    let rows = q
        .query_map([], |r| {
            let last: Option<String> = r.get(4)?;
            let state = if r.get::<_, Option<String>>(5)?.is_some() {
                "error"
            } else if last.is_some() {
                "live"
            } else {
                "waiting"
            };
            Ok(SourceStatus {
                id: r.get(0)?,
                name: r.get(1)?,
                producer: r.get(2)?,
                latitude_deg: r.get(9)?,
                longitude_deg: r.get(10)?,
                calibrated: r.get(11)?,
                enabled: r.get(12)?,
                quality_exponent: r.get(13)?,
                state: state.into(),
                timestamp_mode: r.get(3)?,
                last_observation_utc: r.get(6)?,
                last_download_utc: r.get(7)?,
                latency_seconds: None,
                images_24h: r.get(8)?,
                clear_fraction: None,
                message: r.get(5)?,
            })
        })
        .map_err(internal)?;
    // Propagated, not skipped: a row that fails to map is a bug in this query,
    // and silently dropping it presents an empty camera registry as success.
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}

async fn history(State(s): State<AppState>) -> ApiResult<Json<Vec<String>>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut q=conn.prepare("SELECT DISTINCT strftime('%Y-%m-%dT%H:%M:00Z',i.observation_utc) AS minute FROM images i JOIN sources s ON s.id=i.source_id WHERE s.enabled=1 AND EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) AND julianday(i.observation_utc)>=julianday('now','-1 day') ORDER BY minute").map_err(internal)?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}

async fn source_frames(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Vec<Value>>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut query = conn
        .prepare("SELECT id,observation_utc,width,height FROM images WHERE source_id=?1 AND julianday(observation_utc)>=julianday('now','-1 day') ORDER BY observation_utc")
        .map_err(internal)?;
    let rows = query
        .query_map([id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "observation_utc": row.get::<_, String>(1)?,
                "width": row.get::<_, Option<i64>>(2)?,
                "height": row.get::<_, Option<i64>>(3)?,
            }))
        })
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
}

#[derive(Deserialize)]
struct SuggestionInput {
    name: Option<String>,
    contact: Option<String>,
    suggestion: String,
    page_url: Option<String>,
}
#[derive(Serialize)]
struct SuggestionReceipt {
    id: i64,
    state: &'static str,
}
async fn suggest(
    State(s): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(input): Json<SuggestionInput>,
) -> ApiResult<(StatusCode, Json<SuggestionReceipt>)> {
    let text = input.suggestion.trim();
    if text.len() < 4 || text.len() > 8000 {
        return Err((
            StatusCode::BAD_REQUEST,
            "suggestion must be 4-8000 characters".into(),
        ));
    }
    let conn = db::open(&s.db_path).map_err(internal)?;
    conn.execute("INSERT INTO suggestions(created_utc,name,contact,suggestion,page_url,user_agent) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![Utc::now().to_rfc3339(),input.name.map(|v|v.trim().chars().take(200).collect::<String>()),input.contact.map(|v|v.trim().chars().take(320).collect::<String>()),text,input.page_url,headers.get("user-agent").and_then(|v|v.to_str().ok())]).map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(SuggestionReceipt {
            id: conn.last_insert_rowid(),
            state: "received",
        }),
    ))
}

#[derive(Deserialize)]
struct LocationInput {
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_m: Option<f64>,
}
async fn set_location(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(input): Json<LocationInput>,
) -> ApiResult<Json<Value>> {
    if !(-90.0..=90.0).contains(&input.latitude_deg)
        || !(-180.0..=180.0).contains(&input.longitude_deg)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "invalid latitude or longitude".into(),
        ));
    }
    let conn = db::open(&s.db_path).map_err(internal)?;
    let changed=conn.execute("UPDATE sources SET latitude_deg=?1,longitude_deg=?2,altitude_m=COALESCE(?3,altitude_m) WHERE id=?4",rusqlite::params![input.latitude_deg,input.longitude_deg,input.altitude_m,id]).map_err(internal)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "camera not found".into()));
    }
    Ok(Json(
        json!({"state":"saved","latitude_deg":input.latitude_deg,"longitude_deg":input.longitude_deg}),
    ))
}

#[derive(Deserialize)]
struct EnabledInput {
    enabled: bool,
}
async fn set_enabled(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(input): Json<EnabledInput>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let changed = conn
        .execute(
            "UPDATE sources SET enabled=?1 WHERE id=?2",
            rusqlite::params![input.enabled, id],
        )
        .map_err(internal)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "camera not found".into()));
    }
    Ok(Json(json!({"state":"saved","enabled":input.enabled})))
}

async fn remove_source(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let mut conn = db::open(&s.db_path).map_err(internal)?;
    let transaction = conn.transaction().map_err(internal)?;
    let changed = transaction
        .execute("UPDATE sources SET enabled=0 WHERE id=?1", [&id])
        .map_err(internal)?;
    if changed == 0 {
        return Err((StatusCode::NOT_FOUND, "camera not found".into()));
    }
    transaction.execute("INSERT INTO removed_sources(source_id,removed_utc) VALUES(?1,?2) ON CONFLICT(source_id) DO UPDATE SET removed_utc=excluded.removed_utc",rusqlite::params![id,Utc::now().to_rfc3339()]).map_err(internal)?;
    transaction.commit().map_err(internal)?;
    Ok(Json(json!({"state":"removed","archive_preserved":true})))
}

#[derive(Deserialize)]
struct CameraSettingsInput {
    crop: Option<Value>,
    mask: Option<Value>,
    /// Omitted leaves the camera's current choice untouched.
    mask_enabled: Option<bool>,
    /// Quality weight as a power of two, 0 to -8. Omitted leaves it untouched.
    quality_exponent: Option<i64>,
}
async fn get_camera_settings(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let row = conn.query_row(
        "SELECT c.crop_json,c.mask_json,COALESCE(c.mask_enabled,1),COALESCE(c.quality_exponent,0) FROM sources s LEFT JOIN camera_settings c ON c.source_id=s.id WHERE s.id=?1",
        [&id],
        |r| Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,bool>(2)?,r.get::<_,i64>(3)?)),
    ).map_err(|e| if matches!(e,rusqlite::Error::QueryReturnedNoRows) {
        (StatusCode::NOT_FOUND,"camera not found".into())
    } else { internal(e) })?;
    let parse = |text: Option<String>| -> ApiResult<Value> {
        text.map(|t| serde_json::from_str(&t).map_err(internal))
            .unwrap_or(Ok(Value::Null))
    };
    Ok(Json(
        json!({"crop":parse(row.0)?,"mask":parse(row.1)?,"mask_enabled":row.2,"quality_exponent":row.3}),
    ))
}
async fn camera_settings(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(input): Json<CameraSettingsInput>,
) -> ApiResult<Json<Value>> {
    let valid = |value: &Option<Value>| {
        value
            .as_ref()
            .is_none_or(|v| serde_json::to_string(v).is_ok_and(|text| text.len() <= 100_000))
    };
    if !valid(&input.crop) || !valid(&input.mask) {
        return Err((StatusCode::BAD_REQUEST, "crop or mask is too large".into()));
    }
    let conn = db::open(&s.db_path).map_err(internal)?;
    let exists: i64 = conn
        .query_row("SELECT count(*) FROM sources WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .map_err(internal)?;
    if exists == 0 {
        return Err((StatusCode::NOT_FOUND, "camera not found".into()));
    }
    // The column is NOT NULL, so resolve an omitted flag to the stored choice
    // rather than binding NULL and resetting cameras that predate the switch.
    let mask_enabled = match input.mask_enabled {
        Some(value) => value,
        None => conn
            .query_row(
                "SELECT COALESCE(mask_enabled,1) FROM camera_settings WHERE source_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(true),
    };
    // Crop and mask are likewise resolved to the stored values when omitted.
    // Without this a request that sets only the quality weight would write NULL
    // over an operator's crop rectangle and obstruction outlines.
    let stored: (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT crop_json,mask_json FROM camera_settings WHERE source_id=?1",
            [&id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((None, None));
    let crop_json = match input.crop {
        Some(v) => Some(v.to_string()),
        None => stored.0,
    };
    let mask_json = match input.mask {
        Some(v) => Some(v.to_string()),
        None => stored.1,
    };
    // Clamped to the offered range so a malformed request cannot silently
    // erase a camera from the mosaic.
    let quality_exponent = match input.quality_exponent {
        Some(value) => value.clamp(-8, 0),
        None => conn
            .query_row(
                "SELECT COALESCE(quality_exponent,0) FROM camera_settings WHERE source_id=?1",
                [&id],
                |r| r.get(0),
            )
            .unwrap_or(0),
    };
    conn.execute("INSERT INTO camera_settings(source_id,updated_utc,crop_json,mask_json,mask_enabled,quality_exponent) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(source_id) DO UPDATE SET updated_utc=excluded.updated_utc,crop_json=excluded.crop_json,mask_json=excluded.mask_json,mask_enabled=excluded.mask_enabled,quality_exponent=excluded.quality_exponent",rusqlite::params![id,Utc::now().to_rfc3339(),crop_json,mask_json,mask_enabled,quality_exponent]).map_err(internal)?;
    Ok(Json(json!({"state":"saved"})))
}

#[derive(Deserialize)]
struct ProjectionQuery {
    at: Option<String>,
    format: Option<String>,
}
async fn projected_image(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ProjectionQuery>,
    headers: axum::http::HeaderMap,
) -> ApiResult<axum::response::Response> {
    let at = query
        .at
        .map(|t| {
            DateTime::parse_from_rfc3339(&t)
                .map(|v| v.with_timezone(&Utc))
                .map_err(|_| (StatusCode::BAD_REQUEST, "invalid frame time".into()))
        })
        .transpose()?;
    if matches!(query.format.as_deref(), Some("assets" | "timeline")) {
        let timeline = query.format.as_deref() == Some("timeline");
        let p = tokio::task::spawn_blocking(move || {
            if timeline {
                projection::timeline(&s, &id)
            } else {
                projection::assets(&s, &id, at)
            }
        })
        .await
        .map_err(internal)?
        .map_err(|e| {
            if e.to_string().contains("Query returned no rows") {
                (StatusCode::NOT_FOUND, "no historical frame".into())
            } else {
                internal(e)
            }
        })?;
        return Ok(([(header::CACHE_CONTROL, "no-store")], Json(p)).into_response());
    }
    let p = tokio::task::spawn_blocking(move || projection::build(&s, &id, at))
        .await
        .map_err(internal)?
        .map_err(|e| {
            if e.to_string().contains("Query returned no rows") {
                (
                    StatusCode::NOT_FOUND,
                    "no frame within ten minutes before selected time".into(),
                )
            } else {
                internal(e)
            }
        })?;
    let mut response = if headers.get(header::ACCEPT).and_then(|v| v.to_str().ok())
        == Some("application/octet-stream")
    {
        let mut bytes = Vec::with_capacity(p.vertices.len() * 4);
        for value in &p.vertices {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
    } else {
        Json(p).into_response()
    };
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}
async fn projection_asset(
    Path(name): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<axum::response::Response> {
    let Some((key, ext)) = name.rsplit_once('.') else {
        return Err((StatusCode::BAD_REQUEST, "invalid asset".into()));
    };
    if key.len() != 64
        || !key.bytes().all(|b| b.is_ascii_hexdigit())
        || !matches!(ext, "bin" | "png")
    {
        return Err((StatusCode::BAD_REQUEST, "invalid asset".into()));
    }
    let bytes = tokio::fs::read(s.archive_root.join("projection-cache").join(&name))
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "asset not ready".into()))?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                if ext == "png" {
                    "image/png"
                } else {
                    "application/octet-stream"
                },
            ),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}
async fn image_texture(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<axum::response::Response> {
    let bytes = tokio::task::spawn_blocking(move || projection::image_texture(&s, &id))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "public, max-age=31536000, immutable"),
        ],
        bytes,
    )
        .into_response())
}
async fn latest_image(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Response<Body>> {
    let row = {
        let conn = db::open(&s.db_path).map_err(internal)?;
        conn.query_row("SELECT i.archive_path,i.media_type,p.name,p.copyright,i.observation_utc,s.latitude_deg,s.longitude_deg,s.altitude_m,s.name FROM images i JOIN sources s ON s.id=i.source_id JOIN producers p ON p.id=s.producer_id WHERE i.source_id=?1 ORDER BY i.observation_utc DESC LIMIT 1",[&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<f64>>(5)?,r.get::<_,Option<f64>>(6)?,r.get::<_,Option<f64>>(7)?,r.get::<_,String>(8)?))).map_err(|e|if matches!(e,rusqlite::Error::QueryReturnedNoRows){(StatusCode::NOT_FOUND,"no image has been acquired for this camera yet".into())}else{internal(e)})?
    };
    let bytes = tokio::fs::read(&row.0).await.map_err(internal)?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, row.1)
        .header(header::CACHE_CONTROL, "no-cache")
        .header("X-GAIA-Producer", row.2)
        .header("X-GAIA-Copyright", row.3)
        .header("X-GAIA-Observation-UTC", row.4)
        .header(
            "X-GAIA-Latitude-Deg",
            row.5.map(|v| v.to_string()).unwrap_or_default(),
        )
        .header(
            "X-GAIA-Longitude-Deg",
            row.6.map(|v| v.to_string()).unwrap_or_default(),
        )
        .header(
            "X-GAIA-Altitude-M",
            row.7.map(|v| v.to_string()).unwrap_or_default(),
        )
        .header("X-GAIA-Source-Name", row.8)
        .body(Body::from(bytes))
        .unwrap())
}

async fn original_image(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Response<Body>> {
    let row = {
        let conn = db::open(&s.db_path).map_err(internal)?;
        conn.query_row("SELECT i.archive_path,i.media_type,p.name,p.copyright,i.observation_utc,s.latitude_deg,s.longitude_deg,s.altitude_m,s.name,s.id FROM images i JOIN sources s ON s.id=i.source_id JOIN producers p ON p.id=s.producer_id WHERE i.id=?1",[&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<f64>>(5)?,r.get::<_,Option<f64>>(6)?,r.get::<_,Option<f64>>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?))).map_err(|e|if matches!(e,rusqlite::Error::QueryReturnedNoRows){(StatusCode::NOT_FOUND,"image not found".into())}else{internal(e)})?
    };
    let bytes = tokio::fs::read(&row.0).await.map_err(internal)?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, row.1)
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header("X-GAIA-Producer", row.2)
        .header("X-GAIA-Copyright", row.3)
        .header("X-GAIA-Observation-UTC", row.4)
        .header(
            "X-GAIA-Latitude-Deg",
            row.5.map(|v| v.to_string()).unwrap_or_default(),
        )
        .header(
            "X-GAIA-Longitude-Deg",
            row.6.map(|v| v.to_string()).unwrap_or_default(),
        )
        .header(
            "X-GAIA-Altitude-M",
            row.7.map(|v| v.to_string()).unwrap_or_default(),
        )
        .header("X-GAIA-Source-Name", row.8)
        .header("X-GAIA-Source-ID", row.9)
        .body(Body::from(bytes))
        .unwrap())
}

async fn calibration(
    State(s): State<AppState>,
    mut mp: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let mut source_id = None;
    let mut valid_from = None;
    let mut residual_px = None;
    let mut submitter = None;
    let mut star_count = None;
    let mut bytes = None;
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "source_id" => source_id = Some(field.text().await.map_err(internal)?),
            "valid_from_utc" => valid_from = Some(field.text().await.map_err(internal)?),
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
    let source = source_id.ok_or((StatusCode::BAD_REQUEST, "source_id is required".into()))?;
    let data = bytes.ok_or((
        StatusCode::BAD_REQUEST,
        "calibration HDF5 is required".into(),
    ))?;
    if data.len() < 8 || &data[0..8] != b"\x89HDF\r\n\x1a\n" {
        return Err((
            StatusCode::BAD_REQUEST,
            "calibration is not an HDF5 file".into(),
        ));
    }
    let conn = db::open(&s.db_path).map_err(internal)?;
    let exists: i64 = conn
        .query_row("SELECT count(*) FROM sources WHERE id=?1", [&source], |r| {
            r.get(0)
        })
        .map_err(internal)?;
    if exists == 0 {
        return Err((StatusCode::NOT_FOUND, "camera not found".into()));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let dir = s.archive_root.join("calibrations").join(&source);
    std::fs::create_dir_all(&dir).map_err(internal)?;
    let path = dir.join(format!("{id}.h5"));
    let tmp = dir.join(format!(".{id}.tmp"));
    std::fs::write(&tmp, &data).map_err(internal)?;
    std::fs::rename(&tmp, &path).map_err(internal)?;
    // The uploader may state the star count; otherwise take it from the fitted set
    // in the file itself. A new calibration is always an extra row, so the previous
    // optical parameters, residual and star count are all retained.
    let stars = star_count.or_else(|| {
        projection::dataset_rows(&path.to_string_lossy(), "/selected_stars")
            .ok()
            .map(|n| n as i64)
    });
    // Activate this newly fitted model, retaining all older models for rollback.
    // Explicit selection uses the stationary camera model for retained frames too.
    let tx = conn.unchecked_transaction().map_err(internal)?;
    tx.execute("INSERT INTO calibrations(id,source_id,created_utc,valid_from_utc,method,hdf5_path,residual_px,submitted_by,star_count) VALUES(?1,?2,?3,?4,'AIDA/WISC',?5,?6,?7,?8)",rusqlite::params![id,source,Utc::now().to_rfc3339(),valid_from,path.to_string_lossy(),residual_px,submitter,stars]).map_err(internal)?;
    tx.execute("INSERT INTO camera_settings(source_id,updated_utc,selected_calibration_id) VALUES(?1,?2,?3) ON CONFLICT(source_id) DO UPDATE SET updated_utc=excluded.updated_utc,selected_calibration_id=excluded.selected_calibration_id",rusqlite::params![source,Utc::now().to_rfc3339(),id]).map_err(internal)?;
    tx.commit().map_err(internal)?;

    Ok((
        StatusCode::CREATED,
        Json(json!({"id":id,"state":"calibrated","source_id":source,"star_count":stars,"residual_px":residual_px})),
    ))
}

/// Every calibration held for one camera, newest first, with the star count and
/// residual of each fit so an operator can compare them before switching.
async fn source_calibrations(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let selected: Option<String> = conn
        .query_row(
            "SELECT selected_calibration_id FROM camera_settings WHERE source_id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap_or(None);
    let rows: Vec<(String, String, Option<String>, Option<String>, String, String, Option<f64>, Option<i64>)> = conn
        .prepare("SELECT id,created_utc,valid_from_utc,valid_to_utc,method,hdf5_path,residual_px,star_count FROM calibrations WHERE source_id=?1 ORDER BY julianday(created_utc) DESC")
        .map_err(internal)?
        .query_map([&id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
        })
        .map_err(internal)?
        .filter_map(Result::ok)
        .collect();
    let mut out = Vec::new();
    for (cid, created, from, to, method, path, residual, stars) in rows {
        // Calibrations archived before the star count was recorded still have the
        // fitted set in their HDF5, so fill it in once and keep it.
        let stars = match stars {
            Some(n) => Some(n),
            None => {
                let found = projection::dataset_rows(&path, "/selected_stars")
                    .ok()
                    .map(|n| n as i64);
                if let Some(n) = found {
                    let _ = conn.execute(
                        "UPDATE calibrations SET star_count=?1 WHERE id=?2",
                        rusqlite::params![n, cid],
                    );
                }
                found
            }
        };
        out.push(json!({
            "id": cid, "created_utc": created, "valid_from_utc": from, "valid_to_utc": to,
            "method": method, "residual_px": residual, "star_count": stars,
            "selected": selected.as_deref() == Some(cid.as_str()),
        }));
    }
    Ok(Json(
        json!({"source_id":id,"selected_calibration_id":selected,"calibrations":out}),
    ))
}

#[derive(serde::Deserialize)]
struct SelectedCalibrationInput {
    /// None restores automatic selection by validity window.
    calibration_id: Option<String>,
}

/// Chooses which calibration maps this camera. The projection cache key contains
/// the calibration's file path, so switching rebuilds the mesh on its own.
async fn set_selected_calibration(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Json(input): Json<SelectedCalibrationInput>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    if let Some(chosen) = &input.calibration_id {
        let owned: i64 = conn
            .query_row(
                "SELECT count(*) FROM calibrations WHERE id=?1 AND source_id=?2",
                rusqlite::params![chosen, id],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if owned == 0 {
            return Err((
                StatusCode::NOT_FOUND,
                "no such calibration for this camera".into(),
            ));
        }
    }
    conn.execute(
        "INSERT INTO camera_settings(source_id,updated_utc,selected_calibration_id) VALUES(?1,?2,?3)
         ON CONFLICT(source_id) DO UPDATE SET updated_utc=excluded.updated_utc,selected_calibration_id=excluded.selected_calibration_id",
        rusqlite::params![id, Utc::now().to_rfc3339(), input.calibration_id],
    )
    .map_err(internal)?;
    Ok(Json(
        json!({"source_id":id,"selected_calibration_id":input.calibration_id}),
    ))
}

#[derive(Deserialize)]
struct StarsQuery {
    channel: Option<String>,
    hours: Option<f64>,
}

#[derive(Deserialize)]
struct StarSeriesQuery {
    star: String,
    channel: Option<String>,
    hours: Option<f64>,
}

/// Every star measured for one camera in a window, with its median image
/// position and how much its brightness varied. This is one request because the
/// scatter plot needs position and variation together.
async fn source_stars(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<StarsQuery>,
) -> ApiResult<Json<Value>> {
    let channel = query.channel.unwrap_or_else(|| "mean".into());
    if !starphot::CHANNELS.contains(&channel.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown channel".into()));
    }
    let hours = query.hours.unwrap_or(24.0).clamp(0.1, 24.0 * 14.0);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut statement = conn
        .prepare(
            "SELECT star_key,vt_mag,ra_hours_j2000,dec_deg_j2000,predicted_x,predicted_y,
                    elevation_deg,flux,background,residual_std,flux_snr
             FROM star_photometry
             WHERE source_id=?1 AND channel=?2
               AND julianday(observation_utc) >= julianday('now', ?3)
             ORDER BY star_key,observation_utc",
        )
        .map_err(internal)?;
    let rows = statement
        .query_map(
            rusqlite::params![id, channel, format!("-{hours} hours")],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, f64>(4)?,
                    r.get::<_, f64>(5)?,
                    r.get::<_, f64>(6)?,
                    r.get::<_, Option<f64>>(7)?,
                    r.get::<_, Option<f64>>(8)?,
                    r.get::<_, Option<f64>>(9)?,
                    r.get::<_, Option<f64>>(10)?,
                ))
            },
        )
        .map_err(internal)?;
    #[derive(Default)]
    struct Aggregate {
        vt_mag: f64,
        ra: f64,
        dec: f64,
        x: Vec<f64>,
        y: Vec<f64>,
        elevation: Vec<f64>,
        flux: Vec<f64>,
        background: Vec<f64>,
        frames: usize,
        found: usize,
        detected: usize,
        noise: Vec<f64>,
        snr: Vec<f64>,
    }
    let mut by_star: std::collections::BTreeMap<String, Aggregate> =
        std::collections::BTreeMap::new();
    for row in rows.filter_map(Result::ok) {
        let (key, mag, ra, dec, x, y, elevation, flux, background, noise, snr) = row;
        let entry = by_star.entry(key).or_default();
        entry.vt_mag = mag;
        entry.ra = ra;
        entry.dec = dec;
        entry.x.push(x);
        entry.y.push(y);
        entry.elevation.push(elevation);
        entry.frames += 1;
        if let Some(f) = flux {
            entry.flux.push(f);
            entry.found += 1;
        }
        if let Some(b) = background {
            entry.background.push(b);
        }
        if let Some(n) = noise {
            entry.noise.push(n);
        }
        if let Some(v) = snr {
            entry.snr.push(v);
            // A peak comparable to the frame's own scatter is not a detection.
            if v >= 5.0 {
                entry.detected += 1;
            }
        }
    }
    let median = |values: &[f64]| starphot::percentile(values, 0.5);
    let stars: Vec<Value> = by_star
        .into_iter()
        .map(|(key, a)| {
            json!({
                "star_key": key,
                "vt_mag": a.vt_mag,
                "ra_hours_j2000": a.ra,
                "dec_deg_j2000": a.dec,
                "image_x": median(&a.x),
                "image_y": median(&a.y),
                "elevation_deg": median(&a.elevation),
                "frames": a.frames,
                "found": a.found,
                "median_flux": median(&a.flux),
                "clear_flux": starphot::percentile(&a.flux, 0.9),
                "median_background": median(&a.background),
                "median_noise": median(&a.noise),
                "median_flux_snr": median(&a.snr),
                "detected": a.detected,
                "variation": starphot::brightness_variation(&a.flux),
            })
        })
        .collect();
    Ok(Json(json!({"source_id":id,"channel":channel,"hours":hours,"stars":stars})))
}

/// The brightness and background time series of one star, for plotting.
async fn source_star_series(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<StarSeriesQuery>,
) -> ApiResult<Json<Value>> {
    let star = query.star;
    let channel = query.channel.unwrap_or_else(|| "mean".into());
    if !starphot::CHANNELS.contains(&channel.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown channel".into()));
    }
    let hours = query.hours.unwrap_or(24.0).clamp(0.1, 24.0 * 14.0);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut statement = conn
        .prepare(
            "SELECT p.observation_utc,p.flux,p.background,p.amplitude,p.sigma_major,p.sigma_minor,
                    p.angle_deg,p.centroid_offset_px,p.elevation_deg,p.azimuth_deg,
                    p.predicted_x,p.predicted_y,p.centroid_x,p.centroid_y,p.rms_residual,
                    p.residual_std,p.amplitude_snr,p.flux_snr,
                    p.background_dx,p.background_dy,p.background_dxy,
                    s.moon_elevation_deg,s.moon_illuminated_fraction,s.moon_sky_brightness,
                    s.moon_apparent_magnitude,s.sun_elevation_deg
             FROM star_photometry p
             LEFT JOIN frame_sky s ON s.source_id=p.source_id AND s.image_id=p.image_id
             WHERE p.source_id=?1 AND p.star_key=?2 AND p.channel=?3
               AND julianday(p.observation_utc) >= julianday('now', ?4)
             ORDER BY p.observation_utc",
        )
        .map_err(internal)?;
    let samples: Vec<Value> = statement
        .query_map(
            rusqlite::params![id, star, channel, format!("-{hours} hours")],
            |r| {
                Ok(json!({
                    "at": r.get::<_,String>(0)?,
                    "flux": r.get::<_,Option<f64>>(1)?,
                    "background": r.get::<_,Option<f64>>(2)?,
                    "amplitude": r.get::<_,Option<f64>>(3)?,
                    "sigma_major": r.get::<_,Option<f64>>(4)?,
                    "sigma_minor": r.get::<_,Option<f64>>(5)?,
                    "angle_deg": r.get::<_,Option<f64>>(6)?,
                    "centroid_offset_px": r.get::<_,Option<f64>>(7)?,
                    "elevation_deg": r.get::<_,f64>(8)?,
                    "azimuth_deg": r.get::<_,f64>(9)?,
                    "predicted_x": r.get::<_,f64>(10)?,
                    "predicted_y": r.get::<_,f64>(11)?,
                    "centroid_x": r.get::<_,Option<f64>>(12)?,
                    "centroid_y": r.get::<_,Option<f64>>(13)?,
                    "rms_residual": r.get::<_,Option<f64>>(14)?,
                    "residual_std": r.get::<_,Option<f64>>(15)?,
                    "amplitude_snr": r.get::<_,Option<f64>>(16)?,
                    "flux_snr": r.get::<_,Option<f64>>(17)?,
                    "background_dx": r.get::<_,Option<f64>>(18)?,
                    "background_dy": r.get::<_,Option<f64>>(19)?,
                    "background_dxy": r.get::<_,Option<f64>>(20)?,
                    "moon_elevation_deg": r.get::<_,Option<f64>>(21)?,
                    "moon_illuminated_fraction": r.get::<_,Option<f64>>(22)?,
                    "moon_sky_brightness": r.get::<_,Option<f64>>(23)?,
                    "moon_apparent_magnitude": r.get::<_,Option<f64>>(24)?,
                    "sun_elevation_deg": r.get::<_,Option<f64>>(25)?,
                }))
            },
        )
        .map_err(internal)?
        .filter_map(Result::ok)
        .collect();
    let fluxes: Vec<f64> = samples
        .iter()
        .filter_map(|v| v["flux"].as_f64())
        .collect();
    Ok(Json(json!({
        "source_id": id, "star_key": star, "channel": channel, "hours": hours,
        "clear_flux": starphot::percentile(&fluxes, 0.9),
        "variation": starphot::brightness_variation(&fluxes),
        "samples": samples,
    })))
}

async fn ingest(State(s): State<AppState>, mut mp: Multipart) -> ApiResult<impl IntoResponse> {
    let mut source_id = None;
    let mut observed = None;
    let mut image = None;
    let mut media = "image/jpeg".to_string();
    while let Some(field) = mp
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "source_id" => source_id = Some(field.text().await.map_err(internal)?),
            "observation_utc" => observed = Some(field.text().await.map_err(internal)?),
            "image" => {
                media = field.content_type().unwrap_or("image/jpeg").to_string();
                image = Some(field.bytes().await.map_err(internal)?.to_vec())
            }
            _ => {}
        }
    }
    let source = source_id.ok_or((StatusCode::BAD_REQUEST, "source_id is required".into()))?;
    let bytes = image.ok_or((StatusCode::BAD_REQUEST, "image is required".into()))?;
    let now = Utc::now();
    let obs = observed
        .as_deref()
        .and_then(|v| DateTime::parse_from_rfc3339(v).ok())
        .map(|v| v.with_timezone(&Utc))
        .unwrap_or(now);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let stored = archive::store_image(
        &conn,
        &s.archive_root,
        &source,
        obs,
        now,
        if observed.is_some() {
            "submitted"
        } else {
            "download_time"
        },
        None,
        archive::validate_media_type(&media)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        &bytes,
    )
    .map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":stored.id,"duplicate":stored.duplicate,"archive_path":stored.path})),
    ))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("gaia_datacenter=info".parse()?),
        )
        .init();
    let archive_root = PathBuf::from(
        std::env::var("GAIA_ARCHIVE_ROOT").unwrap_or_else(|_| "/mnt/shovel/gaia".into()),
    );
    let db_path = PathBuf::from(std::env::var("GAIA_DB_PATH").unwrap_or_else(|_| {
        archive_root
            .join("gaia.sqlite3")
            .to_string_lossy()
            .into_owned()
    }));
    let source_path =
        PathBuf::from(std::env::var("GAIA_SOURCES").unwrap_or_else(|_| "sources/core.json".into()));
    let source_configs = Arc::new(crawler::load_sources(&source_path)?);
    let conn = db::open(&db_path)?;
    for source in source_configs.iter() {
        db::upsert_source(&conn, source)?
    }
    drop(conn);
    let igrf_year = Utc::now().format("%Y").to_string().parse::<i32>()?;
    let igrf = Arc::new(igrf_grid::load_or_generate(&archive_root, igrf_year)?);
    let state = AppState {
        db_path: db_path.clone(),
        archive_root: archive_root.clone(),
        sources: source_configs.clone(),
        igrf,
        igrf_year,
    };
    if let Some(index) = std::env::args().position(|arg| arg == "--backfill-nmn") {
        let date = std::env::args().nth(index + 1).ok_or_else(|| {
            anyhow::anyhow!("--backfill-nmn requires YYYY-MM-DD (night starting that UTC date)")
        })?;
        return meteor_backfill::run(&source_configs, &db_path, &archive_root, &date).await;
    }

    if std::env::args().any(|arg| arg == "--publish") {
        return publish::run(&state);
    }
    // Stage a migrated archive without running two collectors against providers.
    if std::env::var("GAIA_CRAWLER_ENABLED").as_deref() != Ok("0") {
        tokio::spawn(crawler::run_loop(source_configs, db_path.clone(), archive_root));
    } else {
        tracing::info!("Crawler disabled for staging/read-only operation");
    }
    // The star photometry writer. Independent of the crawler: it measures what
    // is already archived, so it is useful on a staging machine that collects
    // nothing. Failure to start is logged and never fatal.
    if starpass::enabled() {
        let settings = starpass::Settings::from_env();
        if starpass::catalog_present(&settings.catalog_path) {
            tokio::spawn(starpass::run_loop(db_path.clone(), settings));
        } else {
            tracing::warn!(
                catalog = %settings.catalog_path.display(),
                "star photometry disabled: set GAIA_STARPHOT_CATALOG to the WISCAT1 file"
            );
        }
    } else {
        tracing::info!("Star photometry pass disabled");
    }
    let static_dir = std::env::var("GAIA_STATIC_DIR").unwrap_or_else(|_| "web-dist".into());
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/sources", get(sources))
        .route("/api/sources/{id}/latest", get(latest_image))
        .route("/api/sources/{id}/frames", get(source_frames))
        .route("/api/sources/{id}/projection", get(projected_image))
        .route("/api/igrf-maglat", get(igrf_maglat))
        .route("/api/suggestions", post(suggest))
        .route("/api/sources/{id}/location", post(set_location))
        .route("/api/sources/{id}/enabled", post(set_enabled))
        .route("/api/sources/{id}", delete(remove_source))
        .route(
            "/api/sources/{id}/settings",
            get(get_camera_settings).post(camera_settings),
        )
        .route("/api/credits", get(credits))
        .route("/api/history", get(history))
        .route("/api/projection-assets/{name}", get(projection_asset))
        .route("/api/images/{id}/texture", get(image_texture))
        .route("/api/images/{id}/original", get(original_image))
        .route("/api/calibrations", post(calibration))
        .route("/api/sources/{id}/stars", get(source_stars))
        .route("/api/sources/{id}/stars/series", get(source_star_series))
        .route(
            "/api/sources/{id}/calibrations",
            get(source_calibrations).post(set_selected_calibration),
        )
        .route("/api/ingest", post(ingest))
        .fallback_service(ServeDir::new(static_dir).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let addr = std::env::var("GAIA_LISTEN").unwrap_or_else(|_| "127.0.0.1:8765".into());
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!(%addr,"GAIA listening");
    axum::serve(listener, app).await?;
    Ok(())
}
