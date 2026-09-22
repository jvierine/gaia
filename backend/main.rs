mod archive;
mod calibration_image;
mod cloudweight;
mod crawler;
mod db;
mod equalize;
mod event_study;
mod extinction;
mod geometry;
mod igrf_grid;
mod keogram;
mod lensfit;
mod image_time;
mod meteor_backfill;
mod model;
mod norsk_meteor;
mod pixel_mask;
mod projection;
mod processing;
mod publish;
mod publish_layers;
mod quality;
mod response;
mod starpair;
mod starpass;
mod starphot;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State, Query},
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
    tokio::task::spawn_blocking(move || {
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
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
    }).await.map_err(internal)?
}

async fn credits(State(s): State<AppState>) -> ApiResult<Json<Vec<Value>>> {
    tokio::task::spawn_blocking(move || {
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
    let mut query = conn
        .prepare("SELECT name,website,acknowledgement,copyright FROM producers ORDER BY name")
        .map_err(internal)?;
    let rows=query.query_map([],|r|Ok(json!({"name":r.get::<_,String>(0)?,"website":r.get::<_,Option<String>>(1)?,"acknowledgement":r.get::<_,String>(2)?,"copyright":r.get::<_,String>(3)?}))).map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
    }).await.map_err(internal)?
}

async fn sources(State(s): State<AppState>) -> ApiResult<Json<Vec<SourceStatus>>> {
    tokio::task::spawn_blocking(move || {
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
    let mut q=conn.prepare("SELECT s.id,s.name,p.name,s.timestamp_mode,s.last_success_utc,s.last_error,(SELECT max(observation_utc) FROM images i WHERE i.source_id=s.id),(SELECT max(downloaded_utc) FROM images i WHERE i.source_id=s.id),(SELECT count(*) FROM images i WHERE i.source_id=s.id AND i.downloaded_utc >= datetime('now','-1 day')),s.latitude_deg,s.longitude_deg,EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id),s.enabled,COALESCE(cs.quality_exponent,0),MAX(COALESCE(cs.updated_utc,''),COALESCE((SELECT MAX(created_utc) FROM calibrations c WHERE c.source_id=s.id),'')) FROM sources s JOIN producers p ON p.id=s.producer_id LEFT JOIN camera_settings cs ON cs.source_id=s.id WHERE NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY s.name").map_err(internal)?;
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
                processing: processing::read(&s,&r.get::<_,String>(0)?,r.get::<_,Option<String>>(14)?.as_deref()),
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
    }).await.map_err(internal)?
}

async fn history(State(s): State<AppState>) -> ApiResult<Json<Vec<String>>> {
    tokio::task::spawn_blocking(move || {
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
    let mut q=conn.prepare("SELECT DISTINCT strftime('%Y-%m-%dT%H:%M:00Z',i.observation_utc) AS minute FROM images i JOIN sources s ON s.id=i.source_id WHERE s.enabled=1 AND EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) AND julianday(i.observation_utc)>=julianday('now','-1 day') ORDER BY minute").map_err(internal)?;
    let rows = q
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>, _>>().map_err(internal)?))
    }).await.map_err(internal)?
}

#[derive(Deserialize, Default)]
struct FrameHistoryRequest { date: Option<String> }
fn frame_history_bounds(date: Option<&str>) -> Result<(String,String), String> {
    if let Some(value)=date {
        let day=chrono::NaiveDate::parse_from_str(value,"%Y-%m-%d").map_err(|_|"Date must be YYYY-MM-DD".to_string())?;
        if day.format("%Y-%m-%d").to_string()!=value {return Err("Date must be YYYY-MM-DD".into());}
        let next=day.succ_opt().ok_or("Date is out of range")?;
        Ok((day.and_hms_opt(0,0,0).unwrap().and_utc().to_rfc3339(),next.and_hms_opt(0,0,0).unwrap().and_utc().to_rfc3339()))
    } else {let now=Utc::now();Ok(((now-chrono::Duration::hours(24)).to_rfc3339(),now.to_rfc3339()))}
}
async fn source_frames(
    Path(id): Path<String>, State(s): State<AppState>, Query(request): Query<FrameHistoryRequest>,
) -> ApiResult<Json<Vec<Value>>> {
    let (start,end)=frame_history_bounds(request.date.as_deref()).map_err(|e|(StatusCode::BAD_REQUEST,e))?;
    tokio::task::spawn_blocking(move || {
    let conn=db::open_readonly(&s.db_path).map_err(internal)?;
    let mut query=conn.prepare("SELECT id,observation_utc,width,height FROM images WHERE source_id=?1 AND julianday(observation_utc)>=julianday(?2) AND julianday(observation_utc)<julianday(?3) ORDER BY julianday(observation_utc),id").map_err(internal)?;
    let rows=query.query_map(rusqlite::params![id,start,end],|row|Ok(json!({"id":row.get::<_,String>(0)?,"observation_utc":row.get::<_,String>(1)?,"width":row.get::<_,Option<i64>>(2)?,"height":row.get::<_,Option<i64>>(3)?}))).map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>,_>>().map_err(internal)?))
    }).await.map_err(internal)?
}
#[cfg(test)] mod frame_history_tests {
    #[tokio::test(flavor="multi_thread",worker_threads=2)]
    async fn startup_reads_complete_while_writer_is_active() {
        let root=std::env::temp_dir().join(format!("gaia-catalogue-{}",uuid::Uuid::new_v4()));
        let state=super::AppState{db_path:root.join("test.sqlite3"),archive_root:root.clone(),sources:std::sync::Arc::new(vec![]),igrf:std::sync::Arc::new(vec![]),igrf_year:2026};
        let writer=super::db::open(&state.db_path).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2),async {
            super::sources(axum::extract::State(state.clone())).await.unwrap();
            super::history(axum::extract::State(state.clone())).await.unwrap();
            super::credits(axum::extract::State(state.clone())).await.unwrap();
            super::status(axum::extract::State(state.clone())).await.unwrap();
        }).await.expect("startup reads must not wait for writer locks");
        writer.execute_batch("ROLLBACK").unwrap();drop(writer);
        std::fs::remove_file(&state.db_path).unwrap();std::fs::remove_dir(root).unwrap();
    }

    #[test] fn history_uses_covering_index_and_utc_order() {
        let conn=rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn.pragma_update(None,"foreign_keys","OFF").unwrap();
        conn.execute_batch("INSERT INTO images(id,source_id,observation_utc,downloaded_utc,timestamp_basis,archive_path,sha256,media_type) VALUES ('a','camera','2026-09-09T23:00:00Z','','','a','a','image/jpeg'), ('b','camera','2026-09-09T01:00:00+00:00','','','b','b','image/jpeg'), ('c','camera','2026-09-10T00:00:00Z','','','c','c','image/jpeg'), ('d','other','2026-09-09T12:00:00Z','','','d','d','image/jpeg');").unwrap();
        let sql="SELECT id,observation_utc,width,height FROM images WHERE source_id=?1 AND julianday(observation_utc)>=julianday(?2) AND julianday(observation_utc)<julianday(?3) ORDER BY julianday(observation_utc),id";
        let params=["camera","2026-09-09","2026-09-10"];
        let plan=conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap().query_map(params,|r|r.get::<_,String>(3)).unwrap().collect::<Result<Vec<_>,_>>().unwrap().join(" ");
        assert!(plan.contains("COVERING INDEX idx_images_source_history"),"{plan}");
        assert!(!plan.contains("TEMP B-TREE"),"{plan}");
        let ids=conn.prepare(sql).unwrap().query_map(params,|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
        assert_eq!(ids,vec!["b","a"]);
    }
    #[test] fn history_read_does_not_wait_for_writer() {
        let path=std::env::temp_dir().join(format!("gaia-read-{}.sqlite3",uuid::Uuid::new_v4()));
        let writer=super::db::open(&path).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE").unwrap();
        let reader=super::db::open_readonly(&path).unwrap();
        let _:i64=reader.query_row("SELECT count(*) FROM images",[],|r|r.get(0)).unwrap();
        assert!(reader.execute("DELETE FROM images",[]).is_err());
        drop(reader);writer.execute_batch("ROLLBACK").unwrap();drop(writer);
        std::fs::remove_file(path).unwrap();
    }

    #[test] fn utc_day_bounds_and_invalid_dates(){
        let (start,end)=super::frame_history_bounds(Some("2026-09-09")).unwrap();
        assert_eq!(start,"2026-09-09T00:00:00+00:00");assert_eq!(end,"2026-09-10T00:00:00+00:00");
        assert!(super::frame_history_bounds(Some("2026-02-30")).is_err());
        assert!(super::frame_history_bounds(Some("2026-9-9")).is_err());
        let (start,end)=super::frame_history_bounds(None).unwrap();
        assert_eq!((chrono::DateTime::parse_from_rfc3339(&end).unwrap()-chrono::DateTime::parse_from_rfc3339(&start).unwrap()).num_hours(),24);
    }
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
    let conn = db::open_readonly(&s.db_path).map_err(internal)?;
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
        || !matches!(ext, "bin" | "png" | "jpg")
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
                } else if ext == "jpg" {
                    "image/jpeg"
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

#[derive(Deserialize,Default)]
struct ImageRequest { #[serde(default)] calibration: bool }
async fn calibration_copy(s:&AppState,id:&str,bytes:Vec<u8>)->ApiResult<Vec<u8>>{
 let settings={let conn=db::open_readonly(&s.db_path).map_err(internal)?;
 conn.query_row("SELECT cs.crop_json,cs.mask_json,COALESCE(cs.mask_enabled,1) FROM sources s LEFT JOIN camera_settings cs ON cs.source_id=s.id WHERE s.id=?1",[id],|r|Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,bool>(2)?))).map_err(internal)?};
 tokio::task::spawn_blocking(move||calibration_image::render(&bytes,settings.0.as_deref(),settings.1.as_deref(),settings.2)).await.map_err(internal)?.map_err(internal)
}

