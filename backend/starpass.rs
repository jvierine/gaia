//! The background star photometry pass: the writer that fills
//! `star_photometry` and `frame_sky`.
//!
//! Stars are measured on the full-resolution archived original. The 256 pixel
//! textures the globe is built from destroy a star image entirely, so nothing
//! here may reuse them.
//!
//! The pass is deliberately unhurried. It shares a machine with the publisher,
//! which takes sixteen cores under its own lock, so it takes one frame at a
//! time, rests between frames, and measures at most a handful of frames per
//! camera per cycle. Missing a frame costs nothing: cloud is estimated from a
//! time series, and the series only needs to be sampled often enough to follow
//! the weather.

use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::starphot::{self, Star};
use crate::{db, extinction, geometry};

/// How the pass is tuned. Every field has an environment override so the rate
/// can be changed on the live machine without a rebuild.
#[derive(Debug, Clone)]
pub struct Settings {
    /// The AIDA `WISCAT1` catalogue, gzipped.
    pub catalog_path: PathBuf,
    /// Stars fainter than this are not looked for.
    pub max_magnitude: f64,
    /// Wait between cycles.
    pub interval: Duration,
    /// Rest between frames, so the publisher keeps its cores.
    pub rest: Duration,
    /// Minimum spacing between two photometered frames of one camera.
    pub cadence_minutes: f64,
    /// Frames photometered per cycle, over all cameras.
    pub max_frames_per_cycle: usize,
    /// Frames given a sky row per cycle. Cheap, so the cap is looser.
    pub max_sky_rows_per_cycle: usize,
    /// Photometry runs only when the sun is at or below this elevation.
    pub sun_below_deg: f64,
    /// Half-width of the fitted patch, in pixels.
    pub patch_half: usize,
    /// A fit further than this from the predicted position is not the star.
    pub max_offset_px: f64,
    /// Stars lower than this are refused: the air mass is punishing and the
    /// horizon is cluttered.
    pub min_elevation_deg: f64,
    /// How far back the pass will reach for unmeasured frames.
    pub lookback_hours: f64,
    /// Clear-sky fits attempted per cycle. Each is a scan over a few hundred
    /// trial coefficients, so this is bounded work, but it shares the cycle
    /// with the photometry and should not crowd it out.
    pub max_fits_per_cycle: usize,
    /// Nights back from the current one that stay eligible for refitting. Two
    /// keeps the night in progress and the one just finished current.
    pub fit_nights: i64,
}

fn env_parse<T: std::str::FromStr>(name: &str, fallback: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            catalog_path: PathBuf::from(
                "/home/j/src/widefield-star-calibrator/data/tycho2_mag8.bin.gz",
            ),
            max_magnitude: 4.0,
            interval: Duration::from_secs(300),
            rest: Duration::from_millis(250),
            cadence_minutes: 10.0,
            max_frames_per_cycle: 12,
            max_sky_rows_per_cycle: 400,
            sun_below_deg: -6.0,
            patch_half: 9,
            max_offset_px: 3.0,
            min_elevation_deg: 10.0,
            lookback_hours: 48.0,
            max_fits_per_cycle: 24,
            fit_nights: 2,
        }
    }
}

impl Settings {
    pub fn from_env() -> Self {
        let d = Self::default();
        Self {
            catalog_path: std::env::var("GAIA_STARPHOT_CATALOG")
                .map(PathBuf::from)
                .unwrap_or(d.catalog_path),
            max_magnitude: env_parse("GAIA_STARPHOT_MAX_MAG", d.max_magnitude),
            interval: Duration::from_secs(env_parse("GAIA_STARPHOT_INTERVAL_SEC", 300)),
            rest: Duration::from_millis(env_parse("GAIA_STARPHOT_REST_MS", 250)),
            cadence_minutes: env_parse("GAIA_STARPHOT_CADENCE_MIN", d.cadence_minutes),
            max_frames_per_cycle: env_parse("GAIA_STARPHOT_FRAMES_PER_CYCLE", d.max_frames_per_cycle),
            max_sky_rows_per_cycle: env_parse("GAIA_STARPHOT_SKY_PER_CYCLE", d.max_sky_rows_per_cycle),
            sun_below_deg: env_parse("GAIA_STARPHOT_SUN_BELOW_DEG", d.sun_below_deg),
            patch_half: env_parse("GAIA_STARPHOT_PATCH_HALF", d.patch_half),
            max_offset_px: env_parse("GAIA_STARPHOT_MAX_OFFSET_PX", d.max_offset_px),
            min_elevation_deg: env_parse("GAIA_STARPHOT_MIN_ELEVATION_DEG", d.min_elevation_deg),
            lookback_hours: env_parse("GAIA_STARPHOT_LOOKBACK_HOURS", d.lookback_hours),
            max_fits_per_cycle: env_parse("GAIA_STARPHOT_FITS_PER_CYCLE", d.max_fits_per_cycle),
            fit_nights: env_parse("GAIA_STARPHOT_FIT_NIGHTS", d.fit_nights),
        }
    }
}

/// One archived frame that has not been through the pass yet.
#[derive(Debug, Clone)]
pub struct Frame {
    pub image_id: String,
    pub source_id: String,
    pub archive_path: String,
    pub observation_utc: String,
    pub unix_seconds: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub calibration_path: String,
}

