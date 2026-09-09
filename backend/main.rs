mod archive;
mod crawler;
mod norsk_meteor;
mod meteor_backfill;
mod db;
mod geometry;
mod igrf_grid;
mod model;
mod quality;
mod projection;
mod publish;
mod pixel_mask;
mod image_time;
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
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
        .query_row("SELECT count(*) FROM sources WHERE enabled=1", [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM images WHERE downloaded_utc >= datetime('now','-1 day')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let latest: Option<String> = conn
        .query_row("SELECT max(observation_utc) FROM images", [], |r| r.get(0))
        .unwrap_or(None);
    let mosaic: Option<String> = conn
        .query_row("SELECT max(observation_utc) FROM mosaics", [], |r| r.get(0))
        .unwrap_or(None);
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
        ],
    }))
}

async fn credits(State(s): State<AppState>) -> ApiResult<Json<Vec<Value>>> {
    let conn=db::open(&s.db_path).map_err(internal)?;
    let mut query=conn.prepare("SELECT name,website,acknowledgement,copyright FROM producers ORDER BY name").map_err(internal)?;
    let rows=query.query_map([],|r|Ok(json!({"name":r.get::<_,String>(0)?,"website":r.get::<_,Option<String>>(1)?,"acknowledgement":r.get::<_,String>(2)?,"copyright":r.get::<_,String>(3)?}))).map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>,_>>().map_err(internal)?))
}

async fn sources(State(s): State<AppState>) -> ApiResult<Json<Vec<SourceStatus>>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let mut q=conn.prepare("SELECT s.id,s.name,p.name,s.timestamp_mode,s.last_success_utc,s.last_error,(SELECT max(observation_utc) FROM images i WHERE i.source_id=s.id),(SELECT max(downloaded_utc) FROM images i WHERE i.source_id=s.id),(SELECT count(*) FROM images i WHERE i.source_id=s.id AND i.downloaded_utc >= datetime('now','-1 day')),s.latitude_deg,s.longitude_deg,EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id),s.enabled FROM sources s JOIN producers p ON p.id=s.producer_id ORDER BY s.name").map_err(internal)?;
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
    Ok(Json(rows.filter_map(Result::ok).collect()))
}

async fn history(State(s):State<AppState>)->ApiResult<Json<Vec<String>>>{
    let conn=db::open(&s.db_path).map_err(internal)?;
    let mut q=conn.prepare("SELECT DISTINCT strftime('%Y-%m-%dT%H:%M:00Z',i.observation_utc) AS minute FROM images i JOIN sources s ON s.id=i.source_id WHERE s.enabled=1 AND EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) AND julianday(i.observation_utc)>=julianday('now','-1 day') ORDER BY minute").map_err(internal)?;
    let rows=q.query_map([],|r|r.get::<_,String>(0)).map_err(internal)?;
    Ok(Json(rows.collect::<Result<Vec<_>,_>>().map_err(internal)?))
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

#[derive(Deserialize)]
struct CameraSettingsInput {
    crop: Option<Value>,
    mask: Option<Value>,
}
async fn get_camera_settings(
    Path(id): Path<String>,
    State(s): State<AppState>,
) -> ApiResult<Json<Value>> {
    let conn = db::open(&s.db_path).map_err(internal)?;
    let row = conn.query_row(
        "SELECT c.crop_json,c.mask_json FROM sources s LEFT JOIN camera_settings c ON c.source_id=s.id WHERE s.id=?1",
        [&id],
        |r| Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?)),
    ).map_err(|e| if matches!(e,rusqlite::Error::QueryReturnedNoRows) {
        (StatusCode::NOT_FOUND,"camera not found".into())
    } else { internal(e) })?;
    let parse = |text:Option<String>| -> ApiResult<Value> {
        text.map(|t|serde_json::from_str(&t).map_err(internal)).unwrap_or(Ok(Value::Null))
    };
    Ok(Json(json!({"crop":parse(row.0)?,"mask":parse(row.1)?})))
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
    conn.execute("INSERT INTO camera_settings(source_id,updated_utc,crop_json,mask_json) VALUES(?1,?2,?3,?4) ON CONFLICT(source_id) DO UPDATE SET updated_utc=excluded.updated_utc,crop_json=excluded.crop_json,mask_json=excluded.mask_json",rusqlite::params![id,Utc::now().to_rfc3339(),input.crop.map(|v|v.to_string()),input.mask.map(|v|v.to_string())]).map_err(internal)?;
    Ok(Json(json!({"state":"saved"})))
}