async fn latest_image(
    Path(id): Path<String>,
    State(s): State<AppState>,
    Query(request): Query<ImageRequest>,
) -> ApiResult<Response<Body>> {
    let row = {
        let conn = db::open_readonly(&s.db_path).map_err(internal)?;
        conn.query_row("SELECT i.archive_path,i.media_type,p.name,p.copyright,i.observation_utc,s.latitude_deg,s.longitude_deg,s.altitude_m,s.name FROM images i JOIN sources s ON s.id=i.source_id JOIN producers p ON p.id=s.producer_id WHERE i.source_id=?1 ORDER BY i.observation_utc DESC LIMIT 1",[&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<f64>>(5)?,r.get::<_,Option<f64>>(6)?,r.get::<_,Option<f64>>(7)?,r.get::<_,String>(8)?))).map_err(|e|if matches!(e,rusqlite::Error::QueryReturnedNoRows){(StatusCode::NOT_FOUND,"no image has been acquired for this camera yet".into())}else{internal(e)})?
    };
    let bytes = tokio::fs::read(&row.0).await.map_err(internal)?;
    let bytes=if request.calibration {calibration_copy(&s,&id,bytes).await?}else{bytes};
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, if request.calibration {"image/png".to_string()}else{row.1})
        .header(header::CACHE_CONTROL, "no-store")
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
    Query(request): Query<ImageRequest>,
) -> ApiResult<Response<Body>> {
    let row = {
        let conn = db::open_readonly(&s.db_path).map_err(internal)?;
        conn.query_row("SELECT i.archive_path,i.media_type,p.name,p.copyright,i.observation_utc,s.latitude_deg,s.longitude_deg,s.altitude_m,s.name,s.id FROM images i JOIN sources s ON s.id=i.source_id JOIN producers p ON p.id=s.producer_id WHERE i.id=?1",[&id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,Option<f64>>(5)?,r.get::<_,Option<f64>>(6)?,r.get::<_,Option<f64>>(7)?,r.get::<_,String>(8)?,r.get::<_,String>(9)?))).map_err(|e|if matches!(e,rusqlite::Error::QueryReturnedNoRows){(StatusCode::NOT_FOUND,"image not found".into())}else{internal(e)})?
    };
    let bytes = tokio::fs::read(&row.0).await.map_err(internal)?;
    let bytes=if request.calibration {calibration_copy(&s,&row.9,bytes).await?}else{bytes};
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, if request.calibration {"image/png".to_string()}else{row.1})
        .header(header::CACHE_CONTROL, if request.calibration {"no-store"}else{"public, max-age=31536000, immutable"})
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
    processing::kick();

    Ok((
        StatusCode::CREATED,
        Json(json!({"id":id,"state":"calibrated","source_id":source,"star_count":stars,"residual_px":residual_px})),
    ))
}

#[cfg(test)]
mod calibration_refit_tests {
    /// A refit is worthless if nothing can read it back. The archive reads
    /// calibrations with h5dump and takes the star count from the row count of
    /// /selected_stars, so those are the two things this checks -- through the
    /// real readers, not by inspecting what was just written.
    #[test]
    fn a_written_calibration_reads_back_through_the_normal_readers() {
        if std::process::Command::new("python3")
            .args(["-c", "import h5py, numpy"])
            .output()
            .map(|o| !o.status.success())
            .unwrap_or(true)
        {
            eprintln!("skipping: h5py unavailable");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("refit.h5");
        let optpar = [2.0, 0.733181, 0.733112, -1.454884, -1.366835, -170.19926, 0.007369, -0.003686, 0.460838];
        let rows: Vec<Vec<f64>> = (0..37)
            .map(|n| {
                let v = n as f64;
                vec![v + 1.0, 1.0, 40.0 + v, 10.0 * v, 100.0 + v, 200.0 + v, v / 24.0, v, 3.0,
                     100.5 + v, 200.5 + v, 0.5, 0.5, 0.707]
            })
            .collect();
        super::write_calibration_hdf5(&path, &optpar, &rows).unwrap();

        let back = super::projection::optical_parameters(&path.to_string_lossy()).unwrap();
        assert_eq!(back.len(), optpar.len());
        for (a, b) in back.iter().zip(optpar.iter()) {
            assert!((a - b).abs() < 1e-12, "parameter changed in the round trip: {a} vs {b}");
        }
        let count = super::projection::dataset_rows(&path.to_string_lossy(), "/selected_stars").unwrap();
        assert_eq!(count, 37, "star count is read from the row count");
    }
}

/// The richest frame of the current night, and how well the live lens model
/// places its stars.
///
/// A lens drifts. The star photometry measures, on every frame, the distance
/// between where the sky says a star should fall and where its centroid was
/// found, so the archive can notice its own calibrations going stale without
/// anyone re-observing anything. The frame with the most identified stars is
/// the one that says the most about it.
fn calibration_drift(
    conn: &rusqlite::Connection,
    source_id: &str,
    judge: Option<&str>,
) -> anyhow::Result<Value> {
    let (longitude, mut hdf5, mut calibration_id, mut star_count, mut residual): (
        f64, String, String, Option<i64>, Option<f64>,
    ) = conn.query_row(
        "SELECT COALESCE(s.longitude_deg,0),c.hdf5_path,c.id,c.star_count,c.residual_px
         FROM sources s JOIN calibrations c ON c.id=COALESCE(
            (SELECT cs.selected_calibration_id FROM camera_settings cs
              WHERE cs.source_id=s.id
                AND EXISTS(SELECT 1 FROM calibrations k WHERE k.id=cs.selected_calibration_id AND k.source_id=s.id)),
            (SELECT k.id FROM calibrations k WHERE k.source_id=s.id
              ORDER BY k.created_utc DESC LIMIT 1))
         WHERE s.id=?1",
        [source_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
    )?;
    // A calibration named explicitly is the one to judge. Without this the
    // panel can only ever show the residuals of whichever model happened to be
    // in force, and choosing a different one changes nothing on screen.
    if let Some(wanted) = judge {
        if let Ok(row) = conn.query_row(
            "SELECT hdf5_path,id,star_count,residual_px FROM calibrations
             WHERE id=?1 AND source_id=?2",
            rusqlite::params![wanted, source_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                    r.get::<_, Option<i64>>(2)?, r.get::<_, Option<f64>>(3)?)),
        ) {
            (hdf5, calibration_id, star_count, residual) = row;
        }
    }

    // The most recent night that actually has stars, by local solar time at
    // this station so a night is not cut in half at midnight UTC.
    //
    // Not simply the night the clock is in. Checking a camera at eleven in the
    // morning is the ordinary case, and by then the current bucket has rolled
    // over at local solar noon and holds nothing but daylight -- the panel
    // answered "no stars measured yet tonight" for every camera in the archive.
    // Taking the night of the newest measurement gives last night when it is
    // daytime and tonight once tonight has started.
    let newest: Option<String> = conn
        .query_row(
            "SELECT max(observation_utc) FROM star_photometry
             WHERE source_id=?1 AND channel='mean' AND centroid_x IS NOT NULL",
            [source_id],
            |r| r.get(0),
        )
        .ok()
        .flatten();
    let newest_epoch = newest
        .as_deref()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp() as f64);
    let now = Utc::now().timestamp() as f64;
    let night = extinction::night_index(newest_epoch.unwrap_or(now), longitude);
    let current = night == extinction::night_index(now, longitude);
    let boundary = |n: i64| {
        DateTime::from_timestamp(
            ((n as f64) * 86400.0 + 43200.0 - longitude / 15.0 * 3600.0) as i64,
            0,
        )
        .map(|t| t.to_rfc3339())
        .unwrap_or_default()
    };
    let (from, to) = (boundary(night), boundary(night + 1));

    // The frame of that night with the most stars actually found.
    let best: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT image_id,observation_utc,count(*) AS found
             FROM star_photometry
             WHERE source_id=?1 AND channel='mean' AND centroid_x IS NOT NULL
               AND observation_utc>=?2 AND observation_utc<?3
             GROUP BY image_id ORDER BY found DESC, observation_utc DESC LIMIT 1",
            rusqlite::params![source_id, from, to],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();

    let Some((image_id, observation_utc, found)) = best else {
        return Ok(json!({
            "source_id": source_id, "night": night, "from": from, "to": to,
            "current_night": current,
            "calibration_id": calibration_id, "calibration_star_count": star_count,
            "calibration_residual_px": residual,
            "frame": Value::Null,
            "note": "no stars have been measured for this camera",
        }));
    };