/// What one cycle did, for the log and for the tests.
#[derive(Debug, Default, PartialEq)]
pub struct Report {
    pub sky_rows: usize,
    pub frames_measured: usize,
    pub measurements: usize,
    pub skipped_daylight: usize,
    pub failed: usize,
    /// Camera-night-channels that produced a clear-sky fit.
    pub fits: usize,
    /// Camera-night-channels examined and refused: too few stars, too little
    /// air mass. A normal outcome, counted so it is visible.
    pub fits_refused: usize,
    /// Measurements given an optical depth by those fits.
    pub optical_depths: usize,
}

fn unix_seconds(utc: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(utc)
        .ok()
        .map(|t| t.timestamp() as f64 + t.timestamp_subsec_micros() as f64 * 1e-6)
}

/// The same calibration choice the projection and publisher make: an explicit
/// operator selection if it names a calibration of this camera, otherwise the
/// one whose validity interval covers the frame. Kept identical on purpose, so
/// a star is measured through the lens model the mosaic is actually drawn with.
const CALIBRATION_FOR_FRAME: &str = "c.id=COALESCE(\
    (SELECT cs2.selected_calibration_id FROM camera_settings cs2 \
      WHERE cs2.source_id=s.id AND EXISTS(SELECT 1 FROM calibrations csel \
        WHERE csel.id=cs2.selected_calibration_id AND csel.source_id=s.id)),\
    (SELECT cc.id FROM calibrations cc WHERE cc.source_id=s.id \
      AND (cc.valid_from_utc IS NULL OR julianday(cc.valid_from_utc)<=julianday(i.observation_utc)) \
      AND (cc.valid_to_utc IS NULL OR julianday(cc.valid_to_utc)>julianday(i.observation_utc)) \
      ORDER BY julianday(cc.valid_from_utc) DESC,cc.created_utc DESC LIMIT 1))";

/// Frames from enabled, located, calibrated cameras that have no sky row yet,
/// newest first. A sky row is the mark that the pass has seen a frame, so this
/// set drains even for frames that are never worth photometering.
pub fn pending_frames(conn: &Connection, settings: &Settings) -> Result<Vec<Frame>> {
    // The cutoff is compared as a string, not through julianday(). Every
    // observation_utc in the archive is RFC 3339 with a +00:00 offset, so the
    // ordering is the same, and it lets this use idx_images_observation instead
    // of evaluating a function over every row. Wrapped in julianday() the same
    // query took over two minutes on the live archive; this takes milliseconds.
    let cutoff = (chrono::Utc::now()
        - chrono::Duration::milliseconds((settings.lookback_hours * 3_600_000.0) as i64))
    .to_rfc3339();
    // Candidates only. Resolving which calibration covers each frame is a
    // correlated subquery, far too expensive to run over every image in the
    // window; it is done below for the few frames actually selected.
    let mut q = conn.prepare(
        "SELECT i.id,s.id,i.archive_path,i.observation_utc,s.latitude_deg,s.longitude_deg \
         FROM images i JOIN sources s ON s.id=i.source_id \
         WHERE s.enabled=1 AND s.latitude_deg IS NOT NULL AND s.longitude_deg IS NOT NULL \
           AND i.observation_utc >= ?1 \
           AND EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) \
           AND NOT EXISTS(SELECT 1 FROM frame_sky f WHERE f.source_id=i.source_id AND f.image_id=i.id) \
         ORDER BY i.observation_utc DESC LIMIT ?2",
    )?;
    let rows = q.query_map(
        rusqlite::params![cutoff, settings.max_sky_rows_per_cycle as i64],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
            ))
        },
    )?;
    let candidates = rows.collect::<Result<Vec<_>, _>>()?;
    // The shared expression keys off i.observation_utc; here the frame's time is
    // bound directly, so that reference becomes the parameter.
    let calibration = format!(
        "SELECT c.hdf5_path FROM sources s JOIN calibrations c ON {} WHERE s.id=?1",
        CALIBRATION_FOR_FRAME.replace("i.observation_utc", "?2")
    );
    let mut lens = conn.prepare(&calibration)?;
    let mut out = Vec::new();
    for (image_id, source_id, archive_path, observation_utc, lat, lon) in candidates {
        // The calibration expression keys off i.observation_utc, so the frame's
        // time is supplied as that column for this single-row lookup.
        let cal: Option<String> = lens
            .query_row(rusqlite::params![source_id, observation_utc], |r| r.get(0))
            .ok();
        let Some(cal) = cal else { continue };
        // A frame whose timestamp cannot be read has no sky position, so it has
        // no measurement either. Left pending rather than recorded wrongly.
        let Some(unix) = unix_seconds(&observation_utc) else {
            continue;
        };
        out.push(Frame {
            image_id,
            source_id,
            archive_path,
            observation_utc,
            unix_seconds: unix,
            latitude_deg: lat,
            longitude_deg: lon,
            calibration_path: cal,
        });
    }
    Ok(out)
}

