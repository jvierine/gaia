use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimestampMode {
    Archive,
    DownloadTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    HtmlIndex,
    JsonFeed,
    SnapshotUrl,
    NorskMeteor,
    Push,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProducerConfig {
    pub name: String,
    pub institution: Option<String>,
    pub website: Option<String>,
    pub acknowledgement: String,
    pub copyright: String,
    pub license: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    pub url: String,
    pub timestamp_mode: TimestampMode,
    pub interval_seconds: u64,
    pub producer: ProducerConfig,
    pub latitude_deg: Option<f64>,
    pub longitude_deg: Option<f64>,
    pub altitude_m: Option<f64>,
    /// Legacy input accepted for compatibility; ignored. Acquisition runs day and night.
    pub darkness_sun_altitude_deg: Option<f64>,
    pub image_link_regex: Option<String>,
    pub timestamp_regex: Option<String>,
    pub timestamp_format: Option<String>,
    /// Optional upstream stream-update time; not an exposure timestamp.
    pub stream_updated_header: Option<String>,
    pub json_items_pointer: Option<String>,
    pub json_url_field: Option<String>,
    pub json_timestamp_field: Option<String>,
    #[serde(default)]
    pub request_headers: std::collections::BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct SourceStatus {
    pub id: String,
    pub name: String,
    pub producer: String,
    pub latitude_deg: Option<f64>,
    pub longitude_deg: Option<f64>,
    pub calibrated: bool,
    pub enabled: bool,
    pub state: String,
    pub timestamp_mode: String,
    pub last_observation_utc: Option<String>,
    pub last_download_utc: Option<String>,
    pub latency_seconds: Option<i64>,
    pub images_24h: i64,
    pub clear_fraction: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    pub service: &'static str,
    pub now_utc: String,
    pub emission_altitude_km: f64,
    pub active_sources: i64,
    pub images_24h: i64,
    pub latest_observation_utc: Option<String>,
    pub latest_mosaic_utc: Option<String>,
    pub pipeline: Vec<PipelineStage>,
}

#[derive(Debug, Serialize)]
pub struct PipelineStage {
    pub name: &'static str,
    pub state: &'static str,
    pub detail: String,
}