    let (width, height): (Option<i64>, Option<i64>) = conn
        .query_row("SELECT width,height FROM images WHERE id=?1", [&image_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap_or((None, None));

    let mut q = conn.prepare(
        "SELECT star_key,vt_mag,azimuth_deg,elevation_deg,predicted_x,predicted_y,
                centroid_x,centroid_y,centroid_offset_px
         FROM star_photometry
         WHERE source_id=?1 AND channel='mean' AND image_id=?2 AND centroid_x IS NOT NULL
         ORDER BY vt_mag",
    )?;
    // The stored prediction was made with whatever calibration was in force
    // when the frame was measured, so it cannot answer "how would *this* model
    // place these stars". Re-project each star through the calibration being
    // judged; fall back to the stored value only if the model cannot be read.
    let optpar = projection::optical_parameters(&hdf5).ok();
    let (fw, fh) = (width.unwrap_or(0) as f64, height.unwrap_or(0) as f64);
    let stars: Vec<Value> = q
        .query_map(rusqlite::params![source_id, image_id], |r| {
            let azimuth: f64 = r.get("azimuth_deg")?;
            let elevation: f64 = r.get("elevation_deg")?;
            let stored = (r.get::<_, f64>("predicted_x")?, r.get::<_, f64>("predicted_y")?);
            let (px, py) = match optpar.as_ref().filter(|_| fw > 0.0 && fh > 0.0) {
                Some(p) => starphot::az_el_to_pixel(azimuth, elevation, p, fw, fh)
                    .unwrap_or(stored),
                None => stored,
            };
            let centroid_x: Option<f64> = r.get("centroid_x")?;
            let centroid_y: Option<f64> = r.get("centroid_y")?;
            let offset = match (centroid_x, centroid_y) {
                (Some(x), Some(y)) => Some(((x - px).powi(2) + (y - py).powi(2)).sqrt()),
                _ => None,
            };
            Ok(json!({
                "star_key": r.get::<_,String>("star_key")?,
                "vt_mag": r.get::<_,f64>("vt_mag")?,
                "azimuth_deg": azimuth,
                "elevation_deg": elevation,
                "predicted_x": px,
                "predicted_y": py,
                "centroid_x": centroid_x,
                "centroid_y": centroid_y,
                "offset_px": offset,
            }))
        })?
        .filter_map(Result::ok)
        .collect();

    // The frame's own sky and brightest star, so the panel can offer a stretch
    // that actually shows the stars rather than a guessed one.
    let (sky, brightest): (Option<f64>, Option<f64>) = conn
        .query_row(
            "SELECT median_background, peak FROM (
               SELECT AVG(background) AS median_background,
                      MAX(COALESCE(peak_raw, background + amplitude)) AS peak
               FROM star_photometry
               WHERE source_id=?1 AND channel='mean' AND image_id=?2
                 AND background IS NOT NULL)",
            rusqlite::params![source_id, image_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((None, None));

    let offsets: Vec<f64> = stars.iter().filter_map(|s| s["offset_px"].as_f64()).collect();
    let rms = if offsets.is_empty() {
        None
    } else {
        Some((offsets.iter().map(|d| d * d).sum::<f64>() / offsets.len() as f64).sqrt())
    };
    // A refit wants more evidence than the calibration was built from, which is
    // the one comparison that does not depend on trusting either fit. But many
    // calibrations record no star count at all -- every UCalgary model does --
    // and refusing those was refusing the cameras most in need of checking.
    // Unknown is not worse than known-larger: it is simply unknown, and the
    // reason says which case applies.
    let richer = match star_count {
        Some(n) => found > n,
        None => true,
    };
    Ok(json!({
        "source_id": source_id, "night": night, "from": from, "to": to,
        "current_night": current,
        "calibration_id": calibration_id, "calibration_star_count": star_count,
        "calibration_residual_px": residual, "hdf5_path": hdf5,
        "frame": {
            "image_id": image_id, "observation_utc": observation_utc,
            "width": width, "height": height, "found": found,
            "rms_offset_px": rms,
            "sky_level": sky, "brightest_level": brightest,
        },
        "stars": stars,
        "refit_available": richer && found as usize >= lensfit::MIN_OBSERVATIONS,
        "judging_calibration_id": calibration_id,
        "refit_reason": if !richer {
            format!("this frame has {found} stars, the calibration in force used {}",
                    star_count.map(|n| n.to_string()).unwrap_or_else(|| "an unrecorded number".into()))
        } else if (found as usize) < lensfit::MIN_OBSERVATIONS {
            format!("{found} stars is too few to fit eight parameters")
        } else {
            match star_count {
                Some(n) => format!("{found} stars against {n} in the calibration in force"),
                None => format!(
                    "{found} stars; the calibration in force records no star count, so there is \
nothing to compare against and the refit is offered on the frame's own strength"
                ),
            }
        },
    }))
}

/// Write a refitted lens model as an AIDA/WISC calibration file.
///
/// The archive reads calibrations with `h5dump`, so a refit has to produce a
/// real HDF5 file rather than a private format, or every later reader would
/// need a second code path. h5py does the writing; the datasets are the ones
/// the readers actually look for -- the parameter vector, and the selected
/// stars whose row count is what the star count is taken from.
fn write_calibration_hdf5(
    path: &std::path::Path,
    optpar: &[f64],
    rows: &[Vec<f64>],
) -> anyhow::Result<()> {
    let payload = json!({"path": path.to_string_lossy(), "optpar": optpar, "stars": rows});
    let script = r#"
import json, sys, numpy, h5py
spec = json.load(sys.stdin)
optpar = numpy.array(spec["optpar"], dtype="f8")
stars = numpy.array(spec["stars"], dtype="f8")
residuals = stars[:, 11:14] if stars.size else numpy.zeros((0, 3))
with h5py.File(spec["path"], "w") as f:
    f.create_dataset("wisc_optpar_with_optmod", data=optpar)
    f.create_dataset("wisc_optpar", data=optpar[1:])
    f.create_dataset("selected_stars", data=stars)
    f.create_dataset("residuals_px", data=residuals)
"#;
    let mut child = std::process::Command::new("python3")
        .args(["-c", script])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().ok_or_else(|| anyhow::anyhow!("python stdin"))?;
        stdin.write_all(serde_json::to_string(&payload)?.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        anyhow::bail!("writing the calibration failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

/// Everything WISC/AIDA needs to show a proposed refit for inspection: which
/// frame, which stars were identified in it, and the lens model that best
/// places them.
///
/// Computed and returned; **nothing is written**. Some automatically fitted
/// stars are dubious -- a hot pixel, a satellite, a star pulled onto a
/// neighbour -- and a model fitted through them should be looked at by a person
/// before it becomes a calibration. So this produces a proposal and AIDA is
/// where it is accepted or rejected.
fn refit_proposal(
    conn: &rusqlite::Connection,
    source_id: &str,
) -> anyhow::Result<Value> {
    let drift = calibration_drift(conn, source_id, None)?;
    let frame = &drift["frame"];
    if frame.is_null() {
        anyhow::bail!("no measured frame to fit");
    }
    let hdf5 = drift["hdf5_path"].as_str().unwrap_or_default();
    let seed = projection::optical_parameters(hdf5)?;
    let (width, height) = (
        frame["width"].as_f64().unwrap_or(0.0),
        frame["height"].as_f64().unwrap_or(0.0),
    );
    if !(width > 0.0 && height > 0.0) {
        anyhow::bail!("frame has no recorded dimensions");
    }
    let empty = Vec::new();
    let stars = drift["stars"].as_array().unwrap_or(&empty);
    let observations: Vec<lensfit::Observation> = stars
        .iter()
        .filter_map(|v| {
            Some(lensfit::Observation {
                azimuth_deg: v["azimuth_deg"].as_f64()?,
                elevation_deg: v["elevation_deg"].as_f64()?,
                x: v["centroid_x"].as_f64()?,
                y: v["centroid_y"].as_f64()?,
            })
        })
        .collect();
    let before = lensfit::rms(&seed, &observations, width, height);
    let (fitted, after) = lensfit::fit(&seed, &observations, width, height)
        .ok_or_else(|| anyhow::anyhow!("the fit did not converge"))?;
    let residuals = lensfit::residuals(&fitted, &observations, width, height);

    // AIDA's own match record: a picked image point paired with a catalogue
    // star. Zenith angle rather than elevation, which is what it stores.
    let matches: Vec<Value> = stars
        .iter()
        .zip(observations.iter())
        .enumerate()
        .map(|(n, (star, observation))| {
            let key = star["star_key"].as_str().unwrap_or_default();
            let (dx, dy, dr) = residuals.get(n).copied().unwrap_or((0.0, 0.0, 0.0));
            let (ra, dec) = key
                .split_once(|c| c == '+' || c == '-')
                .map(|(a, b)| {
                    let sign = if key.contains('-') { -1.0 } else { 1.0 };
                    (a.parse::<f64>().unwrap_or(0.0), sign * b.parse::<f64>().unwrap_or(0.0))
                })
                .unwrap_or((0.0, 0.0));
            json!({
                "id": n + 1,
                "star_key": key,
                "image_x": observation.x,
                "image_y": observation.y,
                "ra_hours": ra,
                "dec_deg": dec,
                "mag": star["vt_mag"].as_f64().unwrap_or(0.0),
                "azimuth_deg": observation.azimuth_deg,
                "zenith_deg": 90.0 - observation.elevation_deg,
                "elevation_deg": observation.elevation_deg,
                // Under the proposed model, so a star placed badly by it is
                // visible before anyone accepts it.
                "residual_px": dr,
                "residual_dx": dx,
                "residual_dy": dy,
            })
        })
        .collect();

    Ok(json!({
        "source_id": source_id,
        "image_id": frame["image_id"],
        "observation_utc": frame["observation_utc"],
        "width": width, "height": height,
        "from_calibration_id": drift["calibration_id"],
        "optmod": seed.first().copied().unwrap_or(2.0) as i64,
        "optpar_with_optmod": fitted,
        // The eight free parameters, the order AIDA's own controls take.
        "optpar": fitted.get(1..).map(|v| v.to_vec()).unwrap_or_default(),
        "seed_optpar_with_optmod": seed,
        "stars": matches.len(),
        "residual_px_before": before,
        "residual_px_after": after,
        "matches": matches,
        "note": "A proposal only. Nothing has been written; inspect the identifications in \
WISC/AIDA and send the calibration back from there if they are sound.",
    }))
}

async fn source_refit_proposal(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let built = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        let conn = db::open(&s.db_path)?;
        refit_proposal(&conn, &id)
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::CONFLICT, e.to_string()))?;
    Ok(Json(built))
}

/// Refit the lens from tonight's richest frame and keep the result.
///
/// The new model is added to the list and deliberately **not** selected.
/// Which calibration a camera uses stays an administrator's decision: a refit
/// is evidence to look at, not a change to make on the archive's own authority.
async fn source_calibration_refit(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let drift = calibration_drift(&conn, &id, None).map_err(internal)?;
    if drift["refit_available"] != json!(true) {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "no refit warranted: {}",
                drift["refit_reason"].as_str().unwrap_or("unknown")
            ),
        ));
    }
    let hdf5 = drift["hdf5_path"].as_str().unwrap_or_default().to_string();
    let seed = projection::optical_parameters(&hdf5).map_err(internal)?;
    let (width, height) = (
        drift["frame"]["width"].as_f64().unwrap_or(0.0),
        drift["frame"]["height"].as_f64().unwrap_or(0.0),
    );
    if !(width > 0.0 && height > 0.0) {
        return Err((StatusCode::CONFLICT, "frame has no recorded dimensions".into()));
    }
    let empty = Vec::new();
    let stars = drift["stars"].as_array().unwrap_or(&empty);
    let observations: Vec<lensfit::Observation> = stars
        .iter()
        .filter_map(|v| {
            Some(lensfit::Observation {
                azimuth_deg: v["azimuth_deg"].as_f64()?,
                elevation_deg: v["elevation_deg"].as_f64()?,
                x: v["centroid_x"].as_f64()?,
                y: v["centroid_y"].as_f64()?,
            })
        })
        .collect();
    let before = lensfit::rms(&seed, &observations, width, height);
    let (fitted, after) = lensfit::fit(&seed, &observations, width, height)
        .ok_or((StatusCode::CONFLICT, "the fit did not converge".into()))?;
    // A refit that is not better than what it started from is not worth
    // keeping; storing it would only clutter the list an operator has to judge.
    if !(after < before) {
        return Err((
            StatusCode::CONFLICT,
            format!("refit did not improve on the calibration in force: {before:.3} -> {after:.3} px"),
        ));
    }

    // The selected_stars layout the AIDA files use, so the new file is readable
    // by the same tools: index, frame, elevation, azimuth, measured x and y,
    // right ascension and declination, magnitude, predicted x and y, and the
    // three residuals.
    let residuals = lensfit::residuals(&fitted, &observations, width, height);
    let mut rows = Vec::new();
    for (n, (star, observation)) in stars.iter().zip(observations.iter()).enumerate() {
        let (dx, dy, dr) = residuals.get(n).copied().unwrap_or((0.0, 0.0, 0.0));
        let key = star["star_key"].as_str().unwrap_or_default();
        let (ra, dec) = key
            .split_once(|c| c == '+' || c == '-')
            .map(|(a, b)| {
                let sign = if key.contains('-') { -1.0 } else { 1.0 };
                (a.parse::<f64>().unwrap_or(0.0), sign * b.parse::<f64>().unwrap_or(0.0))
            })
            .unwrap_or((0.0, 0.0));
        rows.push(vec![
            (n + 1) as f64, 1.0,
            observation.elevation_deg, observation.azimuth_deg,
            observation.x, observation.y,
            ra, dec,
            star["vt_mag"].as_f64().unwrap_or(0.0),
            observation.x + dx, observation.y + dy,
            dx, dy, dr,
        ]);
    }

    let new_id = uuid::Uuid::new_v4().to_string();
    let dir = s.archive_root.join("calibrations").join(&id);
    std::fs::create_dir_all(&dir).map_err(internal)?;
    let path = dir.join(format!("{new_id}.h5"));
    write_calibration_hdf5(&path, &fitted, &rows).map_err(internal)?;
    conn.execute(
        "INSERT INTO calibrations(id,source_id,created_utc,method,hdf5_path,residual_px,submitted_by,star_count)
         VALUES(?1,?2,?3,'AIDA/WISC refit',?4,?5,'gaia drift check',?6)",
        rusqlite::params![
            new_id, id, Utc::now().to_rfc3339(), path.to_string_lossy(),
            after, observations.len() as i64
        ],
    )
    .map_err(internal)?;

    // Where the refitted model puts each star, beside where it was actually
    // found. Without this the panel can only show the residuals of the model
    // being replaced, which says nothing about whether the replacement is any
    // better -- and that judgement is the entire purpose of a refit.
    let refitted: Vec<Value> = stars
        .iter()
        .zip(observations.iter())
        .enumerate()
        .map(|(n, (star, observation))| {
            let (dx, dy, _) = residuals.get(n).copied().unwrap_or((0.0, 0.0, 0.0));
            json!({
                "star_key": star["star_key"],
                "vt_mag": star["vt_mag"],
                "centroid_x": observation.x,
                "centroid_y": observation.y,
                // The prediction is the measured position plus the residual,
                // which is how `lensfit::residuals` defines it.
                "predicted_x": observation.x + dx,
                "predicted_y": observation.y + dy,
                "offset_px": dx.hypot(dy),
            })
        })
        .collect();

    Ok(Json(json!({
        "id": new_id,
        "source_id": id,
        "selected": false,
        "stars": refitted,
        "note": "added to the list, not made live; selecting it is an administrator's decision",
        "star_count": observations.len(),
        "residual_px_before": before,
        "residual_px_after": after,
        "optpar": fitted,
        "frame": drift["frame"],
        "previous_calibration_id": drift["calibration_id"],
    })))
}

#[derive(Deserialize)]
struct DriftQuery {
    /// Judge this calibration instead of the one in force, so an operator can
    /// compare models without switching the live one.
    calibration_id: Option<String>,
}

async fn source_calibration_drift(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<DriftQuery>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    Ok(Json(
        calibration_drift(&conn, &id, query.calibration_id.as_deref()).map_err(internal)?,
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
    from: Option<String>,
    to: Option<String>,
}

#[derive(Deserialize)]
struct StarNightsQuery {
    channel: Option<String>,
    days: Option<f64>,
}

/// The window a star request covers: an explicit night range when one is given,
/// otherwise the trailing hours the panel has always used.
fn star_window(from: &Option<String>, to: &Option<String>, hours: Option<f64>) -> (String, String) {
    let parses = |v: &Option<String>| {
        v.as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc).to_rfc3339())
    };
    match (parses(from), parses(to)) {
        (Some(a), Some(b)) if a < b => (a, b),
        _ => {
            let hours = hours.unwrap_or(24.0).clamp(0.1, 24.0 * 400.0);
            let now = Utc::now();
            (
                (now - chrono::Duration::milliseconds((hours * 3_600_000.0) as i64)).to_rfc3339(),
                // Open at the top: a frame is never in the future, and clamping
                // to now would drop one recorded in the same second.
                (now + chrono::Duration::days(1)).to_rfc3339(),
            )
        }
    }
}

#[derive(Deserialize)]
struct StarFrameQuery {
    /// Instant to scrub to, RFC 3339. The nearest measured frame is returned.
    at: String,
    /// "next" or "prev" to step to the neighbouring measured frame instead of
    /// the nearest one. Stepping is by measured frame, not by wall-clock
    /// interval, so a gap in the archive is one step rather than many.
    step: Option<String>,
    channel: Option<String>,
    hours: Option<f64>,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Deserialize)]