/// Observation times, in unix seconds, that already carry photometry for this
/// camera. Used to hold the sampling cadence across cycles and restarts.
pub fn measured_times(conn: &Connection, source_id: &str, lookback_hours: f64) -> Result<Vec<f64>> {
    let mut q = conn.prepare(
        "SELECT DISTINCT observation_utc FROM star_photometry \
         WHERE source_id=?1 AND julianday(observation_utc) >= julianday('now') - ?2/24.0",
    )?;
    let rows = q.query_map(rusqlite::params![source_id, lookback_hours], |r| {
        r.get::<_, String>(0)
    })?;
    let mut out = Vec::new();
    for row in rows {
        if let Some(t) = unix_seconds(&row?) {
            out.push(t);
        }
    }
    Ok(out)
}

/// Thins a camera's candidate frames to the sampling cadence. Frames are taken
/// newest first, and one is accepted only if it stands clear of every frame
/// already measured and every frame accepted in this pass.
///
/// Returns indices into `candidates`.
pub fn thin_to_cadence(
    candidates: &[f64],
    already_measured: &[f64],
    cadence_seconds: f64,
    limit: usize,
) -> Vec<usize> {
    let mut taken: Vec<f64> = already_measured.to_vec();
    let mut out = Vec::new();
    for (index, &t) in candidates.iter().enumerate() {
        if out.len() >= limit {
            break;
        }
        if taken.iter().all(|&u| (t - u).abs() >= cadence_seconds) {
            taken.push(t);
            out.push(index);
        }
    }
    out
}

/// Measures one frame and writes its rows. The optical parameters are passed in
/// rather than read here so the caller can cache them per calibration: reading
/// them shells out to `h5dump`, which is far more expensive than the fit.
pub fn photometer_frame(
    conn: &Connection,
    frame: &Frame,
    optpar: &[f64],
    stars: &[Star],
    settings: &Settings,
) -> Result<usize> {
    // No expensive star fitting in daylight, even if configured permissively.
    if geometry::solar_elevation_deg(frame.latitude_deg,frame.longitude_deg,frame.unix_seconds)>0.0 {return Ok(0)}
    // Full resolution, never a thumbnail: a star is one or two pixels across.
    let image = image::open(&frame.archive_path)
        .with_context(|| format!("cannot decode {}", frame.archive_path))?
        .to_rgb8();
    let measurements = starphot::measure_frame(
        &image,
        optpar,
        frame.latitude_deg,
        frame.longitude_deg,
        frame.unix_seconds,
        stars,
        settings.patch_half,
        settings.max_offset_px,
        settings.min_elevation_deg,
    );
    if measurements.is_empty() {
        return Ok(0);
    }
    let count=starphot::record_frame(conn,&frame.source_id,&frame.image_id,&frame.observation_utc,&measurements)?;
    // This is the bounded background analysis worker, never the publisher.
    // Publish its ready cloud product atomically; failure leaves fallback intact.
    if let Some(root)=std::env::var_os("GAIA_ARCHIVE_ROOT") {
        let calibration:rusqlite::Result<String>=conn.query_row("SELECT id FROM calibrations WHERE source_id=?1 AND hdf5_path=?2 ORDER BY created_utc DESC LIMIT 1",rusqlite::params![frame.source_id,frame.calibration_path],|r|r.get(0));
        if let Ok(calibration)=calibration {
            if let Err(error)=crate::cloudweight::prepare_ready_field(conn,Path::new(&root),&frame.source_id,&frame.observation_utc,&calibration,[image.width() as f64,image.height() as f64]) {tracing::warn!(%error,"cloud preparation failed; keep fallback");}
        }
    }
    Ok(count)
}

/// Records where the Sun and Moon were for this frame. Cheap, no image is read,
/// and it doubles as the mark that the pass has seen the frame.
pub fn record_sky_for(conn: &Connection, frame: &Frame) -> Result<f64> {
    let sun = geometry::solar_elevation_deg(
        frame.latitude_deg,
        frame.longitude_deg,
        frame.unix_seconds,
    );
    let moon = starphot::moon_state(
        frame.unix_seconds,
        frame.latitude_deg,
        frame.longitude_deg,
    );
    starphot::record_sky(
        conn,
        &frame.source_id,
        &frame.image_id,
        &frame.observation_utc,
        sun,
        &moon,
    )?;
    Ok(sun)
}

/// Optical parameters per calibration file, so `h5dump` runs once per
/// calibration rather than once per frame.
#[derive(Default)]
pub struct OptparCache(HashMap<String, Option<Vec<f64>>>);

impl OptparCache {
    /// Seeds a lens model directly, without reading a file. Used by the tests,
    /// which exercise the pass without an HDF5 calibration on disk.
    pub fn insert(&mut self, hdf5_path: &str, optpar: Vec<f64>) {
        self.0.insert(hdf5_path.to_string(), Some(optpar));
    }

    pub fn get(&mut self, hdf5_path: &str) -> Option<&Vec<f64>> {
        self.0
            .entry(hdf5_path.to_string())
            .or_insert_with(|| match crate::projection::optical_parameters(hdf5_path) {
                Ok(p) => Some(p),
                Err(error) => {
                    tracing::warn!(%hdf5_path, %error, "star photometry cannot read lens model");
                    None
                }
            })
            .as_ref()
    }
}