#[derive(Deserialize)]
struct ProjectionQuery { at:Option<String>,format:Option<String> }
async fn projected_image(Path(id):Path<String>,State(s):State<AppState>,axum::extract::Query(query):axum::extract::Query<ProjectionQuery>,headers:axum::http::HeaderMap)->ApiResult<axum::response::Response> {
    let at=query.at.map(|t|DateTime::parse_from_rfc3339(&t).map(|v|v.with_timezone(&Utc)).map_err(|_|(StatusCode::BAD_REQUEST,"invalid frame time".into()))).transpose()?;
    if matches!(query.format.as_deref(),Some("assets"|"timeline")){
        let timeline=query.format.as_deref()==Some("timeline");
        let p=tokio::task::spawn_blocking(move||if timeline{projection::timeline(&s,&id)}else{projection::assets(&s,&id,at)}).await.map_err(internal)?.map_err(|e|if e.to_string().contains("Query returned no rows"){(StatusCode::NOT_FOUND,"no historical frame".into())}else{internal(e)})?;
        return Ok(([(header::CACHE_CONTROL,"no-store")],Json(p)).into_response());
    }
    let p=tokio::task::spawn_blocking(move||projection::build(&s,&id,at)).await.map_err(internal)?.map_err(|e|if e.to_string().contains("Query returned no rows"){(StatusCode::NOT_FOUND,"no frame within ten minutes before selected time".into())}else{internal(e)})?;
    let mut response=if headers.get(header::ACCEPT).and_then(|v|v.to_str().ok())==Some("application/octet-stream"){
        let mut bytes=Vec::with_capacity(p.vertices.len()*4);
        for value in &p.vertices {bytes.extend_from_slice(&value.to_le_bytes());}
        ([(header::CONTENT_TYPE,"application/octet-stream")],bytes).into_response()
    }else{Json(p).into_response()};
    response.headers_mut().insert(header::CACHE_CONTROL,axum::http::HeaderValue::from_static("no-store"));
    Ok(response)
}
async fn projection_asset(Path(name):Path<String>,State(s):State<AppState>)->ApiResult<axum::response::Response>{
    let Some((key,ext))=name.rsplit_once('.')else{return Err((StatusCode::BAD_REQUEST,"invalid asset".into()))};
    if key.len()!=64||!key.bytes().all(|b|b.is_ascii_hexdigit())||!matches!(ext,"bin"|"png"){return Err((StatusCode::BAD_REQUEST,"invalid asset".into()))}
    let bytes=tokio::fs::read(s.archive_root.join("projection-cache").join(&name)).await.map_err(|_|(StatusCode::NOT_FOUND,"asset not ready".into()))?;
    Ok(([(header::CONTENT_TYPE,if ext=="png"{"image/png"}else{"application/octet-stream"}),(header::CACHE_CONTROL,"public, max-age=31536000, immutable")],bytes).into_response())
}
async fn image_texture(Path(id):Path<String>,State(s):State<AppState>)->ApiResult<axum::response::Response>{
    let bytes=tokio::task::spawn_blocking(move||projection::image_texture(&s,&id)).await.map_err(internal)?.map_err(internal)?;
    Ok(([(header::CONTENT_TYPE,"image/png"),(header::CACHE_CONTROL,"public, max-age=31536000, immutable")],bytes).into_response())
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

async fn calibration(
    State(s): State<AppState>,
    mut mp: Multipart,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let mut source_id = None;
    let mut valid_from = None;
    let mut residual_px = None;
    let mut submitter = None;
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
    conn.execute("INSERT INTO calibrations(id,source_id,created_utc,valid_from_utc,method,hdf5_path,residual_px,submitted_by) VALUES(?1,?2,?3,?4,'AIDA/WISC',?5,?6,?7)",rusqlite::params![id,source,Utc::now().to_rfc3339(),valid_from,path.to_string_lossy(),residual_px,submitter]).map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(json!({"id":id,"state":"calibrated","source_id":source})),
    ))
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
        let date = std::env::args().nth(index + 1).ok_or_else(|| anyhow::anyhow!("--backfill-nmn requires YYYY-MM-DD (night starting that UTC date)"))?;
        return meteor_backfill::run(&source_configs, &db_path, &archive_root, &date).await;
    }

    if std::env::args().any(|arg|arg=="--publish") { return publish::run(&state); }
    // Stage a migrated archive without running two collectors against providers.
    if std::env::var("GAIA_CRAWLER_ENABLED").as_deref() != Ok("0") {
        tokio::spawn(crawler::run_loop(source_configs, db_path, archive_root));
    } else {
        tracing::info!("Crawler disabled for staging/read-only operation");
    }
    let static_dir = std::env::var("GAIA_STATIC_DIR").unwrap_or_else(|_| "web-dist".into());
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/status", get(status))
        .route("/api/sources", get(sources))
        .route("/api/sources/{id}/latest", get(latest_image))
        .route("/api/sources/{id}/projection", get(projected_image))
        .route("/api/igrf-maglat", get(igrf_maglat))
        .route("/api/suggestions", post(suggest))
        .route("/api/sources/{id}/location", post(set_location))
        .route("/api/sources/{id}/enabled", post(set_enabled))
        .route("/api/sources/{id}/settings", get(get_camera_settings).post(camera_settings))
        .route("/api/credits", get(credits))
        .route("/api/history", get(history))
        .route("/api/projection-assets/{name}", get(projection_asset))
        .route("/api/images/{id}/texture", get(image_texture))
        .route("/api/calibrations", post(calibration))
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