struct StarSeriesQuery {
    star: String,
    channel: Option<String>,
    hours: Option<f64>,
    from: Option<String>,
    to: Option<String>,
}

#[derive(Deserialize)]
struct StarDistributionQuery {
    star: String,
    /// How far back to gather. The default is the whole archive.
    days: Option<f64>,
}

/// Every detected measurement of one star, in all four channels, as bare
/// columns over a long baseline.
///
/// The histograms want the season, not the night on screen. A star at high
/// latitude barely changes elevation, so its air mass hardly varies from one
/// night to the next and its clear-sky level is far better determined over
/// months than over hours; that level is what a fading is measured against.
/// Returning columns rather than whole sample rows keeps a season of data small
/// enough to send: three numbers per measurement instead of twenty-five.
async fn source_star_distribution(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<StarDistributionQuery>,
) -> ApiResult<Json<Value>> {
    let days = query.days.unwrap_or(400.0).clamp(1.0, 4000.0);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut statement = conn
        .prepare(
            "SELECT channel,flux,amplitude,background FROM star_photometry
             WHERE source_id=?1 AND star_key=?2 AND amplitude IS NOT NULL
               AND julianday(observation_utc) >= julianday('now') - ?3
             ORDER BY observation_utc",
        )
        .map_err(internal)?;
    let mut channels: std::collections::BTreeMap<String, (Vec<f64>, Vec<f64>, Vec<f64>)> =
        Default::default();
    let rows = statement
        .query_map(rusqlite::params![id, query.star, days], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<f64>>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
            ))
        })
        .map_err(internal)?;
    let mut total = 0usize;
    for row in rows {
        let (channel, flux, amplitude, background) = row.map_err(internal)?;
        let (Some(flux), Some(amplitude), Some(background)) = (flux, amplitude, background) else {
            continue;
        };
        let entry = channels.entry(channel).or_default();
        entry.0.push(flux);
        entry.1.push(amplitude);
        entry.2.push(background);
        total += 1;
    }
    let listed: serde_json::Map<String, Value> = channels
        .into_iter()
        .map(|(channel, (flux, amplitude, background))| {
            (channel, json!({"flux": flux, "amplitude": amplitude, "background": background}))
        })
        .collect();
    Ok(Json(json!({
        "source_id": id, "star_key": query.star, "days": days,
        "samples": total, "channels": Value::Object(listed),
    })))
}