/// Writes the optical depths of fitted nights that have none, whatever their
/// age.
///
/// The fit pass only looks a night or two back, so a night fitted last week is
/// never revisited and a depth added after it was fitted would never reach it.
/// This walks the stored fits instead of the recent nights, and is self
/// limiting: a night it fills is not a candidate again.
pub fn backfill_depths(conn: &Connection, limit: usize) -> Result<usize> {
    let mut q = conn.prepare(
        "SELECT e.source_id,e.night,e.channel,COALESCE(s.longitude_deg,0) \
         FROM extinction_nights e JOIN sources s ON s.id=e.source_id \
         ORDER BY e.night DESC, e.source_id, e.channel",
    )?;
    let candidates: Vec<(String, i64, String, f64)> = q
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut written = 0usize;
    let mut filled = 0usize;
    for (source_id, night, channel, longitude) in candidates {
        if filled >= limit {
            break;
        }
        let offset = longitude / 15.0 * 3600.0;
        let from = night as f64 * 86400.0 + 43200.0 - offset;
        let (from_utc, to_utc) = (rfc3339(from), rfc3339(from + 86400.0));
        // A window is done when it has depths and none of them exceeds the
        // opaque cap. A value above it was written before the cap existed and
        // is meaningless, so the window is rewritten rather than left.
        let (has, over): (i64, i64) = conn
            .query_row(
                "SELECT count(optical_depth),COALESCE(sum(optical_depth > ?5),0) \
                 FROM star_photometry \
                 WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 \
                   AND observation_utc<?4",
                rusqlite::params![
                    source_id,
                    channel,
                    from_utc,
                    to_utc,
                    extinction::TOTAL_FADING_DEPTH
                ],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((1, 0));
        if has > 0 && over == 0 {
            continue;
        }
        if let Ok(Some(fit)) = extinction::load(conn, &source_id, night, &channel) {
            written +=
                extinction::record_depths(conn, &source_id, &channel, &from_utc, &to_utc, &fit)?;
            filled += 1;
        }
    }
    Ok(written)
}

/// Channels the clear-sky reference is fitted in. Cloud extinction is
/// wavelength dependent, so each is fitted separately.
const FIT_CHANNELS: [&str; 4] = ["mean", "r", "g", "b"];

/// Fits the clear-sky reference for the camera-nights that have moved on since
/// they were last fitted, newest night first. Returns how many fits were stored
/// and how many were examined and refused.
///
/// A night is refitted whenever it has photometry newer than its stored fit, so
/// a night in progress improves through the evening and settles once the camera
/// stops contributing to it.
pub fn fit_recent_nights(conn: &Connection, settings: &Settings) -> Result<(usize, usize, usize)> {
    // Least recently tried first, and never-tried before either. A cycle can
    // only afford a few fits, so without this the walk restarts at the same
    // camera every time and the far end of the archive is never reached.
    let mut cameras = conn.prepare(
        "SELECT p.source_id, s.longitude_deg, \
                (SELECT max(a.attempted_utc) FROM extinction_attempts a WHERE a.source_id=p.source_id) AS tried \
         FROM star_photometry p JOIN sources s ON s.id=p.source_id \
         WHERE s.longitude_deg IS NOT NULL \
           AND julianday(p.observation_utc) >= julianday('now') - ?1 \
         GROUP BY p.source_id ORDER BY tried IS NOT NULL, tried",
    )?;
    let rows = cameras.query_map(rusqlite::params![settings.fit_nights + 1], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
    })?;
    let cameras: Vec<(String, f64)> = rows.collect::<Result<Vec<_>, _>>()?;

    let now = chrono::Utc::now().timestamp() as f64;
    let current_night = extinction::night_index(now, 0.0);
    let mut stored = 0usize;
    let mut refused = 0usize;
    let mut depths = 0usize;
    for (source_id, longitude) in cameras {
        for back in 0..settings.fit_nights {
            if stored + refused >= settings.max_fits_per_cycle {
                return Ok((stored, refused, depths));
            }
            let night = extinction::night_index(now, longitude) - back;
            // The night in local solar time, converted back to a UTC window.
            let offset = longitude / 15.0 * 3600.0;
            let from = (night as f64) * 86400.0 + 43200.0 - offset;
            let to = from + 86400.0;
            let (from_utc, to_utc) = (rfc3339(from), rfc3339(to));
            // Nothing new since the last fit means nothing to do.
            let newest: Option<String> = conn
                .query_row(
                    "SELECT max(observation_utc) FROM star_photometry \
                     WHERE source_id=?1 AND observation_utc>=?2 AND observation_utc<?3",
                    rusqlite::params![source_id, from_utc, to_utc],
                    |r| r.get(0),
                )
                .unwrap_or(None);
            let Some(newest) = newest else { continue };
            // Every channel already decided on data at least this recent. A
            // refusal counts: it means this night cannot be fitted as it stands,
            // and re-deciding it every five minutes starves every camera behind
            // it in the list.
            let decided: Option<String> = conn
                .query_row(
                    "SELECT min(attempted_utc) FROM extinction_attempts \
                     WHERE source_id=?1 AND night=?2",
                    rusqlite::params![source_id, night],
                    |r| r.get(0),
                )
                .unwrap_or(None);
            let complete: i64 = conn
                .query_row(
                    "SELECT count(*) FROM extinction_attempts WHERE source_id=?1 AND night=?2",
                    rusqlite::params![source_id, night],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            let settled = complete as usize >= FIT_CHANNELS.len()
                && decided.as_deref().is_some_and(|d| d > newest.as_str());
            for channel in FIT_CHANNELS {
                if settled {
                    // The night has been decided and has no new data, so it
                    // does not need fitting again. It may still be missing its
                    // optical depths: the depths were added after these fits
                    // were made, and a night nothing new lands in would
                    // otherwise never get them. Writing them from the stored
                    // fit costs one pass over the window and no refit.
                    let (has_depths, over_cap): (i64, i64) = conn
                        .query_row(
                            "SELECT count(optical_depth),COALESCE(sum(optical_depth > ?5),0) \
                             FROM star_photometry \
                             WHERE source_id=?1 AND channel=?2 \
                               AND observation_utc>=?3 AND observation_utc<?4",
                            rusqlite::params![
                                source_id,
                                channel,
                                from_utc,
                                to_utc,
                                extinction::TOTAL_FADING_DEPTH
                            ],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .unwrap_or((1, 0));
                    if has_depths == 0 || over_cap > 0 {
                        if let Ok(Some(fit)) =
                            extinction::load(conn, &source_id, night, channel)
                        {
                            depths += extinction::record_depths(
                                conn, &source_id, channel, &from_utc, &to_utc, &fit,
                            )?;
                        }
                    }
                    continue;
                }
                let mut q = conn.prepare(
                    "SELECT star_key,elevation_deg,flux FROM star_photometry \
                     WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 \
                       AND observation_utc<?4 AND amplitude IS NOT NULL \
                       AND flux IS NOT NULL AND flux>0 AND flux_snr>=?5",
                )?;
                let samples = q
                    .query_map(
                        rusqlite::params![source_id, channel, from_utc, to_utc, min_fit_snr()],
                        |r| {
                            Ok(extinction::Sample {
                                star_key: r.get(0)?,
                                elevation_deg: r.get(1)?,
                                flux: r.get(2)?,
                            })
                        },
                    )?
                    .collect::<Result<Vec<_>, _>>()?;
                let outcome = extinction::fit_night(&samples);
                if let Some(fit) = &outcome {
                    extinction::record(conn, &source_id, night, channel, fit)?;
                    // The reference is only useful once it has been carried
                    // through to the measurements it explains.
                    depths += extinction::record_depths(
                        conn, &source_id, channel, &from_utc, &to_utc, fit,
                    )?;
                    stored += 1;
                } else {
                    // A night that cannot be fitted leaves no depth behind, not
                    // even a stale one from an earlier fit of the same night.
                    conn.execute(
                        "UPDATE star_photometry SET optical_depth=NULL \
                         WHERE source_id=?1 AND channel=?2 \
                           AND observation_utc>=?3 AND observation_utc<?4 \
                           AND optical_depth IS NOT NULL",
                        rusqlite::params![source_id, channel, from_utc, to_utc],
                    )?;
                    refused += 1;
                }
                conn.execute(
                    "INSERT INTO extinction_attempts(source_id,night,channel,attempted_utc,fitted) \
                     VALUES(?1,?2,?3,?4,?5) \
                     ON CONFLICT(source_id,night,channel) DO UPDATE SET \
                        attempted_utc=excluded.attempted_utc,fitted=excluded.fitted",
                    rusqlite::params![
                        source_id,
                        night,
                        channel,
                        chrono::Utc::now().to_rfc3339(),
                        outcome.is_some() as i64
                    ],
                )?;
            }
        }
    }
    Ok((stored, refused, depths))
}

/// A measurement has to stand clear of the noise before it can define a clear
/// sky, and the floor has to be well above the detection threshold rather than
/// at it. A star measured near its detection limit is only recorded on the
/// nights and elevations where it happened to be bright enough, so the
/// surviving samples at high air mass are the clear ones. That censoring
/// flattens the extinction slope and can drive the fitted coefficient negative.
/// Requiring a comfortable margin keeps the sample complete.
fn min_fit_snr() -> f64 {
    env_parse("GAIA_STARPHOT_FIT_MIN_SNR", 20.0)
}

fn rfc3339(unix_seconds: f64) -> String {
    chrono::DateTime::from_timestamp(unix_seconds as i64, 0)
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339()
}

/// One pass over the pending frames.
pub fn run_cycle(
    conn: &Connection,
    stars: &[Star],
    settings: &Settings,
    cache: &mut OptparCache,
    rest: impl Fn(),
) -> Result<Report> {
    let mut report = Report::default();
    let pending = pending_frames(conn, settings)?;
    // Sun and Moon first, for every pending frame. This is arithmetic only, and
    // writing it now both fills the sky record and drains the pending set.
    let mut dark: HashMap<String, Vec<Frame>> = HashMap::new();
    for frame in pending {
        match record_sky_for(conn, &frame) {
            Ok(sun) => {
                report.sky_rows += 1;
                if sun <= settings.sun_below_deg.min(0.0) {
                    dark.entry(frame.source_id.clone()).or_default().push(frame);
                } else {
                    report.skipped_daylight += 1;
                }
            }
            Err(error) => {
                report.failed += 1;
                tracing::warn!(image = %frame.image_id, %error, "cannot record frame sky");
            }
        }
    }

    // Then the expensive half, thinned to the cadence and capped per cycle.
    let cadence = settings.cadence_minutes * 60.0;
    let mut cameras: Vec<_> = dark.into_iter().collect();
    // Stable order so a cycle is reproducible and no camera is starved by map
    // iteration order.
    cameras.sort_by(|a, b| a.0.cmp(&b.0));
    for (source_id, frames) in cameras {
        if report.frames_measured >= settings.max_frames_per_cycle {
            break;
        }
        let measured = measured_times(conn, &source_id, settings.lookback_hours)?;
        let times: Vec<f64> = frames.iter().map(|f| f.unix_seconds).collect();
        let remaining = settings.max_frames_per_cycle - report.frames_measured;
        for index in thin_to_cadence(&times, &measured, cadence, remaining) {
            let frame = &frames[index];
            let Some(optpar) = cache.get(&frame.calibration_path).cloned() else {
                report.failed += 1;
                continue;
            };
            match photometer_frame(conn, frame, &optpar, stars, settings) {
                Ok(rows) => {
                    report.frames_measured += 1;
                    report.measurements += rows;
                }
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(image = %frame.image_id, %error, "star photometry failed");
                }
            }
            rest();
        }
    }

    // Then the clear-sky reference over what has been measured. It is cheap
    // next to the fitting, and it is what turns a flux into an optical depth.
    // Fitted nights older than the fit window still need their depths written
    // once, and this is what reaches them.
    match backfill_depths(conn, 12) {
        Ok(written) => report.optical_depths += written,
        Err(error) => tracing::warn!(%error, "clear-sky depth backfill failed"),
    }
    match fit_recent_nights(conn, settings) {
        Ok((stored, refused, depths)) => {
            report.fits = stored;
            report.fits_refused = refused;
            report.optical_depths = depths;
        }
        Err(error) => tracing::warn!(%error, "clear-sky fit failed"),
    }
    Ok(report)
}

/// The background loop. Returns without doing anything if the catalogue is
/// missing, which is not fatal: the archive keeps working, only the cloud
/// estimate goes unfed.
pub async fn run_loop(db_path: PathBuf, settings: Settings) {
    let stars = match starphot::load_catalog(&settings.catalog_path, settings.max_magnitude) {
        Ok(stars) => stars,
        Err(error) => {
            tracing::warn!(
                catalog = %settings.catalog_path.display(), %error,
                "star photometry disabled: no catalogue"
            );
            return;
        }
    };
    tracing::info!(
        stars = stars.len(),
        magnitude = settings.max_magnitude,
        cadence_min = settings.cadence_minutes,
        "star photometry pass started"
    );
    let stars = std::sync::Arc::new(stars);
    let mut cache = OptparCache::default();
    loop {
        let path = db_path.clone();
        let these = stars.clone();
        let tuning = settings.clone();
        let rest = settings.rest;
        // Fitting is blocking, CPU-bound work and must not sit on an async
        // worker thread. The cache moves in and back out so the lens models are
        // read once rather than once per cycle.
        let mut carried = std::mem::take(&mut cache);
        let handle = tokio::task::spawn_blocking(move || {
            let conn = db::open(&path)?;
            let report = run_cycle(&conn, &these, &tuning, &mut carried, || {
                std::thread::sleep(rest)
            })?;
            Ok::<_, anyhow::Error>((report, carried))
        });
        match handle.await {
            Ok(Ok((report, returned))) => {
                cache = returned;
                if report != Report::default() {
                    tracing::info!(
                        sky_rows = report.sky_rows,
                        frames = report.frames_measured,
                        measurements = report.measurements,
                        daylight = report.skipped_daylight,
                        failed = report.failed,
                        fits = report.fits,
                        fits_refused = report.fits_refused,
                        depths = report.optical_depths,
                        "star photometry cycle"
                    );
                }
            }
            // A failed cycle loses the cache; it is only an optimisation, and
            // the next cycle reads the lens models again.
            Ok(Err(error)) => tracing::warn!(%error, "star photometry cycle failed"),
            Err(error) => tracing::warn!(%error, "star photometry task failed"),
        }
        tokio::time::sleep(settings.interval).await;
    }
}

/// Whether the pass should run at all.
pub fn enabled() -> bool {
    std::env::var("GAIA_STARPHOT_ENABLED").as_deref() != Ok("0")
}

/// Path check used by the caller to log a clear reason when the catalogue is
/// absent, before any work is scheduled.
pub fn catalog_present(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daylight_never_decodes_or_fits_an_image() {
        let conn=test_db();let frame=Frame{image_id:"daylight".into(),source_id:"cam".into(),archive_path:"/does-not-exist".into(),observation_utc:"2026-06-21T12:00:00Z".into(),unix_seconds:unix_seconds("2026-06-21T12:00:00Z").unwrap(),latitude_deg:69.35,longitude_deg:20.36,calibration_path:"unused".into()};
        let mut settings=Settings::default();settings.sun_below_deg=90.;
        assert_eq!(photometer_frame(&conn,&frame,&[],&[],&settings).unwrap(),0);
    }
    #[test]
    fn cadence_thinning_keeps_one_frame_per_window() {
        // Frames every two minutes, newest first, with a ten minute cadence.
        let candidates: Vec<f64> = (0..10).map(|i| 10_000.0 - i as f64 * 120.0).collect();
        let taken = thin_to_cadence(&candidates, &[], 600.0, 10);
        assert_eq!(taken, vec![0, 5]);
        let times: Vec<f64> = taken.iter().map(|&i| candidates[i]).collect();
        assert!((times[0] - times[1]).abs() >= 600.0);
    }

    #[test]
    fn cadence_thinning_respects_frames_already_measured() {
        let candidates = [10_000.0, 9_880.0, 9_400.0];
        // The newest frame is inside the window of one already measured, so it
        // must be refused; the cadence has to survive a restart.
        let taken = thin_to_cadence(&candidates, &[10_100.0], 600.0, 10);
        assert_eq!(taken, vec![2]);
    }

    #[test]
    fn cadence_thinning_honours_the_limit() {
        let candidates: Vec<f64> = (0..20).map(|i| 100_000.0 - i as f64 * 700.0).collect();
        assert_eq!(thin_to_cadence(&candidates, &[], 600.0, 3).len(), 3);
    }

    /// A fisheye lens model with no rotation and unit scaling, so a direction
    /// maps to a pixel by a formula the test can invert by hand.
    fn equidistant_optpar() -> Vec<f64> {
        // optmod 2: r = sin(alpha * theta) / alpha, with alpha 1 this is sin(theta).
        vec![2.0, 0.25, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0]
    }

    fn planted_image(width: u32, height: u32, stars: &[(f64, f64, f64)]) -> image::RgbImage {
        let mut image = image::RgbImage::from_pixel(width, height, image::Rgb([12, 12, 12]));
        for &(cx, cy, amplitude) in stars {
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f64 - cx;
                    let dy = y as f64 - cy;
                    let value = amplitude * (-(dx * dx + dy * dy) / (2.0 * 1.6 * 1.6)).exp();
                    let pixel = image.get_pixel_mut(x, y);
                    for channel in 0..3 {
                        pixel.0[channel] = (pixel.0[channel] as f64 + value).min(255.0) as u8;
                    }
                }
            }
        }
        image
    }

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        // The photometry tables reference sources and images; this test exercises
        // them in isolation, without the rest of the archive.
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn
    }


    /// Seeds a camera, a calibration and a run of frames, each frame a PNG with
    /// this frame's own stars planted where the lens model puts them.
    fn seed_archive(conn: &Connection, dir: &Path, optpar: &[f64], catalog: &[Star])
        -> (Vec<String>, String) {
        // An equatorial station, so midnight UTC is dark whatever the date. A
        // polar station would make this test fail every summer.
        let (lat, lon) = (0.0_f64, 0.0_f64);
        conn.execute(
            "INSERT INTO producers(id,name,acknowledgement,copyright) \
             VALUES('p','Producer','Thanks','(c) Producer')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO sources(id,producer_id,name,kind,url,timestamp_mode,interval_seconds,\
                config_json,latitude_deg,longitude_deg,enabled) \
             VALUES('cam','p','Camera','allsky','http://example.invalid','archive',60,'{}',?1,?2,1)",
            rusqlite::params![lat, lon],
        ).unwrap();
        conn.execute(
            "INSERT INTO calibrations(id,source_id,created_utc,method,hdf5_path) \
             VALUES('cal','cam','2026-01-01T00:00:00+00:00','AIDA/WISC','lens.h5')",
            [],
        ).unwrap();

        let midnight = chrono::Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let mut night = Vec::new();
        for step in 0..6 {
            night.push(midnight - chrono::Duration::minutes(2 * step));
        }
        let noon = midnight - chrono::Duration::hours(12);

        let mut night_ids = Vec::new();
        for (index, time) in night.iter().chain(std::iter::once(&noon)).enumerate() {
            let unix = time.timestamp() as f64;
            let path = dir.join(format!("frame-{index}.png"));
            let mut planted = Vec::new();
            for star in catalog {
                let (az, ze) = starphot::star_az_ze(star.ra_hours, star.dec_deg, unix, lat, lon);
                let el = 90.0 - ze.to_degrees();
                if el < 15.0 {
                    continue;
                }
                if let Some((x, y)) =
                    starphot::star_pixel(az.to_degrees(), el, optpar, 512.0, 512.0, 9.0)
                {
                    planted.push((x, y, 180.0));
                }
            }
            assert!(
                !planted.is_empty(),
                "frame {index} has no star above the horizon; the synthetic catalogue should \
                 guarantee some at every hour"
            );
            planted_image(512, 512, &planted).save(&path).unwrap();
            let id = format!("img-{index}");
            conn.execute(
                "INSERT INTO images(id,source_id,observation_utc,downloaded_utc,timestamp_basis,\
                    archive_path,sha256,media_type,width,height,processing_state) \
                 VALUES(?1,'cam',?2,?2,'archive',?3,?4,'image/png',512,512,'received')",
                rusqlite::params![
                    id,
                    time.to_rfc3339(),
                    path.to_string_lossy(),
                    format!("sha-{index}")
                ],
            ).unwrap();
            if index < night.len() {
                night_ids.push(id);
            }
        }
        (night_ids, "lens.h5".into())
    }

    #[test]
    fn a_cycle_records_every_frame_and_photometers_at_the_cadence() {
        let dir = std::env::temp_dir().join(format!("gaia-cycle-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let optpar = equidistant_optpar();
        // A synthetic catalogue spread right around the sky rather than real
        // stars: which real star is up depends on the date the test runs, and
        // this test asserts on frames timed relative to today.
        let catalog: Vec<Star> = (0..24)
            .flat_map(|hour| {
                [-20.0, 0.0, 20.0].map(move |dec| Star {
                    ra_hours: hour as f64,
                    dec_deg: dec,
                    vt_mag: 1.0,
                })
            })
            .collect();
        let conn = test_db();
        let (night_ids, lens) = seed_archive(&conn, &dir, &optpar, &catalog);

        let settings = Settings { rest: Duration::ZERO, ..Settings::default() };
        let mut cache = OptparCache::default();
        cache.insert(&lens, optpar.clone());

        let report = run_cycle(&conn, &catalog, &settings, &mut cache, || {}).unwrap();
        // Every frame gets a sky row, including the one in daylight.
        assert_eq!(report.sky_rows, 7, "{report:?}");
        assert_eq!(report.skipped_daylight, 1, "midday must not be photometered");
        assert_eq!(report.failed, 0, "{report:?}");
        // Six night frames two minutes apart span ten minutes, so a ten minute
        // cadence takes the newest and the oldest, and nothing between.
        assert_eq!(report.frames_measured, 2, "{report:?}");

        let sky: i64 = conn.query_row("SELECT count(*) FROM frame_sky", [], |r| r.get(0)).unwrap();
        assert_eq!(sky, 7);
        let measured: Vec<String> = {
            let mut q = conn
                .prepare("SELECT DISTINCT image_id FROM star_photometry ORDER BY image_id")
                .unwrap();
            let rows = q.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        assert_eq!(measured, vec![night_ids[0].clone(), night_ids[5].clone()]);

        // Both measured frames must actually contain detected stars, not just
        // rows saying a star was looked for.
        for image_id in &measured {
            let found: i64 = conn
                .query_row(
                    "SELECT count(*) FROM star_photometry WHERE image_id=?1 AND amplitude IS NOT NULL",
                    [image_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(found > 0, "no star detected in {image_id}");
        }

        // The pending set is drained: a second cycle finds nothing to do. Without
        // this the pass would re-measure the same frames for ever.
        let again = run_cycle(&conn, &catalog, &settings, &mut cache, || {}).unwrap();
        assert_eq!(again, Report::default(), "{again:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_frame_is_measured_and_recorded_once_the_sky_row_marks_it_seen() {
        let dir = std::env::temp_dir().join(format!("gaia-starpass-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("frame.png");

        // Skibotn, deep polar night, so bright stars are well up.
        let (lat, lon) = (69.35, 20.36);
        let observation = "2026-01-15T23:00:00+00:00";
        let unix = unix_seconds(observation).unwrap();
        let optpar = equidistant_optpar();
        let (width, height) = (512u32, 512u32);

        // Plant the three brightest catalogue stars that the lens actually sees.
        let catalog = [
            Star { ra_hours: 6.752, dec_deg: -16.716, vt_mag: -1.09 }, // Sirius
            Star { ra_hours: 14.261, dec_deg: 19.182, vt_mag: -0.05 }, // Arcturus
            Star { ra_hours: 5.278, dec_deg: 45.998, vt_mag: 0.08 },   // Capella
            Star { ra_hours: 18.615, dec_deg: 38.784, vt_mag: 0.03 },  // Vega
        ];
        let mut planted = Vec::new();
        for star in &catalog {
            let (az, ze) = starphot::star_az_ze(star.ra_hours, star.dec_deg, unix, lat, lon);
            let el = 90.0 - ze.to_degrees();
            if el < 10.0 {
                continue;
            }
            if let Some((x, y)) =
                starphot::star_pixel(az.to_degrees(), el, &optpar, width as f64, height as f64, 9.0)
            {
                planted.push((x, y, 180.0));
            }
        }
        assert!(!planted.is_empty(), "the test needs at least one star in the field");
        planted_image(width, height, &planted).save(&path).unwrap();

        let conn = test_db();
        let frame = Frame {
            image_id: "img-1".into(),
            source_id: "cam".into(),
            archive_path: path.to_string_lossy().into_owned(),
            observation_utc: observation.into(),
            unix_seconds: unix,
            latitude_deg: lat,
            longitude_deg: lon,
            calibration_path: "unused".into(),
        };
        let settings = Settings::default();

        let sun = record_sky_for(&conn, &frame).unwrap();
        assert!(sun < -6.0, "January midnight at Skibotn is dark, got {sun}");
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM frame_sky", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "the sky row marks the frame as seen");

        let written = photometer_frame(&conn, &frame, &optpar, &catalog, &settings).unwrap();
        assert!(written > 0, "no measurements were recorded");

        // Every planted star must be found, in all four channels, close to where
        // the lens model put it.
        let (found, worst): (i64, f64) = conn
            .query_row(
                "SELECT count(*),max(centroid_offset_px) FROM star_photometry \
                 WHERE amplitude IS NOT NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(found as usize, planted.len() * 4, "one row per star per channel");
        assert!(worst < 1.0, "worst centroid offset {worst} px");

        // And the flux must be well above the noise of a clean synthetic frame.
        let weakest: f64 = conn
            .query_row(
                "SELECT min(flux_snr) FROM star_photometry WHERE flux_snr IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(weakest > 5.0, "weakest flux SNR {weakest}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
