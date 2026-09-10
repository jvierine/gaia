//! Offline public atlas publisher. No public database or projection API required.
use crate::{AppState, db, geometry, projection};
use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};
/// Keeps the mask-edge fade strictly positive so a single-contributor pixel is
/// unchanged by it: its weight cancels in the per-pixel normalization.
const MASK_FADE_FLOOR: f64 = 1e-6;
const W: u32 = 4096;
const H: u32 = 2048;
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(temp, path)?;
    Ok(())
}
fn publish_lens_models(
    conn: &rusqlite::Connection,
    assets: &Path,
) -> Result<Vec<serde_json::Value>> {
    let mut query=conn.prepare("SELECT s.id,s.name,p.name,s.latitude_deg,s.longitude_deg,c.id,c.created_utc,c.valid_from_utc,c.valid_to_utc,c.method,c.hdf5_path,c.residual_px FROM sources s JOIN producers p ON p.id=s.producer_id JOIN calibrations c ON c.source_id=s.id WHERE s.enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY s.name,julianday(c.valid_from_utc),c.created_utc")?;
    let rows = query.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<f64>>(3)?,
            r.get::<_, Option<f64>>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, String>(6)?,
            r.get::<_, Option<String>>(7)?,
            r.get::<_, Option<String>>(8)?,
            r.get::<_, String>(9)?,
            r.get::<_, String>(10)?,
            r.get::<_, Option<f64>>(11)?,
        ))
    })?;
    let mut cameras: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for row in rows {
        let (
            source_id,
            name,
            producer,
            lat,
            lon,
            id,
            created,
            valid_from,
            valid_to,
            method,
            path,
            residual,
        ) = row?;
        let bytes = std::fs::read(&path)?;
        let sha = format!("{:x}", Sha256::digest(&bytes));
        let filename = format!("lens-{sha}.h5");
        let destination = assets.join(&filename);
        if !destination.exists() {
            atomic(&destination, &bytes)?
        }
        let camera=cameras.entry(source_id.clone()).or_insert_with(||json!({"source_id":source_id,"name":name,"producer":producer,"latitude_deg":lat,"longitude_deg":lon,"models":[]}));
        camera["models"].as_array_mut().unwrap().push(json!({"calibration_id":id,"created_utc":created,"valid_from_utc":valid_from,"valid_to_utc":valid_to,"method":method,"residual_px":residual,"format":"AIDA/WISC HDF5","sha256":sha,"url":format!("/gaia/public/assets/{filename}")}));
    }
    Ok(cameras.into_values().collect())
}
pub fn run(s: &AppState) -> Result<()> {
    let root = s.archive_root.join("public");
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets)?;
    let conn = db::open(&s.db_path)?;
    let cameras=conn.prepare("SELECT id,latitude_deg,longitude_deg,COALESCE(altitude_m,0) FROM sources WHERE enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=sources.id) AND latitude_deg IS NOT NULL AND longitude_deg IS NOT NULL AND EXISTS(SELECT 1 FROM calibrations WHERE source_id=sources.id) ORDER BY id")?.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,f64>(1)?,r.get::<_,f64>(2)?,r.get::<_,f64>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
    // Crop, obstruction outlines and the working-grid size for each camera, used by
    // the mask-edge fade below. The mesh is rasterized on the 256px thumbnail, so the
    // fade width is expressed in those pixels.
    let mut mask_outlines: BTreeMap<String, ([f64; 4], Vec<Vec<[f64; 2]>>, [f64; 2])> =
        BTreeMap::new();
    for (id, _, _, _) in &cameras {
        let (crop_json, mask_json, mask_enabled): (Option<String>, Option<String>, bool) = conn
            .query_row(
                "SELECT c.crop_json,c.mask_json,COALESCE(c.mask_enabled,1) FROM sources s LEFT JOIN camera_settings c ON c.source_id=s.id WHERE s.id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap_or((None, None, true));
        let (raw_w, raw_h): (f64, f64) = conn
            .query_row(
                "SELECT width,height FROM images WHERE source_id=?1 AND width IS NOT NULL AND height IS NOT NULL ORDER BY observation_utc DESC LIMIT 1",
                [id],
                |r| Ok((r.get::<_, i64>(0)? as f64, r.get::<_, i64>(1)? as f64)),
            )
            .unwrap_or((256.0, 256.0));
        let crop = crop_json
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|v| {
                Some([
                    v["left"].as_f64()?,
                    v["top"].as_f64()?,
                    v["right"].as_f64()?,
                    v["bottom"].as_f64()?,
                ])
            })
            .unwrap_or([0.0, 0.0, 1.0, 1.0]);
        // A switched-off mask excludes no pixels, so it must not feather either.
        let polygons = if !mask_enabled { None } else { mask_json }
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .filter(|v| v["coordinate_system"] == "normalized_image")
            .and_then(|v| {
                Some(
                    v["polygons"]
                        .as_array()?
                        .iter()
                        .filter_map(|polygon| {
                            Some(
                                polygon
                                    .as_array()?
                                    .iter()
                                    .filter_map(|p| {
                                        Some([p[0].as_f64()?, p[1].as_f64()?])
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .filter(|p: &Vec<[f64; 2]>| p.len() >= 3)
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_default();
        // image::thumbnail(256,256) fits inside the box and never enlarges.
        let fit = (256.0 / raw_w).min(256.0 / raw_h).min(1.0);
        mask_outlines.insert(id.clone(), (crop, polygons, [raw_w * fit, raw_h * fit]));
    }
    let camera_indices: BTreeMap<String, u32> = cameras
        .iter()
        .enumerate()
        .map(|(i, (id, _, _, _))| (id.clone(), i as u32 + 1))
        .collect();
    let mut mesh = Vec::new();
    for y in 0..90 {
        for x in 0..180 {
            for (dx, dy) in [(0, 0), (1, 0), (1, 1), (0, 0), (1, 1), (0, 1)] {
                let u = (x + dx) as f64 / 180.;
                let v = (y + dy) as f64 / 90.;
                let lon = (u * 360. - 180.).to_radians();
                let lat = (90. - v * 180.).to_radians();
                let r = 1. + 100. / 6371.;
                for f in [
                    r * lat.cos() * lon.sin(),
                    r * lat.sin(),
                    r * lat.cos() * lon.cos(),
                    u,
                    v,
                ] {
                    mesh.extend_from_slice(&(f as f32).to_le_bytes())
                }
            }
        }
    }
    atomic(&assets.join("shell-v1.bin"), &mesh)?;
    atomic(&assets.join(format!("igrf-{}.bin", s.igrf_year)), &s.igrf)?;
    let end = Utc::now().timestamp() / 60 * 60;
    let mut frames = Vec::new();
    let falloff_deg = std::env::var("GAIA_MAGNETIC_FALLOFF_DEG")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(20.0);
    anyhow::ensure!(
        falloff_deg.is_finite() && falloff_deg > 0.0 && falloff_deg <= 90.0,
        "GAIA_MAGNETIC_FALLOFF_DEG must be in (0,90]"
    );
    let falloff_rad = falloff_deg.to_radians();
    // Zenith-angle taper: full weight out to the start angle, smoothly to zero
    // across the width. Keeps horizon-grazing pixels, where the projection
    // smears worst, out of the composite.
    let taper_start_deg = std::env::var("GAIA_ZENITH_TAPER_START_DEG")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(75.0);
    let taper_width_deg = std::env::var("GAIA_ZENITH_TAPER_WIDTH_DEG")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(10.0);
    anyhow::ensure!(
        taper_start_deg.is_finite() && taper_start_deg > 0.0 && taper_start_deg <= 90.0,
        "GAIA_ZENITH_TAPER_START_DEG must be in (0,90]"
    );
    anyhow::ensure!(
        taper_width_deg.is_finite() && taper_width_deg > 0.0 && taper_width_deg <= 90.0,
        "GAIA_ZENITH_TAPER_WIDTH_DEG must be in (0,90]"
    );
    let (taper_start_rad, taper_width_rad) =
        (taper_start_deg.to_radians(), taper_width_deg.to_radians());
    // Soft fade inwards from the crop rectangle and obstruction outlines, measured
    // in pixels of the working grid the projection mesh is rasterized on.
    let mask_fade_px = std::env::var("GAIA_MASK_FADE_PX")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(10.0);
    anyhow::ensure!(
        mask_fade_px.is_finite() && mask_fade_px >= 0.0,
        "GAIA_MASK_FADE_PX must be finite and non-negative"
    );
    // Twilight taper. A whole image is weighted by the solar elevation at its own
    // station: full weight while the sun is at or below the dark angle, easing to a
    // floor once it reaches the light angle. Unlike the other factors this varies
    // with time, so it is applied per frame and never folded into the vertex cache.
    let env_f64 = |name: &str, fallback: f64| {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(fallback)
    };
    let sun_dark_deg = env_f64("GAIA_SOLAR_DARK_DEG", -12.0);
    let sun_light_deg = env_f64("GAIA_SOLAR_LIGHT_DEG", 0.0);
    let sun_floor = env_f64("GAIA_SOLAR_FLOOR", 0.05);
    anyhow::ensure!(
        sun_dark_deg.is_finite() && sun_light_deg.is_finite() && sun_light_deg > sun_dark_deg,
        "GAIA_SOLAR_LIGHT_DEG must be finite and above GAIA_SOLAR_DARK_DEG"
    );
    anyhow::ensure!(
        sun_floor.is_finite() && sun_floor > 0.0 && sun_floor <= 1.0,
        "GAIA_SOLAR_FLOOR must be in (0,1]"
    );
    // Every archived observation minute, rather than throwing four minutes out
    // of five away. Historical regeneration uses exactly the live stitcher.
    let full_archive = std::env::var("GAIA_PUBLISH_ALL").as_deref() == Ok("1");
    let start = if full_archive { 0 } else { end - 86400 };
    let mut query = conn.prepare("SELECT DISTINCT (CAST(strftime('%s',i.observation_utc) AS INTEGER)/60+1)*60 AS epoch FROM images i JOIN sources s ON s.id=i.source_id WHERE s.enabled=1 AND EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) AND CAST(strftime('%s',i.observation_utc) AS INTEGER)>=?1 AND CAST(strftime('%s',i.observation_utc) AS INTEGER)<=?2 ORDER BY epoch")?;
    let mut epochs: Vec<i64> = query.query_map(rusqlite::params![start,end], |r| r.get(0))?.collect::<Result<Vec<_>,_>>()?;
    epochs.retain(|epoch| *epoch<=end);
    epochs.push(end);
    epochs.sort();
    epochs.dedup();
    // Work newest-first so initial publication has a live frame before backfill.
    epochs.reverse();
    let workers = std::env::var("GAIA_PREPROCESS_WORKERS").ok()
        .and_then(|v|v.parse::<usize>().ok()).unwrap_or(16).clamp(1,16)
        .min(epochs.len().max(1));
    let next_epoch = std::sync::atomic::AtomicUsize::new(0);
    let started = std::time::Instant::now();
    tracing::info!(workers, frames=epochs.len(), "Starting parallel atlas preprocessing");
    let batches = std::thread::scope(|scope| -> Result<Vec<Vec<serde_json::Value>>> {
        let mut handles = Vec::new();
        for _ in 0..workers {
            handles.push(scope.spawn(|| -> Result<Vec<serde_json::Value>> {
                let igrf = ferromagnetic::igrf::IGRF::default();
                let mut magnetic_weight_cache: BTreeMap<String, Vec<f32>> = BTreeMap::new();
                let mut frames = Vec::new();
                loop {
                    let index = next_epoch.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(&epoch) = epochs.get(index) else {break};
        let at = Utc.timestamp_opt(epoch, 0).unwrap();
        let mut inputs = Vec::new();
        for (id, lat, lon, alt) in &cameras {
            match projection::assets(s, id, Some(at)) {
                Ok(a) => inputs.push((id, lat, lon, alt, a)),
                Err(e) => tracing::debug!(%id,%e,"No publishable camera frame"),
            }
        }
        if inputs.is_empty() {
            continue;
        }
        let texture_key = format!(
            "{:x}",
            Sha256::digest(format!(
                "atlas-v6-igrf-laplacian-{falloff_deg:.6}-taper-{taper_start_deg:.6}-{taper_width_deg:.6}-maskfade-{mask_fade_px:.6}-sun-{sun_dark_deg:.6}-{sun_light_deg:.6}-{sun_floor:.6}:{epoch}:{}",
                serde_json::to_string(&inputs)?
            ))
        );
        let source_key = format!(
            "{:x}",
            Sha256::digest(format!(
                "source-v6-igrf-laplacian-{falloff_deg:.6}-taper-{taper_start_deg:.6}-{taper_width_deg:.6}-maskfade-{mask_fade_px:.6}-sun-{sun_dark_deg:.6}-{sun_light_deg:.6}-{sun_floor:.6}:{epoch}:{}",
                serde_json::to_string(&inputs)?
            ))
        );
        let name = format!("{texture_key}.webp");
        let dest = assets.join(&name);
        let source_name = format!("source-{source_key}.png");
        let source_dest = assets.join(&source_name);
        if !dest.exists() || !source_dest.exists() {
            let mut out = image::RgbaImage::new(W, H);
            let mut source_pixels = image::RgbImage::new(W, H);
            let mut accum = vec![[0f32; 4]; (W * H) as usize];
            let mut best_weights = vec![-1f32; (W * H) as usize];
            for (id, lat, lon, alt, a) in &inputs {
                let source_index = *camera_indices.get(*id).expect("published camera index");
                let cache = s.archive_root.join("projection-cache");
                let geometry_name = a["geometry_url"]
                    .as_str()
                    .unwrap()
                    .rsplit('/')
                    .next()
                    .unwrap();
                let geo = std::fs::read(cache.join(geometry_name))?;
                let im = image::open(
                    cache.join(
                        a["texture_url"]
                            .as_str()
                            .unwrap()
                            .rsplit('/')
                            .next()
                            .unwrap(),
                    ),
                )?
                .to_rgb8();
                let g: Vec<f32> = geo
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                    .collect();
                let o = geometry::observer_ecef(**lat, **lon, **alt / 1000.);
                // Whole-image twilight weight from the sun at this station, at this frame.
                let sun_weight = geometry::solar_taper(
                    geometry::solar_elevation_deg(**lat, **lon, epoch as f64),
                    sun_dark_deg,
                    sun_light_deg,
                    sun_floor,
                ) as f32;
                if !magnetic_weight_cache.contains_key(geometry_name) {
                    let weight_key = format!(
                        "{:x}",
                        Sha256::digest(format!(
                            "magnetic-weight-v3:{geometry_name}:{}:{falloff_deg:.6}:{taper_start_deg:.6}:{taper_width_deg:.6}:{mask_fade_px:.6}",
                            s.igrf_year
                        ))
                    );
                    let weight_path = cache.join(format!("magnetic-{weight_key}.bin"));
                    let weights = if weight_path.exists() {
                        std::fs::read(&weight_path)?
                            .chunks_exact(4)
                            .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()))
                            .collect::<Vec<_>>()
                    } else {
                        let weights = g
                            .chunks_exact(5)
                            .map(|v| {
                                let hit = [
                                    v[2] as f64 * 6371.,
                                    v[0] as f64 * 6371.,
                                    v[1] as f64 * 6371.,
                                ];
                                let (hit_lat, hit_lon) = geometry::ecef_to_lat_lon(hit);
                                let field =
                                    igrf.calc(hit_lat, hit_lon, 100., s.igrf_year as f64).result;
                                // IGRF inclination is positive down; the axial angle is invariant to field sign.
                                let field_ecef = geometry::az_el_direction(
                                    hit_lat,
                                    hit_lon,
                                    field.declination,
                                    -field.inclination,
                                );
                                let look = [hit[0] - o[0], hit[1] - o[1], hit[2] - o[2]];
                                // Magnetic-axis preference, tapered off towards the horizon.
                                let taper = geometry::zenith_taper(
                                    geometry::zenith_angle(o, look),
                                    taper_start_rad,
                                    taper_width_rad,
                                );
                                // Soft fade in from the crop and obstruction outlines. The
                                // floor keeps the factor strictly positive, which is what
                                // confines the fade to overlaps: where a pixel has a single
                                // contributor the per-pixel normalization divides its weight
                                // out again, so a lone image is never eroded.
                                let fade = match mask_outlines.get(*id) {
                                    Some((crop, polygons, scale)) if mask_fade_px > 0.0 => {
                                        let distance = crate::pixel_mask::boundary_distance_px(
                                            [v[3] as f64, v[4] as f64],
                                            *crop,
                                            polygons,
                                            *scale,
                                        );
                                        geometry::smooth_step(distance / mask_fade_px)
                                            .max(MASK_FADE_FLOOR)
                                    }
                                    _ => 1.0,
                                };
                                (geometry::magnetic_axis_weight(look, field_ecef, falloff_rad)
                                    * taper
                                    * fade) as f32
                            })
                            .collect::<Vec<_>>();
                        let bytes = weights
                            .iter()
                            .flat_map(|value| value.to_le_bytes())
                            .collect::<Vec<_>>();
                        atomic(&weight_path, &bytes)?;
                        weights
                    };
                    anyhow::ensure!(
                        weights.len() == g.len() / 5,
                        "magnetic weight cache length mismatch"
                    );
                    magnetic_weight_cache.insert(geometry_name.to_string(), weights);
                }
                let magnetic_weights = &magnetic_weight_cache[geometry_name];
                for (triangle_index, t) in g.chunks_exact(15).enumerate() {
                    let mut p = [[0f64; 2]; 3];
                    for k in 0..3 {
                        let x = t[k * 5] as f64;
                        let y = t[k * 5 + 1] as f64;
                        let z = t[k * 5 + 2] as f64;
                        p[k] = [
                            (x.atan2(z) / std::f64::consts::TAU + 0.5) * W as f64,
                            (0.5 - (y / (x * x + y * y + z * z).sqrt()).clamp(-1., 1.).asin()
                                / std::f64::consts::PI)
                                * H as f64,
                        ];
                    }
                    for k in 1..3 {
                        while p[k][0] - p[0][0] > W as f64 / 2. {
                            p[k][0] -= W as f64
                        }
                        while p[k][0] - p[0][0] < -(W as f64) / 2. {
                            p[k][0] += W as f64
                        }
                    }
                    let den = (p[1][1] - p[2][1]) * (p[0][0] - p[2][0])
                        + (p[2][0] - p[1][0]) * (p[0][1] - p[2][1]);
                    if den.abs() < 1e-9 {
                        continue;
                    }
                    let xmin = p.iter().map(|p| p[0].floor() as i32).min().unwrap();
                    let xmax = p.iter().map(|p| p[0].ceil() as i32).max().unwrap();
                    let ymin = p.iter().map(|p| p[1].floor() as i32).min().unwrap().max(0);
                    let ymax = p
                        .iter()
                        .map(|p| p[1].ceil() as i32)
                        .max()
                        .unwrap()
                        .min(H as i32 - 1);
                    for y in ymin..=ymax {
                        for x in xmin..=xmax {
                            let a = ((p[1][1] - p[2][1]) * (x as f64 + 0.5 - p[2][0])
                                + (p[2][0] - p[1][0]) * (y as f64 + 0.5 - p[2][1]))
                                / den;
                            let b = ((p[2][1] - p[0][1]) * (x as f64 + 0.5 - p[2][0])
                                + (p[0][0] - p[2][0]) * (y as f64 + 0.5 - p[2][1]))
                                / den;
                            let c = 1. - a - b;
                            if a < 0. || b < 0. || c < 0. {
                                continue;
                            }
                            let q = [a, b, c];
                            let v = |axis: usize| {
                                (0..3).map(|k| q[k] * t[k * 5 + axis] as f64).sum::<f64>()
                            };
                            let weight = (0..3)
                                .map(|k| q[k] as f32 * magnetic_weights[triangle_index * 3 + k])
                                .sum::<f32>()
                                * sun_weight;
                            if weight <= 0. {
                                continue;
                            }
                            let xx = x.rem_euclid(W as i32) as u32;
                            let idx = (y as u32 * W + xx) as usize;
                            let tx =
                                (v(3) * im.width() as f64).clamp(0., im.width() as f64 - 1.) as u32;
                            let ty = (v(4) * im.height() as f64).clamp(0., im.height() as f64 - 1.)
                                as u32;
                            let rgb = im.get_pixel(tx, ty).0;
                            for channel in 0..3 {
                                accum[idx][channel] += weight * rgb[channel] as f32
                            }
                            accum[idx][3] += weight;
                            if weight > best_weights[idx] {
                                best_weights[idx] = weight;
                                source_pixels.put_pixel(
                                    xx,
                                    y as u32,
                                    image::Rgb([
                                        ((source_index >> 16) & 255) as u8,
                                        ((source_index >> 8) & 255) as u8,
                                        (source_index & 255) as u8,
                                    ]),
                                );
                            }
                        }
                    }
                }
            }
            for (idx, sum) in accum.iter().enumerate() {
                if sum[3] > 0. {
                    let x = idx as u32 % W;
                    let y = idx as u32 / W;
                    out.put_pixel(
                        x,
                        y,
                        image::Rgba([
                            (sum[0] / sum[3]).round().clamp(0., 255.) as u8,
                            (sum[1] / sum[3]).round().clamp(0., 255.) as u8,
                            (sum[2] / sum[3]).round().clamp(0., 255.) as u8,
                            255,
                        ]),
                    );
                }
            }
            let temp = dest.with_extension("pending");
            out.save_with_format(&temp, image::ImageFormat::WebP)?;
            std::fs::rename(temp, dest)?;
            let source_small = image::imageops::resize(
                &source_pixels,
                W / 4,
                H / 4,
                image::imageops::FilterType::Nearest,
            );
            let source_temp = source_dest.with_extension("pending");
            source_small.save_with_format(&source_temp, image::ImageFormat::Png)?;
            std::fs::rename(source_temp, source_dest)?;
        }
        frames.push(json!({"at":at.to_rfc3339(),"width":W,"height":H,"texture_url":format!("/gaia/public/assets/{name}"),"source_map_url":format!("/gaia/public/assets/{source_name}"),"contributors":inputs.iter().map(|(id,_,_,_,a)|json!({"source_id":id,"observation_utc":a["observation_utc"],"calibration_id":a["calibration_id"]})).collect::<Vec<_>>()}));
    }
                Ok(frames)
            }));
        }
        handles.into_iter().map(|handle| handle.join()
            .map_err(|_|anyhow::anyhow!("atlas worker panicked"))?).collect()
    })?;
    for batch in batches {frames.extend(batch);}
    frames.sort_by(|a,b|a["at"].as_str().cmp(&b["at"].as_str()));
    tracing::info!(workers, frames=frames.len(), seconds=started.elapsed().as_secs_f64(), "Parallel atlas preprocessing complete");
    anyhow::ensure!(
        !frames.is_empty(),
        "No composites available; retaining previous publication"
    );
    let credits=conn.prepare("SELECT DISTINCT p.name,p.website,p.acknowledgement,p.copyright FROM producers p JOIN sources s ON s.producer_id=p.id")?.query_map([],|r|Ok(json!({"name":r.get::<_,String>(0)?,"website":r.get::<_,Option<String>>(1)?,"acknowledgement":r.get::<_,String>(2)?,"copyright":r.get::<_,String>(3)?})))?.collect::<Result<Vec<_>,_>>()?;
    let public_cameras=conn.prepare("SELECT s.id,s.name,p.name,COALESCE(p.institution,p.name),COALESCE(NULLIF(p.website,''),s.url),s.latitude_deg,s.longitude_deg,p.acknowledgement,p.copyright,EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id) FROM sources s JOIN producers p ON p.id=s.producer_id WHERE s.enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY p.name,s.name")?.query_map([],|r|{
        let id=r.get::<_,String>(0)?;
        Ok(json!({"source_id":id,"name":r.get::<_,String>(1)?,"producer":r.get::<_,String>(2)?,"institution":r.get::<_,String>(3)?,"website_url":r.get::<_,String>(4)?,"latitude_deg":r.get::<_,Option<f64>>(5)?,"longitude_deg":r.get::<_,Option<f64>>(6)?,"acknowledgement":r.get::<_,String>(7)?,"copyright":r.get::<_,String>(8)?,"calibrated":r.get::<_,bool>(9)?,"map_index":camera_indices.get(&id)}))
    })?.collect::<Result<Vec<_>,_>>()?;
    let lens_models = publish_lens_models(&conn, &assets)?;
    let lens_model_documentation = json!({"format":"AIDA/WISC HDF5","recommended_dataset":"/wisc_optpar_with_optmod","dimension_attributes":["image_width","image_height"],"pixel_coordinates":"zero-based raw image pixel centers","azimuth":"degrees clockwise from geographic north","elevation":"degrees above horizon","validity_interval":"valid_from_utc inclusive, valid_to_utc exclusive; null is open","python_mapper":"https://github.com/jvierine/widefield-star-calibrator/blob/main/wisc_lens.py"});
    let stitching = json!({"model":"IGRF-14 magnetic-axis Laplacian blend","field_evaluation_altitude_km":100.0,"theta_definition":"atan2(|u cross B|, u dot B), folded to min(theta, pi-theta)","weight":"exp(-abs(theta_B)/S) * zenith_taper(z_a) * mask_edge_fade(d) * solar_taper(sun_elevation)","zenith_taper":"1 - psi(u)/(psi(u)+psi(1-u)) with u=(z_a-Z0)/Zw and psi(x)=exp(-1/abs(x)) for x>0 else 0","zenith_taper_start_deg":taper_start_deg,"zenith_taper_width_deg":taper_width_deg,"mask_edge_fade":"smooth_step(d/F) floored at 1e-6, d the working-grid pixel distance to the crop or obstruction outline; per-pixel normalization confines it to overlaps","mask_edge_fade_px":mask_fade_px,"solar_taper":"F + (1-F) * (1 - psi(u)/(psi(u)+psi(1-u))) with u=(elevation-D)/(L-D); whole-image weight from the solar elevation at the camera station","solar_taper_dark_deg":sun_dark_deg,"solar_taper_light_deg":sun_light_deg,"solar_taper_floor":sun_floor,"zenith_angle_definition":"angle at the camera between its local vertical and the line of sight to the shell point","falloff_angle_deg":falloff_deg,"normalization":"per output pixel, divide every contributing weight by their sum","source_map":"dominant magnetic weight"});
    let manifest = json!({"generated_utc":Utc::now().to_rfc3339(),"width":W,"height":H,"geometry_url":"/gaia/public/assets/shell-v1.bin","vertex_count":mesh.len()/20,"images":frames,"credits":credits,"cameras":public_cameras,"lens_models":lens_models,"lens_model_documentation":lens_model_documentation,"stitching":stitching,"igrf_url":format!("/gaia/public/assets/igrf-{}.bin",s.igrf_year)});
    let name=if full_archive { "archive-manifest.json" } else { "manifest.json" };
    atomic(&root.join(name), &serde_json::to_vec(&manifest)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_edge_fade_spans_ten_pixels_and_stays_positive() {
        // Composed exactly as the weight loop does it: pixel distance to the kept
        // region, the same smooth step used by the zenith taper, then the floor.
        let (scale, crop, width) = ([256.0, 256.0], [0.0, 0.0, 1.0, 1.0], 10.0);
        let fade = |u: f64| {
            let d = crate::pixel_mask::boundary_distance_px([u, 0.5], crop, &[], scale);
            geometry::smooth_step(d / width).max(MASK_FADE_FLOOR)
        };
        // On the outline the fade is at the floor, not zero: a lone contributor's
        // weight must stay positive or the pixel would be dropped entirely.
        assert_eq!(fade(0.0), MASK_FADE_FLOOR);
        assert!(MASK_FADE_FLOOR > 0.0, "the floor is what confines the fade to overlaps");
        // Half weight halfway across the ramp, full weight exactly ten pixels in.
        assert!((fade(5.0 / 256.0) - 0.5).abs() < 1e-12);
        assert_eq!(fade(10.0 / 256.0), 1.0);
        assert_eq!(fade(0.5), 1.0);
        // Monotone across the ramp.
        let mut previous = 0.0;
        for step in 0..=100 {
            let value = fade(step as f64 * 0.1 / 256.0);
            assert!(value >= previous - 1e-15);
            previous = value;
        }
    }

    #[test]
    fn one_contributor_normalizes_the_fade_away() {
        // Σ(w·rgb)/Σw with a single contributor returns rgb for any positive w,
        // which is why a faded edge is only visible where images overlap.
        for fade in [MASK_FADE_FLOOR, 1e-3, 0.5, 1.0] {
            let w = (0.37 * fade) as f32;
            assert!(w > 0.0);
            assert!(((w * 200.0) / w - 200.0).abs() < 1e-2, "fade {fade} eroded a lone image");
        }
        // In an overlap the faded edge yields to its unfaded neighbour.
        let (near, edge) = (1.0f32, MASK_FADE_FLOOR as f32);
        let blended = (near * 200.0 + edge * 40.0) / (near + edge);
        assert!((blended - 200.0).abs() < 1e-2);
    }
}