#[derive(Deserialize)]
struct ExtinctionQuery {
    /// Limit the listing to one camera. Omitted, the whole archive is summarised.
    source: Option<String>,
    /// How many recent fits to list. The summary counts are never truncated.
    limit: Option<i64>,
}

/// The clear-sky reference, as the archive currently holds it.
///
/// This exists to be readable without opening the database. The fit runs in a
/// background pass whose only product is two tables, and checking whether it is
/// working had meant taking a copy of the live database from another account,
/// which has twice left it unwritable and the API down. A read-only endpoint
/// costs nothing and removes the reason to ever do that.
async fn extinction_state(
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ExtinctionQuery>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let filter = query.source.clone().unwrap_or_default();
    let all = filter.is_empty();
    let limit = query.limit.unwrap_or(20).clamp(1, 500);

    let depths: (i64, i64, Option<f64>, Option<f64>) = conn
        .query_row(
            "SELECT count(optical_depth), count(*),
                    avg(optical_depth), max(optical_depth)
             FROM star_photometry WHERE ?1 OR source_id=?2",
            rusqlite::params![all, filter],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(internal)?;

    let counts: (i64, i64, String, i64, i64, String) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM extinction_nights WHERE ?1 OR source_id=?2),
                    (SELECT count(DISTINCT source_id) FROM extinction_nights WHERE ?1 OR source_id=?2),
                    (SELECT COALESCE(max(fitted_utc),'') FROM extinction_nights WHERE ?1 OR source_id=?2),
                    (SELECT count(*) FROM extinction_attempts WHERE ?1 OR source_id=?2),
                    (SELECT count(DISTINCT source_id) FROM extinction_attempts WHERE ?1 OR source_id=?2),
                    (SELECT COALESCE(max(attempted_utc),'') FROM extinction_attempts WHERE ?1 OR source_id=?2)",
            rusqlite::params![all, filter],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .map_err(internal)?;

    let mut by_channel = conn
        .prepare(
            "SELECT channel,count(*),avg(k_mag_per_airmass),min(k_mag_per_airmass),
                    max(k_mag_per_airmass),avg(stars)
             FROM extinction_nights WHERE ?1 OR source_id=?2 GROUP BY channel ORDER BY channel",
        )
        .map_err(internal)?;
    let channels: Vec<Value> = by_channel
        .query_map(rusqlite::params![all, filter], |r| {
            Ok(json!({
                "channel": r.get::<_, String>(0)?,
                "fits": r.get::<_, i64>(1)?,
                "mean_k": r.get::<_, Option<f64>>(2)?,
                "min_k": r.get::<_, Option<f64>>(3)?,
                "max_k": r.get::<_, Option<f64>>(4)?,
                "mean_moving_stars": r.get::<_, Option<f64>>(5)?,
            }))
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;

    let mut recent = conn
        .prepare(
            "SELECT source_id,night,channel,k_mag_per_airmass,stars,samples,airmass_span,
                    envelope_scatter,curvature,fitted_utc
             FROM extinction_nights WHERE ?1 OR source_id=?2
             ORDER BY fitted_utc DESC LIMIT ?3",
        )
        .map_err(internal)?;
    let fits: Vec<Value> = recent
        .query_map(rusqlite::params![all, filter, limit], |r| {
            Ok(json!({
                "source_id": r.get::<_, String>(0)?,
                "night": r.get::<_, i64>(1)?,
                "channel": r.get::<_, String>(2)?,
                "k_mag_per_airmass": r.get::<_, f64>(3)?,
                "moving_stars": r.get::<_, i64>(4)?,
                "samples": r.get::<_, i64>(5)?,
                "airmass_span": r.get::<_, f64>(6)?,
                "envelope_scatter": r.get::<_, Option<f64>>(7)?,
                "curvature": r.get::<_, f64>(8)?,
                "fitted_utc": r.get::<_, String>(9)?,
            }))
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;

    // A refusal is a result, not a failure, so both are reported: a healthy
    // pass refuses most camera-nights, because most cannot constrain extinction.
    let refused = counts.3 - counts.0.min(counts.3);
    Ok(Json(json!({
        "source": query.source,
        "fits": counts.0,
        "cameras_fitted": counts.1,
        "newest_fit": if counts.2.is_empty() { Value::Null } else { json!(counts.2) },
        "attempts": counts.3,
        "cameras_attempted": counts.4,
        "newest_attempt": if counts.5.is_empty() { Value::Null } else { json!(counts.5) },
        "attempts_refused": refused,
        // How far the reference has been carried: a depth exists only for
        // measurements a fitted night explains and the gates accept.
        "measurements": depths.1,
        "with_optical_depth": depths.0,
        "mean_optical_depth": depths.2,
        "max_optical_depth": depths.3,
        "by_channel": channels,
        "recent": fits,
    })))
}

/// The observing nights this camera has photometry for, newest first.
///
/// A night runs local solar noon to noon, the same definition the clear-sky fit
/// uses, so one period of darkness is one entry instead of being split at
/// midnight UTC. Selecting by night is what an operator wants: a run of frames
/// under one sky, rather than a trailing number of hours that cuts an evening
/// in half.
async fn source_star_nights(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<StarNightsQuery>,
) -> ApiResult<Json<Value>> {
    let channel = query.channel.unwrap_or_else(|| "mean".into());
    if !starphot::CHANNELS.contains(&channel.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown channel".into()));
    }
    let days = query.days.unwrap_or(30.0).clamp(1.0, 400.0);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let longitude: f64 = conn
        .query_row("SELECT COALESCE(longitude_deg,0) FROM sources WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .unwrap_or(0.0);
    let mut statement = conn
        .prepare(
            "SELECT observation_utc,count(*),sum(amplitude IS NOT NULL) FROM star_photometry
             WHERE source_id=?1 AND channel=?2
               AND julianday(observation_utc) >= julianday('now', ?3)
             GROUP BY image_id ORDER BY observation_utc",
        )
        .map_err(internal)?;
    let rows = statement
        .query_map(rusqlite::params![id, channel, format!("-{days} days")], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
        })
        .map_err(internal)?;
    let mut nights: std::collections::BTreeMap<i64, (i64, i64, i64)> = Default::default();
    for row in rows {
        let (at, looked, found) = row.map_err(internal)?;
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&at) else { continue };
        let night = extinction::night_index(parsed.timestamp() as f64, longitude);
        let entry = nights.entry(night).or_insert((0, 0, 0));
        entry.0 += 1;
        entry.1 += looked;
        entry.2 += found;
    }
    // The UTC span of a night, so the client can ask for it without repeating
    // the local-solar arithmetic.
    let span = |night: i64| {
        let offset = longitude / 15.0 * 3600.0;
        let from = night as f64 * 86400.0 + 43200.0 - offset;
        let stamp = |t: f64| {
            chrono::DateTime::from_timestamp(t as i64, 0)
                .unwrap_or_else(Utc::now)
                .to_rfc3339()
        };
        (stamp(from), stamp(from + 86400.0))
    };
    let listed: Vec<Value> = nights
        .iter()
        .rev()
        .map(|(night, (frames, looked, found))| {
            let (from, to) = span(*night);
            json!({
                "night": night, "from": from, "to": to,
                "frames": frames, "looked_for": looked, "detections": found,
            })
        })
        .collect();
    Ok(Json(json!({"source_id": id, "channel": channel, "nights": listed})))
}

/// The horizon of a camera, projected into its own image, as a closed polygon.
/// This is the edge of the sky a fisheye actually sees; its corners are ground
/// and lens housing. Computed from the camera's own lens model and cached,
/// because reading that model shells out to `h5dump`.
fn field_outline(hdf5_path: &str, width: f64, height: f64) -> Option<Vec<[f64; 2]>> {
    static CACHE: std::sync::LazyLock<
        std::sync::Mutex<std::collections::HashMap<String, Option<Vec<[f64; 2]>>>>,
    > = std::sync::LazyLock::new(Default::default);
    let key = format!("{hdf5_path}|{width}x{height}");
    if let Some(hit) = CACHE.lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }
    let outline = projection::optical_parameters(hdf5_path).ok().and_then(|optpar| {
        let mut points = Vec::new();
        let mut outside = 0usize;
        for step in 0..180 {
            let az = step as f64 * 2.0;
            // The horizon itself. A star is only measured well above it, so this
            // is a generous edge rather than a tight one.
            let Some((x, y)) = starphot::az_el_to_pixel(az, 0.0, &optpar, width, height) else {
                return None;
            };
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            // A rectilinear lens throws its horizon to infinity; a few points
            // beyond the frame are ordinary for a fisheye whose circle is cut
            // off by the sensor, but a mostly-outside outline is not a field.
            if x < -width || x > 2.0 * width || y < -height || y > 2.0 * height {
                outside += 1;
            }
            points.push([x, y]);
        }
        if outside * 4 > points.len() { None } else { Some(points) }
    });
    if let Ok(mut cache) = CACHE.lock() {
        cache.insert(key, outline.clone());
    }
    outline
}

/// One measured frame, chosen as the nearest to a requested instant, with every
/// star looked for in it. Each star carries its brightness relative to the
/// brightest that star reached anywhere in the window, which is what makes the
/// colours comparable between a bright star and a faint one: the quantity of
/// interest is how far a star has fallen from its own best, not how bright it
/// is.
async fn source_star_frame(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<StarFrameQuery>,
) -> ApiResult<Json<Value>> {
    let channel = query.channel.unwrap_or_else(|| "mean".into());
    if !starphot::CHANNELS.contains(&channel.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown channel".into()));
    }
    let (from, to) = star_window(&query.from, &query.to, query.hours);
    let conn = db::open(&s.db_path).map_err(internal)?;
    // The nearest measured frame to the requested instant, or its neighbour
    // when stepping. Both stay inside the window.
    let order = match query.step.as_deref() {
        Some("next") => {
            "AND julianday(observation_utc) > julianday(?4) ORDER BY julianday(observation_utc)"
        }
        Some("prev") => {
            "AND julianday(observation_utc) < julianday(?4) ORDER BY julianday(observation_utc) DESC"
        }
        _ => "AND ?4 IS NOT NULL ORDER BY abs(julianday(observation_utc) - julianday(?4))",
    };
    let frame: Option<(String, String)> = conn
        .query_row(
            &format!(
                "SELECT image_id,observation_utc FROM star_photometry
                 WHERE source_id=?1 AND channel=?2
                   AND julianday(observation_utc) >= julianday(?3)
                   AND julianday(observation_utc) < julianday(?5)
                   {order}
                 LIMIT 1"
            ),
            rusqlite::params![id, channel, from, query.at, to],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((image_id, observation_utc)) = frame else {
        return Ok(Json(json!({"stars": [], "image_id": null})));
    };
    // Full-resolution dimensions, so the overlay lines up with the image the
    // browser fetches rather than with a padded bounding box of the stars.
    let size: Option<(i64, i64)> = conn
        .query_row(
            "SELECT width,height FROM images WHERE id=?1",
            [&image_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    // The brightest sky this frame shows, by which a missing star is either
    // cloud or a full detector. Same test the depth pass applies.
    let washed_out: bool = conn
        .query_row(
            "SELECT COALESCE(max(background),0) > ?3 FROM star_photometry
             WHERE source_id=?1 AND image_id=?2 AND amplitude IS NOT NULL",
            rusqlite::params![id, image_id, extinction::WASHED_OUT_BACKGROUND],
            |r| r.get(0),
        )
        .unwrap_or(false);
    let mut statement = conn
        .prepare(
            "SELECT p.star_key,p.vt_mag,p.predicted_x,p.predicted_y,p.centroid_x,p.centroid_y,
                    p.elevation_deg,p.flux,p.flux_snr,p.amplitude,m.best,p.background,
                    p.optical_depth,p.amplitude_snr
             FROM star_photometry p
             LEFT JOIN (SELECT star_key,max(flux) AS best FROM star_photometry
                        WHERE source_id=?1 AND channel=?2 AND amplitude IS NOT NULL
                          AND flux_snr >= 5
                          AND julianday(observation_utc) >= julianday(?3)
                          AND julianday(observation_utc) < julianday(?5)
                        GROUP BY star_key) m ON m.star_key=p.star_key
             WHERE p.source_id=?1 AND p.channel=?2 AND p.image_id=?4
             ORDER BY p.vt_mag",
        )
        .map_err(internal)?;
    let rows = statement
        .query_map(
            rusqlite::params![id, channel, from, image_id, to],
            |r| {
                let predicted: (Option<f64>, Option<f64>) = (r.get(2)?, r.get(3)?);
                let centroid: (Option<f64>, Option<f64>) = (r.get(4)?, r.get(5)?);
                let flux: Option<f64> = r.get(7)?;
                let vt_mag: f64 = r.get(1)?;
                let elevation: Option<f64> = r.get(6)?;
                let best: Option<f64> = r.get(10)?;
                let detected: Option<f64> = r.get(9)?;
                // Where to draw it: where the fit found it when it was found,
                // otherwise where the lens model said to look.
                let (x, y) = match centroid {
                    (Some(x), Some(y)) => (Some(x), Some(y)),
                    _ => predicted,
                };
                Ok(json!({
                    "star_key": r.get::<_, String>(0)?,
                    "vt_mag": vt_mag,
                    "x": x,
                    "y": y,
                    "elevation_deg": elevation,
                    "flux": flux,
                    "flux_snr": r.get::<_, Option<f64>>(8)?,
                    "detected": detected.is_some(),
                    "best_flux": best,
                    // The peak reached the top of the range, so the profile is
                    // clipped and this flux is an underestimate. Aurora causes
                    // it by lifting the background out from under the stars.
                    "optical_depth": r.get::<_, Option<f64>>(12)?,
                    // A star bright enough and high enough that this camera
                    // should see it whenever the sky is clear. Its absence is
                    // the strongest cloud signal there is, unless the frame is
                    // washed out, when the detector explains it instead.
                    "certain": vt_mag < extinction::CERTAIN_MAGNITUDE
                        && elevation.is_some_and(|e| e > extinction::CERTAIN_ELEVATION_DEG),
                    "usable": detected.is_some()
                        && r.get::<_, Option<f64>>(8)?.is_some_and(|s| s >= extinction::MIN_DEPTH_SNR)
                        && r.get::<_, Option<f64>>(13)?.is_some_and(|s| s >= extinction::MIN_DEPTH_SNR),
                    "washed_out": washed_out,
                    "saturated": match (detected, r.get::<_, Option<f64>>(11)?) {
                        (Some(amplitude), Some(background)) => {
                            amplitude + background >= starphot::SATURATION_LEVEL
                        }
                        _ => false,
                    },
                    // Fraction of this star's own best in the window. Null when
                    // the star was not found, which the client draws as absent
                    // rather than as dark.
                    "relative": match (flux, best) {
                        (Some(f), Some(b)) if b > 0.0 && detected.is_some() => {
                            Some((f / b).clamp(0.0, 1.0))
                        }
                        _ => None,
                    },
                }))
            },
        )
        .map_err(internal)?;
    let stars = rows.collect::<Result<Vec<_>, _>>().map_err(internal)?;
    // The lens model that was used to look for these stars, so the client can
    // cut the tessellation to the sky this camera actually sees.
    let field = match size {
        Some((w, h)) => conn
            .query_row(
                "SELECT c.hdf5_path FROM star_photometry p \
                 JOIN sources s ON s.id=p.source_id JOIN calibrations c ON c.source_id=s.id \
                 WHERE p.source_id=?1 AND p.image_id=?2 \
                 ORDER BY c.created_utc DESC LIMIT 1",
                rusqlite::params![id, image_id],
                |r| r.get::<_, String>(0),
            )
            .ok()
            .and_then(|path| field_outline(&path, w as f64, h as f64)),
        None => None,
    };
    Ok(Json(json!({
        "image_id": image_id,
        "observation_utc": observation_utc,
        "width": size.map(|(w, _)| w),
        "height": size.map(|(_, h)| h),
        "field": field,
        "stars": stars,
    })))
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
    let (from, to) = star_window(&query.from, &query.to, query.hours);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut statement = conn
        .prepare(
            "SELECT star_key,vt_mag,ra_hours_j2000,dec_deg_j2000,predicted_x,predicted_y,
                    elevation_deg,flux,background,residual_std,flux_snr
             FROM star_photometry
             WHERE source_id=?1 AND channel=?2
               AND julianday(observation_utc) >= julianday(?3)
               AND julianday(observation_utc) < julianday(?4)
             ORDER BY star_key,observation_utc",
        )
        .map_err(internal)?;
    let rows = statement
        .query_map(
            rusqlite::params![id, channel, from, to],
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
    Ok(Json(json!({"source_id":id,"channel":channel,"from":from,"to":to,"stars":stars})))
}

/// The brightness and background time series of one star, for plotting.
#[derive(Deserialize)]
struct KeogramQuery {
    /// The partner camera. The camera the panel is already showing supplies the
    /// other half of the pair.
    partner: String,
    from: Option<String>,
    to: Option<String>,
    hours: Option<f64>,
    samples: Option<usize>,
    cadence_seconds: Option<i64>,
    half_width_km: Option<f64>,
}

#[derive(Deserialize)]
struct StarSensitivityQuery {
    partner: String,
    channel: Option<String>,
    from: Option<String>,
    to: Option<String>,
    hours: Option<f64>,
    /// Include the individual pairs, not just the summary. Off by default,
    /// because a long window pairs tens of thousands of measurements.
    detail: Option<bool>,
    /// How far apart two measurements may be. See starpair for why this is
    /// far looser than the keogram's, and what it costs.
    max_skew_seconds: Option<f64>,
}

/// Relative sensitivity of two cameras from stars they both measured.
///
/// The star's own brightness cancels in the ratio, so this needs no catalogue
/// magnitude and no photometric zero point -- only that both cameras saw the
/// same star at the same moment through clear sky. See `starpair`.
async fn source_star_sensitivity(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<StarSensitivityQuery>,
) -> ApiResult<Json<Value>> {
    let channel = query.channel.unwrap_or_else(|| "mean".into());
    if !starphot::CHANNELS.contains(&channel.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown channel".into()));
    }
    let (from, to) = star_window(&query.from, &query.to, query.hours);
    let detail = query.detail.unwrap_or(false);
    let built = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        let conn = db::open(&s.db_path)?;
        let a = keogram::station(&conn, &id)?;
        let b = keogram::station(&conn, &query.partner)?;
        let skew_limit = query
            .max_skew_seconds
            .unwrap_or(starpair::DEFAULT_MAX_SKEW_SECONDS)
            .clamp(1.0, 3600.0);
        let pairs = starpair::pairs(
            &conn, &a.id, &b.id, a.longitude_deg, b.longitude_deg, &channel, &from, &to,
            skew_limit,
        )?;
        let mut skews: Vec<f64> = pairs.iter().map(|p| p.skew_seconds).collect();
        skews.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
        let median_skew = skews.get(skews.len() / 2).copied();
        let estimate = starpair::estimate(&pairs);
        let stars: std::collections::BTreeSet<&str> =
            pairs.iter().map(|p| p.star_key.as_str()).collect();
        Ok(json!({
            "a": {"id": a.id, "name": a.name},
            "b": {"id": b.id, "name": b.name},
            "channel": channel, "from": from, "to": to,
            "separation_km": keogram::separation_km(&a, &b),
            "pairs": pairs.len(),
            "stars": stars.len(),
            "median_skew_seconds": median_skew,
            "estimate": estimate.as_ref().map(|e| e.to_json()),
            "gates": {
                "max_skew_seconds": skew_limit,
                "min_flux_snr": starpair::MIN_FLUX_SNR,
                "min_elevation_deg": starpair::MIN_ELEVATION_DEG,
                "clear_frame_fading": starpair::CLEAR_FRAME_FADING,
                "min_pairs": starpair::MIN_PAIRS,
            },
            "method": "ln(F_A/F_B) = ln(G_A/G_B) - beta (k_A X_A - k_B X_B) - (tau_A - tau_B); \
the star's own flux cancels, so no catalogue magnitude or photometric zero point is used",
            "caveat": "Gain only: a point source says nothing about the background, which the \
paired keograms measure. Thin cloud over one station and not the other biases the ratio, so \
frames are gated on the rest of their stars and the estimator is a median.",
            "samples": if detail {
                json!(pairs.iter().map(|p| json!({
                    "star_key": p.star_key, "at": p.at,
                    "flux_a": p.flux_a, "flux_b": p.flux_b,
                    "elevation_a": p.elevation_a, "elevation_b": p.elevation_b,
                    "airmass_a": p.airmass_a, "airmass_b": p.airmass_b,
                    "ln_ratio": p.ln_ratio, "corrected_ln_ratio": p.corrected,
                    "skew_seconds": p.skew_seconds,
                })).collect::<Vec<_>>())
            } else { Value::Null },
        }))
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(built))
}

#[derive(Deserialize)]
struct ResponseSurfaceQuery {
    channel: Option<String>,
    /// "flux", "amplitude" or "peak". Comma-separated for several at once, so
    /// the operator can see whether they agree.
    measure: Option<String>,
    from: Option<String>,
    to: Option<String>,
    hours: Option<f64>,
    clear_threshold: Option<f64>,
    /// Compare against this camera's surfaces as well.
    partner: Option<String>,
}

/// The assimilated clear-sky stellar response of one camera.
///
/// Built from every clear, unsaturated measurement rather than from moments
/// when two cameras happened to look at once. Expect it to be thin for a long
/// while: at these latitudes darkness, moon and cloud multiply, so the coverage
/// figures matter as much as the values.
async fn source_response_surface(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ResponseSurfaceQuery>,
) -> ApiResult<Json<Value>> {
    let channel = query.channel.unwrap_or_else(|| "mean".into());
    if !starphot::CHANNELS.contains(&channel.as_str()) {
        return Err((StatusCode::BAD_REQUEST, "unknown channel".into()));
    }
    let names = query.measure.unwrap_or_else(|| "flux,peak".into());
    let measures: Vec<response::Measure> = names
        .split(',')
        .filter_map(|n| response::Measure::parse(n.trim()))
        .collect();
    if measures.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "measure must be flux, amplitude or peak".into()));
    }
    let (from, to) = star_window(&query.from, &query.to, query.hours);
    let clear = query.clear_threshold.unwrap_or(0.70).clamp(0.0, 1.0);
    let built = tokio::task::spawn_blocking(move || -> anyhow::Result<Value> {
        let conn = db::open(&s.db_path)?;
        let here = keogram::station(&conn, &id)?;
        let partner = query
            .partner
            .as_deref()
            .map(|p| keogram::station(&conn, p))
            .transpose()?;
        let mut surfaces = serde_json::Map::new();
        for measure in &measures {
            let samples = response::gather(
                &conn, &here.id, &channel, *measure, &from, &to,
                here.longitude_deg, clear,
            )?;
            let mine = response::solve(&samples);
            let mut entry = json!({
                "gathered_samples": samples.len(),
                "surface": mine.as_ref().map(|v| v.to_json()),
            });
            if let (Some(mine), Some(other)) = (mine.as_ref(), partner.as_ref()) {
                let theirs = response::gather(
                    &conn, &other.id, &channel, *measure, &from, &to,
                    other.longitude_deg, clear,
                )?;
                let theirs = response::solve(&theirs);
                if let Some(theirs) = theirs.as_ref() {
                    entry["partner_surface"] = theirs.to_json();
                    if let Some((centre, scatter, shared)) = response::ratio(mine, theirs) {
                        entry["shape_difference"] = json!({
                            "shared_cells": shared,
                            "median_ln_difference": centre,
                            "scatter": scatter,
                            "note": "difference in shape only: each surface is pinned to its own \
median, so a constant sensitivity ratio cancels here and has to come from a shared reference",
                        });
                    }
                }
            }
            surfaces.insert(measure.name().to_string(), entry);
        }
        Ok(json!({
            "source_id": here.id, "source_name": here.name,
            "partner_id": partner.as_ref().map(|p| p.id.clone()),
            "channel": channel, "from": from, "to": to,
            "clear_threshold": clear,
            "azimuth_bins": response::AZIMUTH_BINS,
            "elevation_bins": response::ELEVATION_BINS,
            "minimum": {
                "elevation_deg": response::MIN_ELEVATION_DEG,
                "bin_samples": response::MIN_BIN_SAMPLES,
                "bin_nights": response::MIN_BIN_NIGHTS,
                "star_samples": response::MIN_STAR_SAMPLES,
            },
            "measures": surfaces,
            "note": "Clear-sky stellar response by azimuth and elevation, each star's own level \
profiled out. Accumulates over many nights: at these latitudes darkness, moon and cloud \
multiply, so expect at least one new-moon period and more likely several before the coverage \
is worth trusting.",
        }))
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(built))
}

/// Which cameras this one can be compared against.
async fn source_keogram_pairs(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let here = keogram::station(&conn, &id)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    let pairs = keogram::pairs(&conn, &id).map_err(internal)?;
    Ok(Json(json!({
        "source_id": here.id,
        "source_name": here.name,
        "maximum_separation_km": keogram::MAX_SEPARATION_KM,
        "pairs": pairs,
    })))
}

/// The keogram pair itself. This decodes one texture per camera per row, so it
/// is deliberately bounded rather than fast: see `keogram::MAX_ROWS`.
async fn source_keogram(
    Path(id): Path<String>,
    State(s): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<KeogramQuery>,
) -> ApiResult<Json<Value>> {
    let (from, to) = star_window(&query.from, &query.to, query.hours);
    let built = tokio::task::spawn_blocking(move || {
        keogram::build(
            &s,
            &id,
            &query.partner,
            &from,
            &to,
            query.samples.unwrap_or(keogram::DEFAULT_SAMPLES),
            query.cadence_seconds.unwrap_or(keogram::DEFAULT_CADENCE_SECONDS),
            query.half_width_km.unwrap_or(keogram::DEFAULT_HALF_WIDTH_KM),
        )
    })
    .await
    .map_err(internal)?
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok(Json(built))
}

/// One star's measurements over a window, as the panel wants them.
///
/// Every field is read by column name rather than by position. The positional
/// form has now twice silently handed back a neighbouring column -- most
/// recently the sun's elevation in place of the optical depth, which looks
/// exactly like a plausible depth and so survived review. A name cannot
/// slide when a column is inserted in the middle of the list.
fn star_series_samples(
    conn: &rusqlite::Connection,
    source_id: &str,
    star: &str,
    channel: &str,
    from: &str,
    to: &str,
) -> rusqlite::Result<Vec<Value>> {
    let mut statement = conn.prepare(
        "SELECT p.observation_utc,p.flux,p.background,p.amplitude,p.sigma_major,p.sigma_minor,
                p.angle_deg,p.centroid_offset_px,p.elevation_deg,p.azimuth_deg,
                p.predicted_x,p.predicted_y,p.centroid_x,p.centroid_y,p.rms_residual,
                p.residual_std,p.amplitude_snr,p.flux_snr,
                p.background_dx,p.background_dy,p.background_dxy,p.optical_depth,
                s.moon_elevation_deg,s.moon_illuminated_fraction,s.moon_sky_brightness,
                s.moon_apparent_magnitude,s.sun_elevation_deg
         FROM star_photometry p
         LEFT JOIN frame_sky s ON s.source_id=p.source_id AND s.image_id=p.image_id
         WHERE p.source_id=?1 AND p.star_key=?2 AND p.channel=?3
           AND julianday(p.observation_utc) >= julianday(?4)
           AND julianday(p.observation_utc) < julianday(?5)
         ORDER BY p.observation_utc",
    )?;
    let rows = statement.query_map(
        rusqlite::params![source_id, star, channel, from, to],
        |r| {
            // Nullable everywhere but the timestamp: a measurement that failed
            // to fit still has a row, and the panel draws the gap.
            let number = |name: &str| r.get::<_, Option<f64>>(name);
            Ok(json!({
                "at": r.get::<_,String>("observation_utc")?,
                "flux": number("flux")?,
                "background": number("background")?,
                "amplitude": number("amplitude")?,
                "sigma_major": number("sigma_major")?,
                "sigma_minor": number("sigma_minor")?,
                "angle_deg": number("angle_deg")?,
                "centroid_offset_px": number("centroid_offset_px")?,
                "elevation_deg": number("elevation_deg")?,
                "azimuth_deg": number("azimuth_deg")?,
                "predicted_x": number("predicted_x")?,
                "predicted_y": number("predicted_y")?,
                "centroid_x": number("centroid_x")?,
                "centroid_y": number("centroid_y")?,
                "rms_residual": number("rms_residual")?,
                "residual_std": number("residual_std")?,
                "amplitude_snr": number("amplitude_snr")?,
                "flux_snr": number("flux_snr")?,
                "background_dx": number("background_dx")?,
                "background_dy": number("background_dy")?,
                "background_dxy": number("background_dxy")?,
                // Null wherever no depth could honestly be claimed: the
                // night was not fitted, or this measurement was saturated,
                // undetectable, or the star was never found.
                "optical_depth": number("optical_depth")?,
                "moon_elevation_deg": number("moon_elevation_deg")?,
                "moon_illuminated_fraction": number("moon_illuminated_fraction")?,
                "moon_sky_brightness": number("moon_sky_brightness")?,
                "moon_apparent_magnitude": number("moon_apparent_magnitude")?,
                "sun_elevation_deg": number("sun_elevation_deg")?,
            }))
        },
    )?;
    rows.collect()
}

#[cfg(test)]
mod star_series_tests {
    use super::star_series_samples;

    /// The photometry tables reference sources and images; these tests exercise
    /// the query in isolation, without the rest of the archive.
    fn seeded() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn
    }

    /// Every column gets a value nothing else has, so reading a neighbouring
    /// one cannot look right. This is the test that was missing when the
    /// handler returned the sun's elevation as the optical depth: -15.1 is a
    /// perfectly believable depth, and only a sentinel catches it.
    #[test]
    fn every_field_comes_from_its_own_column() {
        let conn = seeded();
        conn.execute(
            "INSERT INTO star_photometry(
                source_id,image_id,observation_utc,star_key,channel,
                ra_hours_j2000,dec_deg_j2000,vt_mag,azimuth_deg,elevation_deg,
                predicted_x,predicted_y,centroid_x,centroid_y,centroid_offset_px,
                background,amplitude,sigma_major,sigma_minor,angle_deg,
                flux,rms_residual,residual_std,amplitude_snr,flux_snr,
                background_dx,background_dy,background_dxy,optical_depth)
             VALUES('cam','img','2026-09-12T20:33:00+00:00','star','mean',
                1.0,2.0,3.0,110.0,120.0,
                130.0,140.0,150.0,160.0,170.0,
                180.0,190.0,200.0,210.0,220.0,
                230.0,240.0,250.0,260.0,270.0,
                280.0,290.0,300.0,310.0)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO frame_sky(
                source_id,image_id,observation_utc,sun_elevation_deg,moon_azimuth_deg,
                moon_elevation_deg,moon_illuminated_fraction,moon_phase_angle_deg,
                moon_distance_km,moon_apparent_magnitude,moon_sky_brightness)
             VALUES('cam','img','2026-09-12T20:33:00+00:00',410.0,420.0,
                430.0,440.0,450.0,460.0,470.0,480.0)",
            [],
        )
        .unwrap();

        let rows = star_series_samples(
            &conn, "cam", "star", "mean", "2026-09-12T00:00:00+00:00",
            "2026-09-13T00:00:00+00:00",
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        let s = &rows[0];
        assert_eq!(s["at"], "2026-09-12T20:33:00+00:00");
        for (field, want) in [
            ("azimuth_deg", 110.0), ("elevation_deg", 120.0),
            ("predicted_x", 130.0), ("predicted_y", 140.0),
            ("centroid_x", 150.0), ("centroid_y", 160.0),
            ("centroid_offset_px", 170.0), ("background", 180.0),
            ("amplitude", 190.0), ("sigma_major", 200.0),
            ("sigma_minor", 210.0), ("angle_deg", 220.0),
            ("flux", 230.0), ("rms_residual", 240.0),
            ("residual_std", 250.0), ("amplitude_snr", 260.0),
            ("flux_snr", 270.0), ("background_dx", 280.0),
            ("background_dy", 290.0), ("background_dxy", 300.0),
            ("optical_depth", 310.0),
            ("sun_elevation_deg", 410.0), ("moon_elevation_deg", 430.0),
            ("moon_illuminated_fraction", 440.0),
            ("moon_apparent_magnitude", 470.0), ("moon_sky_brightness", 480.0),
        ] {
            assert_eq!(s[field].as_f64(), Some(want), "{field} read the wrong column");
        }
    }

    /// A frame with no sky row still yields its photometry, with the sky
    /// fields null rather than the row vanishing.
    #[test]
    fn a_frame_without_a_sky_row_still_returns_its_measurement() {
        let conn = seeded();
        conn.execute(
            "INSERT INTO star_photometry(
                source_id,image_id,observation_utc,star_key,channel,
                ra_hours_j2000,dec_deg_j2000,vt_mag,azimuth_deg,elevation_deg,
                predicted_x,predicted_y,flux)
             VALUES('cam','img','2026-09-12T20:33:00+00:00','star','mean',
                1.0,2.0,3.0,110.0,120.0,130.0,140.0,230.0)",
            [],
        )
        .unwrap();
        let rows = star_series_samples(
            &conn, "cam", "star", "mean", "2026-09-12T00:00:00+00:00",
            "2026-09-13T00:00:00+00:00",
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["flux"].as_f64(), Some(230.0));
        assert!(rows[0]["sun_elevation_deg"].is_null());
        assert!(rows[0]["optical_depth"].is_null());
    }
}

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
    let (from, to) = star_window(&query.from, &query.to, query.hours);
    let conn = db::open(&s.db_path).map_err(internal)?;
    let samples =
        star_series_samples(&conn, &id, &star, &channel, &from, &to).map_err(internal)?;
    let fluxes: Vec<f64> = samples
        .iter()
        .filter_map(|v| v["flux"].as_f64())
        .collect();
    Ok(Json(json!({
        "source_id": id, "star_key": star, "channel": channel, "from": from, "to": to,
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
        .route("/api/events/{event}/projection-manifest", get(event_study::projection_manifest))
        .route("/api/events/{event}/assets/{name}", get(event_study::projection_asset))
        .route("/api/events/{event}/calibrations", post(event_study::calibration))
        .route("/api/events/{event}/records/{record}/image", get(event_study::record_image))
        .route("/api/events/{event}/records/{record}/settings", get(event_study::settings).post(event_study::save_settings))
        .route("/api/sources/{id}/stars", get(source_stars))
        .route("/api/sources/{id}/stars/series", get(source_star_series))
        .route("/api/sources/{id}/calibration/drift", get(source_calibration_drift))
        .route("/api/sources/{id}/calibration/refit", post(source_calibration_refit))
        .route("/api/sources/{id}/calibration/refit-proposal", get(source_refit_proposal))
        .route("/api/sources/{id}/keogram-pairs", get(source_keogram_pairs))
        .route("/api/sources/{id}/keogram", get(source_keogram))
        .route("/api/sources/{id}/star-sensitivity", get(source_star_sensitivity))
        .route("/api/sources/{id}/response-surface", get(source_response_surface))
        .route("/api/sources/{id}/stars/frame", get(source_star_frame))
        .route("/api/sources/{id}/stars/nights", get(source_star_nights))
        .route("/api/extinction", get(extinction_state))
        .route("/api/sources/{id}/stars/distribution", get(source_star_distribution))
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
