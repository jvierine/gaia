//! Single-frame, session/CSRF-aware adapter for Norsk Meteornettverk.
use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Datelike, Timelike, Utc};
use reqwest::{Client, Url};
use scraper::{Html, Selector};
use serde_json::{json, Value};
use crate::model::SourceConfig;

fn selection(url: &str) -> Result<(String, u8)> {
    let url = Url::parse(url)?;
    if url.scheme() != "https" || url.host_str() != Some("norskmeteornettverk.no") || url.path() != "/data/" {
        bail!("Norsk Meteor source must use https://norskmeteornettverk.no/data/");
    }
    let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    let station = params.get("station").context("missing station")?.clone();
    if !station.starts_with("ams") || !station[3..].chars().all(|c| c.is_ascii_digit()) || station.len() <= 3 { bail!("invalid station"); }
    let camera: u8 = params.get("camera").context("missing camera")?.parse()?;
    if !(1..=7).contains(&camera) { bail!("camera must be 1..7"); }
    Ok((station, camera))
}

// Approximate solar altitude is only a crawler daylight gate, never a map overlay.
fn daylight(source: &SourceConfig, now: DateTime<Utc>) -> bool {
    let (Some(lat), Some(lon)) = (source.latitude_deg, source.longitude_deg) else { return false; };
    let decl = (23.44_f64 * (std::f64::consts::TAU * (now.ordinal() as f64 - 81.0) / 365.25).sin()).to_radians();
    let hour = (now.hour() as f64 + now.minute() as f64 / 60.0) * 15.0 + lon - 180.0;
    let lat = lat.to_radians();
    let altitude = (lat.sin()*decl.sin()+lat.cos()*decl.cos()*hour.to_radians().cos()).asin().to_degrees();
    altitude > -4.0
}

pub async fn candidates(client: &Client, source: &SourceConfig, now: DateTime<Utc>) -> Result<Vec<String>> {
    let (station, camera) = selection(&source.url)?;
    if daylight(source, now) { return Ok(vec![]); }
    let base = Url::parse("https://norskmeteornettverk.no/data/")?;
    let response = client.get(base.clone()).send().await?.error_for_status()?;
    let cookie = response.headers().get_all(reqwest::header::SET_COOKIE).iter()
        .filter_map(|v| v.to_str().ok()).filter_map(|v| v.split(';').next()).collect::<Vec<_>>().join("; ");
    let body = response.text().await?;
    let token = {
        let doc = Html::parse_document(&body);
        doc.select(&Selector::parse("meta[name='csrf-token']").unwrap()).next()
            .and_then(|e| e.value().attr("content")).context("missing CSRF token")?.to_owned()
    };
    // Upstream's Now button uses two minutes of latency. Request exactly one still.
    let time = now - chrono::Duration::minutes(2);
    let payload = json!({"stations":[station],"date":time.format("%Y-%m-%d").to_string(),
        "hour":time.hour().to_string(),"minute":time.minute().to_string(),"length":"1","interval":"1",
        "duration":1,"cameras":[camera.to_string()],"file_type":"image_lowres",
        "hevc_supported":false,"stitch_fisheye":false,"stitch_equirect":false});
    let task: Value = client.post(base.join("index.php?action=download")?)
        .header(reqwest::header::COOKIE, &cookie).header("X-CSRF-Token", &token)
        .json(&payload).send().await?.error_for_status()?.json().await?;
    let id = task.get("task_id").and_then(Value::as_str).context("upstream did not create image task")?;
    let mut status_url = base.join("index.php?action=status")?;
    status_url.query_pairs_mut().append_pair("id", id);
    let result = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let status: Value = client.get(status_url.clone()).header(reqwest::header::COOKIE, &cookie)
                .send().await?.error_for_status()?.json().await?;
            match status.get("status").and_then(Value::as_str) {
                Some("complete") => return image_urls(&base, &status),
                Some("error") => bail!("upstream image task failed"),
                _ => {}
            }
        }
    }).await;
    match result {
        Ok(value) => value,
        Err(_) => {
            let mut cancel = base.join("index.php?action=cancel")?;
            cancel.query_pairs_mut().append_pair("id", id).append_pair("csrf_token", &token);
            let _ = client.get(cancel).header(reqwest::header::COOKIE, cookie).send().await;
            Err(anyhow!("Norsk Meteor image task timed out; cancellation requested"))
        }
    }
}

fn image_urls(base: &Url, status: &Value) -> Result<Vec<String>> {
    let files = status.get("files").and_then(Value::as_object).context("missing image results")?;
    for station in files.values().filter_map(Value::as_object) {
        for images in station.values().filter_map(Value::as_array) {
            for image in images {
                if let Some(path) = image.get("url").and_then(Value::as_str) {
                    let url = base.join(path)?;
                    if url.origin() != base.origin() || !url.path().starts_with("/data/download/") || !url.path().ends_with(".jpg") { bail!("unexpected image URL"); }
                    return Ok(vec![url.to_string()]);
                }
            }
        }
    }
    bail!("no image available for requested minute")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn validates_selection() { assert_eq!(selection("https://norskmeteornettverk.no/data/?station=ams123&camera=1").unwrap(), ("ams123".into(),1)); assert!(selection("https://example.org/data/?station=ams123&camera=1").is_err()); assert!(selection("https://norskmeteornettverk.no/data/?station=ams123&camera=8").is_err()); }
    #[test] fn extracts_one_safe_image() { let base=Url::parse("https://norskmeteornettverk.no/data/").unwrap(); let data=json!({"files":{"KRI":{"19:55":[{"url":"download/test.jpg"}]}}}); assert_eq!(image_urls(&base,&data).unwrap().len(),1); assert!(image_urls(&base,&json!({"files":{}})).is_err()); assert!(image_urls(&base,&json!({"files":{"KRI":{"x":[{"url":"https://example.org/test.jpg"}]}}})).is_err()); }
}
    #[tokio::test]
    #[ignore = "requests one real upstream still; archive availability is time limited"]
    async fn live_archive() {
        let sources: Vec<SourceConfig> = serde_json::from_str(include_str!("../sources/norsk-meteor.json")).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-09-08T19:57:00Z").unwrap().with_timezone(&Utc);
        let client = Client::builder().user_agent("GAIA Data Center (gaia@juha.no)").timeout(std::time::Duration::from_secs(30)).build().unwrap();
        let urls = candidates(&client, &sources[0], now).await.unwrap();
        assert_eq!(urls.len(), 1);
        assert!(urls[0].ends_with("KRI_cam1_20260908_1955_image_lowres.jpg"));
        let bytes = client.get(&urls[0]).send().await.unwrap().error_for_status().unwrap().bytes().await.unwrap();
        let image = image::load_from_memory(&bytes).unwrap();
        assert!(image.width() > 100 && image.height() > 100);
        println!("Verified {}: {}x{} ({} bytes)", urls[0], image.width(), image.height(), bytes.len());
    }
