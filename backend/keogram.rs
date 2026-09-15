//! Paired keograms along the equidistant cut between two stations.
//!
//! `equalize.rs` establishes *where* two cameras may fairly be compared: the
//! great circle of points equidistant from both stations, where neither view is
//! favoured by slant range or zenith angle. This module turns that locus into
//! data. Sampling the cut in both cameras at one instant gives two profiles of
//! the same sky; stacking those profiles over time gives a pair of keograms
//! sharing a spatial and a temporal axis, so any row pair is a set of
//! simultaneous, co-located intensity measurements and their scatter plot is
//! the errors-in-variables problem the equalization fit has to solve.
//!
//! The cut geometry never moves. A projection mesh depends on the calibration,
//! the station and the crop or mask, but never on time, so the texture
//! coordinates of the cut points are found once per camera and reused for every
//! frame. What each further frame costs is one texture decode and `samples`
//! reads, which is what makes a night's keogram affordable at all.

use crate::{AppState, equalize, geometry, projection};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Twice the ground reach at the \SI{75}{\degree} taper knee. Stations further
/// apart than this share no sky worth comparing, so they are not offered as a
/// pair at all.
pub const MAX_SEPARATION_KM: f64 = 746.0;

/// Half-length of the cut, either side of the midpoint. Wide enough to cross
/// the useful overlap of a separated pair without running so far out that every
/// sample falls outside one of the meshes.
pub const DEFAULT_HALF_WIDTH_KM: f64 = 300.0;

/// Samples across the cut. 128 over 600 km is roughly one sample per 4.7 km,
/// finer than the projected resolution of any camera in the archive at that
/// range, so the limit on detail is the optics rather than this grid.
pub const DEFAULT_SAMPLES: usize = 128;
pub const MAX_SAMPLES: usize = 512;

/// Seconds between keogram rows, and the most rows one request will build. The
/// cap is what keeps a careless window from turning into an hour of decoding:
/// a request that would exceed it is thinned, not truncated, so the keogram
/// still spans the window the operator asked for.
pub const DEFAULT_CADENCE_SECONDS: i64 = 300;
pub const MAX_ROWS: usize = 300;

/// How far apart two frames may be and still count as simultaneous. Auroral
/// forms move at about a kilometre a second, so 30 s is some 30 km of smear at
/// the shell -- already comparable to the sample spacing, and the point past
/// which a row pair stops describing the same sky.
pub const MAX_SKEW_SECONDS: i64 = 30;

#[derive(Clone, Debug)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
}

impl Station {
    pub fn ecef(&self) -> [f64; 3] {
        geometry::observer_ecef(self.latitude_deg, self.longitude_deg, self.altitude_m / 1000.0)
    }
}

/// Great-circle separation of two stations at ground level, in kilometres.
///
/// Taken from the chord rather than from `acos` of the dot product. The two
/// agree for separated stations, but `acos` loses all its precision exactly
/// where this archive has its best-conditioned pairs: at zero separation the
/// dot product rounds to just under one, and `acos` turns that last bit into
/// a hundred metres of phantom baseline. The chord form stays accurate there,
/// which is what lets co-located instruments be recognised as co-located.
pub fn separation_km(a: &Station, b: &Station) -> f64 {
    let unit = |v: [f64; 3]| {
        let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        [v[0] / m, v[1] / m, v[2] / m]
    };
    let (p, q) = (unit(a.ecef()), unit(b.ecef()));
    let chord = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt();
    2.0 * geometry::EARTH_RADIUS_KM * (chord / 2.0).clamp(-1.0, 1.0).asin()
}

