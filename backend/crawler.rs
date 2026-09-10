use crate::{
    archive, db,
    model::{SourceConfig, SourceKind, TimestampMode},
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, NaiveDateTime, Utc};
use regex::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub fn load_sources(path: &Path) -> Result<Vec<SourceConfig>> {
    if path.is_file() {
        return Ok(serde_json::from_slice(&std::fs::read(path)?)?);
    }
    let mut files = std::fs::read_dir(path)
        .with_context(|| format!("read source directory {}", path.display()))?
        .filter_map(|e| e.ok().map(|v| v.path()))
        .filter(|p| p.extension().and_then(|v| v.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    files.sort();
    let mut all = Vec::new();
    for file in files {
        let mut entries: Vec<SourceConfig> = serde_json::from_slice(&std::fs::read(&file)?)
            .with_context(|| format!("parse {}", file.display()))?;
        all.append(&mut entries)
    }
    let mut ids = std::collections::HashSet::new();
    for source in &all {
        if !ids.insert(&source.id) {
            return Err(anyhow!("duplicate source id {}", source.id));
        }
    }
    Ok(all)
}

fn expand_url(url: &str, now: DateTime<Utc>) -> String {
    url.replace("{YYYY}", &now.format("%Y").to_string())
        .replace("{MM}", &now.format("%m").to_string())
        .replace("{DD}", &now.format("%d").to_string())
        .replace("{HH}", &now.format("%H").to_string())
}

fn parse_timestamp(
    source: &SourceConfig,
    url: &str,
    downloaded: DateTime<Utc>,
) -> Result<(DateTime<Utc>, &'static str)> {
    if matches!(source.timestamp_mode, TimestampMode::DownloadTime) {
        return Ok((downloaded, "download_time"));
    }
    let re = Regex::new(
        source
            .timestamp_regex
            .as_deref()
            .ok_or_else(|| anyhow!("archive source needs timestamp_regex"))?,
    )?;
    let caps = re
        .captures(url)
        .ok_or_else(|| anyhow!("no timestamp in {url}"))?;
    let raw = caps
        .name("timestamp")
        .map(|m| m.as_str())
        .or_else(|| caps.get(1).map(|m| m.as_str()))
        .ok_or_else(|| anyhow!("timestamp_regex needs a capture"))?;
    let fmt = source
        .timestamp_format
        .as_deref()
        .unwrap_or("%Y-%m-%dT%H.%M.%S%.3f");
    let naive = NaiveDateTime::parse_from_str(raw, fmt)?;
    Ok((naive.and_utc(), "source_filename"))
}

fn parse_http_or_rfc3339_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .or_else(|_| DateTime::parse_from_rfc2822(value))
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

async fn candidates(client: &Client, source: &SourceConfig) -> Result<Vec<String>> {
    if source.darkness_sun_altitude_deg.is_some_and(|threshold| {
        crate::norsk_meteor::solar_altitude_deg(source, Utc::now())
            .is_some_and(|altitude| altitude > threshold)
    }) {
        return Ok(vec![]);
    }
    let url = expand_url(&source.url, Utc::now());
    if matches!(source.kind, SourceKind::NorskMeteor) {
        return crate::norsk_meteor::candidates(client, source, Utc::now()).await;
    }
    if matches!(source.kind, SourceKind::SnapshotUrl | SourceKind::Push) {
        return Ok(vec![url]);
    }
    let mut request = client.get(&url);
    for (k, v) in &source.request_headers {
        request = request.header(k, v)
    }
    let response = request.send().await?.error_for_status()?;
    if matches!(source.kind, SourceKind::JsonFeed) {
        let value: serde_json::Value = response.json().await?;
        let items = value
            .pointer(source.json_items_pointer.as_deref().unwrap_or("/"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow!("json items pointer is not an array"))?;
        let field = source.json_url_field.as_deref().unwrap_or("url");
        return Ok(items
            .iter()
            .filter_map(|v| v.get(field)?.as_str().map(String::from))
            .collect());
    }
    let body = response.text().await?;
    let doc = Html::parse_document(&body);
    let selector = Selector::parse("a[href], img[src]").unwrap();
    let filter = Regex::new(
        source
            .image_link_regex
            .as_deref()
            .unwrap_or(r"(?i)\.(jpe?g|png|webp)$"),
    )?;
    let base = reqwest::Url::parse(&url)?;
    let mut out = Vec::new();
    for item in doc.select(&selector) {
        if let Some(href) = item
            .value()
            .attr("href")
            .or_else(|| item.value().attr("src"))
        {
            if filter.is_match(href) {
                if let Ok(u) = base.join(href) {
                    out.push(u.to_string())
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub async fn crawl_once(
    source: &SourceConfig,
    client: &Client,
    db_path: &Path,
    archive_root: &Path,
) -> Result<(usize, usize, usize)> {
    let urls = candidates(client, source).await?;
    archive_urls(source, client, db_path, archive_root, urls).await
}

pub async fn archive_urls(source: &SourceConfig, client: &Client, db_path: &Path,
    archive_root: &Path, urls: Vec<String>) -> Result<(usize, usize, usize)> {
    let discovered = urls.len();
    let mut downloaded_count = 0;
    let mut duplicates = 0;
    // newest candidates last in archive indexes; a bounded pass prevents accidental mass download on first run
    for url in urls
        .into_iter()
        .rev()
        .take(if matches!(source.kind, SourceKind::HtmlIndex) {
            3
        } else {
            1
        })
    {
        let mut request = client.get(&url);
        for (k, v) in &source.request_headers {
            request = request.header(k, v)
        }
        let response = request.send().await?.error_for_status()?;
        let stream_updated=source.stream_updated_header.as_deref()
            .and_then(|key|response.headers().get(key)).and_then(|v|v.to_str().ok())
            .and_then(parse_http_or_rfc3339_time);
        let media = archive::validate_media_type(
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/jpeg"),
        )?
        .to_string();
        let bytes = response.bytes().await?;
        let downloaded=Utc::now();
        if bytes.len() < 1024 {
            continue;
        }
        // Unchanged snapshots already have an archived timestamp; do not OCR
        // them again on every poll (especially daytime/overnight stale feeds).
        {
            use sha2::{Digest,Sha256};
            let digest=format!("{:x}",Sha256::digest(&bytes));
            let conn=db::open(db_path)?;
            let exists:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM images WHERE sha256=?1)",[digest],|r|r.get(0))?;
            if exists{duplicates+=1;continue}
        }
        let (mut observed, mut basis) = parse_timestamp(source, &url, downloaded)?;
        if basis=="download_time" {
            if let Some(time)=stream_updated.filter(|t|*t<=downloaded+chrono::Duration::minutes(2)){
                observed=time;basis="source_stream_update";
            }
        }
        if basis == "download_time" && std::env::var("GAIA_OCR_ENABLED").as_deref() == Ok("1") {
            if let Some((time, provenance)) = crate::image_time::read(&bytes, downloaded, archive_root).await {
                observed = time;
                basis = provenance;
            }
        }
        let conn = db::open(db_path)?;
        let stored = archive::store_image(
            &conn,
            archive_root,
            &source.id,
            observed,
            downloaded,
            basis,
            Some(&url),
            &media,
            &bytes,
        )?;
        if stored.duplicate {
            duplicates += 1
        } else {
            downloaded_count += 1
        }
    }
    Ok((discovered, downloaded_count, duplicates))
}

pub async fn run_loop(
    sources: Arc<Vec<SourceConfig>>,
    db_path: std::path::PathBuf,
    archive_root: std::path::PathBuf,
) {
    let client = Client::builder()
        .user_agent("Mozilla/5.0 (compatible; GAIA-Aurora-Archive/0.1; +https://juha.no/gaia/; contact=gaia@juha.no)")
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                "From",
                reqwest::header::HeaderValue::from_static("gaia@juha.no"),
            );
            h
        })
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let slots=Arc::new(tokio::sync::Semaphore::new(4));
    // Keep this on-demand provider from occupying all general crawler slots.
    let meteor_slots=Arc::new(tokio::sync::Semaphore::new(2));
    let meteor_count=sources.iter().filter(|s|s.enabled && matches!(s.kind,SourceKind::NorskMeteor)).count().max(1);
    let mut meteor_index=0usize;
    let mut jobs=tokio::task::JoinSet::new();
    for (index,source) in sources.iter().filter(|s|s.enabled).cloned().enumerate(){
        let client=client.clone();let db_path=db_path.clone();let archive_root=archive_root.clone();let slots=slots.clone();
        let is_meteor=matches!(source.kind,SourceKind::NorskMeteor);
        let meteor_slots=meteor_slots.clone();
        let initial_delay=if is_meteor {
            let offset=(meteor_index as u64)*source.interval_seconds/(meteor_count as u64);
            meteor_index+=1; offset
        } else { index as u64 };
        jobs.spawn(async move{
            tokio::time::sleep(Duration::from_secs(initial_delay)).await;
            let mut timer=tokio::time::interval(Duration::from_secs(source.interval_seconds.max(30)));
            timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let mut failures=0u32;
            let mut retry_after=Instant::now();
            loop{
            timer.tick().await;
            if Instant::now()<retry_after {continue}
            let enabled=db::open(&db_path).ok().and_then(|conn|conn.query_row("SELECT enabled AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=sources.id) FROM sources WHERE id=?1",[&source.id],|r|r.get::<_,bool>(0)).ok()).unwrap_or(false);
            if !enabled{continue}
            let _meteor_permit=if is_meteor {Some(meteor_slots.acquire().await.unwrap())} else {None};
            let _permit=slots.acquire().await.unwrap();
            let started = Utc::now();
            let run_id={let conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("database: {e:#}");
                    continue;
                }
            };
            let _ = db::upsert_source(&conn, &source);
            let run_id = conn
                .execute(
                    "INSERT INTO crawler_runs(source_id,started_utc,state) VALUES(?1,?2,'running')",
                    rusqlite::params![source.id, started.to_rfc3339()],
                )
                .ok()
                .map(|_| conn.last_insert_rowid());
            run_id};
            let result = crawl_once(&source, &client, &db_path, &archive_root).await;
            if let (Some(run_id), Ok(conn)) = (run_id, db::open(&db_path)) {
                match result {
                    Ok((d, n, x)) => {
                        failures=0;
                        let _=conn.execute("UPDATE crawler_runs SET finished_utc=?1,state='complete',discovered=?2,downloaded=?3,duplicate=?4 WHERE id=?5",rusqlite::params![Utc::now().to_rfc3339(),d as i64,n as i64,x as i64,run_id]);
                        let _=conn.execute("UPDATE sources SET last_attempt_utc=?1,last_success_utc=?1,last_error=NULL WHERE id=?2",rusqlite::params![Utc::now().to_rfc3339(),source.id]);
                    }
                    Err(e) => {
                        failures=failures.saturating_add(1);
                        let delay=source.interval_seconds.max(30).saturating_mul(1u64<<failures.min(5)).min(900);
                        retry_after=Instant::now()+Duration::from_secs(delay);
                        tracing::warn!(source=%source.id,"crawl failed: {e:#}");
                        let _=conn.execute("UPDATE crawler_runs SET finished_utc=?1,state='failed',error=?2 WHERE id=?3",rusqlite::params![Utc::now().to_rfc3339(),format!("{e:#}"),run_id]);
                        let _ = conn.execute(
                            "UPDATE sources SET last_attempt_utc=?1,last_error=?2 WHERE id=?3",
                            rusqlite::params![Utc::now().to_rfc3339(), format!("{e:#}"), source.id],
                        );
                    }
                }
            }
            }
        });
    }
    while let Some(result)=jobs.join_next().await{if let Err(e)=result{tracing::error!("camera worker stopped: {e}")}}
}

#[cfg(test)]
mod tests {
    use super::parse_http_or_rfc3339_time;

    #[test]
    fn parses_last_modified_and_api_times() {
        assert_eq!(
            parse_http_or_rfc3339_time("Wed, 09 Sep 2026 09:31:59 GMT")
                .unwrap()
                .to_rfc3339(),
            "2026-09-09T09:31:59+00:00"
        );
        assert_eq!(
            parse_http_or_rfc3339_time("2026-09-09T09:31:59Z")
                .unwrap()
                .to_rfc3339(),
            "2026-09-09T09:31:59+00:00"
        );
    }
}