/// Every camera that could take part in a comparison: enabled, located, and
/// calibrated, since without a calibration there is no mesh to sample.
pub fn stations(conn: &rusqlite::Connection) -> Result<Vec<Station>> {
    let mut q = conn.prepare(
        "SELECT id,name,latitude_deg,longitude_deg,COALESCE(altitude_m,0)
         FROM sources
         WHERE enabled=1 AND latitude_deg IS NOT NULL AND longitude_deg IS NOT NULL
           AND EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=sources.id)
         ORDER BY name",
    )?;
    let rows = q.query_map([], |r| {
        Ok(Station {
            id: r.get("id")?,
            name: r.get("name")?,
            latitude_deg: r.get("latitude_deg")?,
            longitude_deg: r.get("longitude_deg")?,
            altitude_m: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn station(conn: &rusqlite::Connection, id: &str) -> Result<Station> {
    stations(conn)?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow!("{id} is not an enabled, located, calibrated camera"))
}

/// The cameras that can be paired with this one, nearest first.
///
/// Co-located instruments come first and are flagged: they are the
/// best-conditioned comparisons in the archive, because equidistance holds
/// everywhere and the two views differ by nothing but the instruments.
pub fn pairs(conn: &rusqlite::Connection, id: &str) -> Result<Vec<Value>> {
    let here = station(conn, id)?;
    let mut out: Vec<(f64, Value)> = Vec::new();
    for other in stations(conn)? {
        if other.id == here.id {
            continue;
        }
        let km = separation_km(&here, &other);
        if km > MAX_SEPARATION_KM {
            continue;
        }
        out.push((
            km,
            json!({
                "partner_id": other.id,
                "partner_name": other.name,
                "separation_km": km,
                // Below a kilometre the stations are the same site; the cut is
                // then taken along the magnetic meridian instead of across a
                // baseline that does not exist.
                "colocated": km < 1.0,
                "latitude_deg": other.latitude_deg,
                "longitude_deg": other.longitude_deg,
            }),
        ));
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out.into_iter().map(|(_, v)| v).collect())
}

/// Magnetic declination at a point, which orients the cut for a co-located
/// pair and is unused for every other pair.
fn declination_deg(latitude_deg: f64, longitude_deg: f64, year: i32) -> f64 {
    ferromagnetic::igrf::IGRF::default()
        .calc(latitude_deg, longitude_deg, 100.0, year as f64)
        .result
        .declination
}

/// The shell points of the cut between two stations, in ECEF kilometres.
pub fn cut_points(a: &Station, b: &Station, half_width_km: f64, samples: usize, year: i32) -> Option<Vec<[f64; 3]>> {
    let midpoint_lat = (a.latitude_deg + b.latitude_deg) / 2.0;
    let midpoint_lon = (a.longitude_deg + b.longitude_deg) / 2.0;
    let half = half_width_km / (geometry::EARTH_RADIUS_KM + geometry::EMISSION_ALTITUDE_KM);
    equalize::equidistant_cut(
        a.ecef(),
        b.ecef(),
        declination_deg(midpoint_lat, midpoint_lon, year),
        half,
        samples,
    )
}

/// One frame of one camera: when it was taken, and where its pixels live.
struct Frame {
    at: chrono::DateTime<chrono::Utc>,
    id: String,
}

/// Frames of one camera inside a window, in time order.
fn frames(
    conn: &rusqlite::Connection,
    source_id: &str,
    from_utc: &str,
    to_utc: &str,
) -> Result<Vec<Frame>> {
    let mut q = conn.prepare(
        "SELECT id,observation_utc FROM images
         WHERE source_id=?1 AND observation_utc>=?2 AND observation_utc<?3
         ORDER BY observation_utc",
    )?;
    let rows = q.query_map(rusqlite::params![source_id, from_utc, to_utc], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, at) = row?;
        if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(&at) {
            out.push(Frame { at: parsed.with_timezone(&chrono::Utc), id });
        }
    }
    Ok(out)
}

/// The frame nearest `at`, if one falls within the simultaneity tolerance.
fn nearest<'f>(list: &'f [Frame], at: chrono::DateTime<chrono::Utc>) -> Option<&'f Frame> {
    let best = list
        .iter()
        .min_by_key(|f| (f.at - at).num_milliseconds().abs())?;
    ((best.at - at).num_seconds().abs() <= MAX_SKEW_SECONDS).then_some(best)
}

/// Texture coordinates of the cut points in one camera, together with the
/// geometry key they were computed for. Held across frames because recomputing
/// them per frame would dominate the cost and never change the answer.
struct Sampler {
    uv: Vec<Option<[f32; 2]>>,
    geometry_url: String,
}

fn sampler(s: &AppState, source_id: &str, at: chrono::DateTime<chrono::Utc>, points: &[[f64; 3]]) -> Result<Sampler> {
    let assets = projection::assets(s, source_id, Some(at))?;
    let url = assets["geometry_url"].as_str().unwrap_or_default().to_string();
    let name = url.rsplit('/').next().unwrap_or_default();
    let bytes = std::fs::read(s.archive_root.join("projection-cache").join(name))?;
    let floats: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Ok(Sampler { uv: equalize::sample_uv(&floats, points), geometry_url: url })
}

/// Read one frame's texture at each cut point. Missing samples -- outside the
/// mesh, or masked -- stay `None` rather than becoming zero, which would read
/// as a black sky and pull any later fit towards the origin.
fn profile(s: &AppState, source_id: &str, at: chrono::DateTime<chrono::Utc>, sampler: &Sampler) -> Result<Vec<Option<[u8; 3]>>> {
    let assets = projection::assets(s, source_id, Some(at))?;
    let texture = assets["texture_url"].as_str().unwrap_or_default();
    let name = texture.rsplit('/').next().unwrap_or_default();
    let image = image::open(s.archive_root.join("projection-cache").join(name))?.to_rgb8();
    let (w, h) = (image.width() as f32, image.height() as f32);
    Ok(sampler
        .uv
        .iter()
        .map(|uv| {
            let [u, v] = (*uv)?;
            // uv are normalised image coordinates with v down, the same
            // convention the compositor samples with; no flip belongs here.
            let x = (u * w).floor().clamp(0.0, w - 1.0) as u32;
            let y = (v * h).floor().clamp(0.0, h - 1.0) as u32;
            Some(image.get_pixel(x, y).0)
        })
        .collect())
}

/// Times to build rows at: a regular grid across the window, thinned to stay
/// under `MAX_ROWS` so a wide window loses cadence rather than its tail.
fn grid(
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    cadence_seconds: i64,
) -> (Vec<chrono::DateTime<chrono::Utc>>, i64) {
    let span = (to - from).num_seconds().max(0);
    let cadence = cadence_seconds.max(1);
    let wanted = (span / cadence) as usize + 1;
    let step = if wanted > MAX_ROWS {
        cadence * ((wanted as i64 + MAX_ROWS as i64 - 1) / MAX_ROWS as i64)
    } else {
        cadence
    };
    let mut out = Vec::new();
    let mut at = from;
    while at <= to && out.len() < MAX_ROWS {
        out.push(at);
        at += chrono::Duration::seconds(step);
    }
    (out, step)
}

/// Build the keogram pair for one window.
pub fn build(
    s: &AppState,
    a_id: &str,
    b_id: &str,
    from_utc: &str,
    to_utc: &str,
    samples: usize,
    cadence_seconds: i64,
    half_width_km: f64,
) -> Result<Value> {
    let conn = crate::db::open(&s.db_path)?;
    let (a, b) = (station(&conn, a_id)?, station(&conn, b_id)?);
    let separation = separation_km(&a, &b);
    if separation > MAX_SEPARATION_KM {
        return Err(anyhow!(
            "{} and {} are {separation:.0} km apart and share no comparable sky",
            a.name,
            b.name
        ));
    }
    let samples = samples.clamp(2, MAX_SAMPLES);
    let points = cut_points(&a, &b, half_width_km, samples, s.igrf_year)
        .ok_or_else(|| anyhow!("the two stations are antipodal"))?;

    let from = chrono::DateTime::parse_from_rfc3339(from_utc)?.with_timezone(&chrono::Utc);
    let to = chrono::DateTime::parse_from_rfc3339(to_utc)?.with_timezone(&chrono::Utc);
    let (frames_a, frames_b) = (
        frames(&conn, &a.id, from_utc, to_utc)?,
        frames(&conn, &b.id, from_utc, to_utc)?,
    );

    // The arc position of each sample, signed, so the spatial axis can be
    // labelled in kilometres from the midpoint rather than in sample index.
    let arc_km: Vec<f64> = (0..samples)
        .map(|i| {
            let t = if samples == 1 { 0.0 } else { i as f64 / (samples - 1) as f64 };
            -half_width_km + 2.0 * half_width_km * t
        })
        .collect();

    let mut samplers: HashMap<String, Sampler> = HashMap::new();
    let mut rows = Vec::new();
    let mut skipped_unpaired = 0usize;
    let (steps, step_seconds) = grid(from, to, cadence_seconds);
    for at in steps {
        let (Some(fa), Some(fb)) = (nearest(&frames_a, at), nearest(&frames_b, at)) else {
            skipped_unpaired += 1;
            continue;
        };
        if (fa.at - fb.at).num_seconds().abs() > MAX_SKEW_SECONDS {
            skipped_unpaired += 1;
            continue;
        }
        for (station, frame) in [(&a, fa), (&b, fb)] {
            if !samplers.contains_key(&station.id) {
                samplers.insert(station.id.clone(), sampler(s, &station.id, frame.at, &points)?);
            }
        }
        let pa = profile(s, &a.id, fa.at, &samplers[&a.id])?;
        let pb = profile(s, &b.id, fb.at, &samplers[&b.id])?;
        rows.push(json!({
            "at": at.to_rfc3339(),
            "a_at": fa.at.to_rfc3339(),
            "b_at": fb.at.to_rfc3339(),
            "a_image_id": fa.id,
            "b_image_id": fb.id,
            "a": pa,
            "b": pb,
        }));
    }

    let covered = |key: &str| -> f64 {
        samplers
            .get(key)
            .map(|s| s.uv.iter().filter(|v| v.is_some()).count() as f64 / samples.max(1) as f64)
            .unwrap_or(0.0)
    };
    Ok(json!({
        "a": {"id": a.id, "name": a.name, "coverage": covered(&a.id),
              "geometry_url": samplers.get(&a.id).map(|s| s.geometry_url.clone())},
        "b": {"id": b.id, "name": b.name, "coverage": covered(&b.id),
              "geometry_url": samplers.get(&b.id).map(|s| s.geometry_url.clone())},
        "separation_km": separation,
        "colocated": separation < 1.0,
        "half_width_km": half_width_km,
        "samples": samples,
        "cadence_seconds": step_seconds,
        "arc_km": arc_km,
        "from": from.to_rfc3339(),
        "to": to.to_rfc3339(),
        "rows": rows,
        "unpaired_steps": skipped_unpaired,
        // Said plainly, because it bounds what a fit built on this data can
        // claim: these are the composite's own 256 px JPEG textures, not the
        // archived originals.
        "pixel_source": "projection-cache texture (<=256 px, JPEG quality 80)",
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(lat: f64, lon: f64) -> Station {
        Station { id: format!("{lat}:{lon}"), name: "camera".into(), latitude_deg: lat, longitude_deg: lon, altitude_m: 0.0 }
    }

    #[test]
    fn separation_matches_the_known_baseline() {
        // Tromso to Kiruna: 1.81 degrees of latitude and 1.45 of longitude at
        // 68.7 N, so 209 km great-circle. Not the road distance, which is half
        // as long again and was what this test first asserted.
        let km = separation_km(&at(69.65, 18.96), &at(67.84, 20.41));
        assert!((km - 209.3).abs() < 2.0, "separation {km} km");
        assert!(separation_km(&at(69.65, 18.96), &at(69.65, 18.96)) < 1e-6);
    }

    #[test]
    fn the_cut_spans_the_requested_width_about_the_midpoint() {
        let (a, b) = (at(69.65, 18.96), at(67.84, 20.41));
        let points = cut_points(&a, &b, 300.0, 65, 2026).unwrap();
        assert_eq!(points.len(), 65);
        let radius = geometry::EARTH_RADIUS_KM + geometry::EMISSION_ALTITUDE_KM;
        let arc = |p: [f64; 3], q: [f64; 3]| {
            let n = |v: [f64; 3]| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            radius * ((p[0] * q[0] + p[1] * q[1] + p[2] * q[2]) / (n(p) * n(q))).clamp(-1.0, 1.0).acos()
        };
        // End to end is twice the half width, and the centre sample is the middle.
        assert!((arc(points[0], points[64]) - 600.0).abs() < 1.0);
        assert!((arc(points[0], points[32]) - 300.0).abs() < 1.0);
        // Every sample sits on the emission shell.
        for p in &points {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - radius).abs() < 1e-6, "sample off the shell at {r}");
        }
    }

    #[test]
    fn a_co_located_pair_still_yields_a_cut() {
        // Equidistance holds everywhere, so the baseline gives no direction and
        // the magnetic meridian has to supply one. This used to be the case
        // that returned nothing at all.
        let points = cut_points(&at(67.84, 20.41), &at(67.84, 20.41), 200.0, 33, 2026).unwrap();
        assert_eq!(points.len(), 33);
        let spread = points
            .iter()
            .map(|p| p[2])
            .fold(f64::NEG_INFINITY, f64::max)
            - points.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        assert!(spread > 1.0, "co-located cut collapsed to a point");
    }

    #[test]
    fn a_wide_window_loses_cadence_rather_than_its_tail() {
        let from = chrono::DateTime::parse_from_rfc3339("2026-09-12T18:00:00+00:00").unwrap().with_timezone(&chrono::Utc);
        let to = from + chrono::Duration::hours(24);
        let (rows, step_seconds) = grid(from, to, 60);
        assert!(step_seconds >= 60);
        assert!(rows.len() <= MAX_ROWS, "{} rows exceeds the cap", rows.len());
        // The last row still reaches the end of the window, within one step.
        let step = (rows[1] - rows[0]).num_seconds();
        assert!((to - *rows.last().unwrap()).num_seconds() < step);
        // A narrow window keeps the cadence it asked for.
        let (narrow, _) = grid(from, from + chrono::Duration::hours(1), 300);
        assert_eq!((narrow[1] - narrow[0]).num_seconds(), 300);
    }

    #[test]
    fn only_simultaneous_frames_are_paired() {
        let base = chrono::DateTime::parse_from_rfc3339("2026-09-12T20:00:00+00:00").unwrap().with_timezone(&chrono::Utc);
        let list: Vec<Frame> = [0i64, 120, 600]
            .iter()
            .map(|d| Frame { at: base + chrono::Duration::seconds(*d), id: format!("f{d}") })
            .collect();
        assert_eq!(nearest(&list, base + chrono::Duration::seconds(10)).unwrap().id, "f0");
        assert_eq!(nearest(&list, base + chrono::Duration::seconds(110)).unwrap().id, "f120");
        // Nothing within tolerance: a row that would compare sky 5 minutes apart
        // is refused rather than built.
        assert!(nearest(&list, base + chrono::Duration::seconds(300)).is_none());
    }

    #[test]
    fn distant_stations_are_not_offered_as_a_pair() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        let add = |id: &str, name: &str, lat: f64, lon: f64, calibrated: bool| {
            conn.execute(
                "INSERT INTO sources(id,name,kind,url,timestamp_mode,interval_seconds,producer_id,
                    latitude_deg,longitude_deg,altitude_m,enabled,config_json)
                 VALUES(?1,?2,'allsky','http://x.invalid','archive',60,'p',?3,?4,0,1,'{}')",
                rusqlite::params![id, name, lat, lon],
            ).unwrap();
            if calibrated {
                conn.execute(
                    "INSERT INTO calibrations(id,source_id,created_utc,method,hdf5_path)
                     VALUES(?1,?2,'2026-01-01T00:00:00+00:00','AIDA','lens.h5')",
                    rusqlite::params![format!("cal-{id}"), id],
                ).unwrap();
            }
        };
        add("tromso", "Tromso", 69.65, 18.96, true);
        add("kiruna", "Kiruna", 67.84, 20.41, true);
        add("twin", "Kiruna twin", 67.84, 20.41, true);
        add("tokyo", "Tokyo", 35.68, 139.69, true);
        add("uncal", "No calibration", 69.60, 19.00, false);

        let found = pairs(&conn, "tromso").unwrap();
        let ids: Vec<&str> = found.iter().map(|p| p["partner_id"].as_str().unwrap()).collect();
        assert!(ids.contains(&"kiruna"));
        assert!(!ids.contains(&"tokyo"), "a camera 6000 km away was offered");
        assert!(!ids.contains(&"uncal"), "a camera with no calibration has no mesh to sample");
        assert!(!ids.contains(&"tromso"), "a camera was paired with itself");

        // From Kiruna the co-located twin sorts first and is flagged as such.
        let from_kiruna = pairs(&conn, "kiruna").unwrap();
        assert_eq!(from_kiruna[0]["partner_id"], "twin");
        assert_eq!(from_kiruna[0]["colocated"], true);
        assert_eq!(from_kiruna[1]["colocated"], false);
    }
}
