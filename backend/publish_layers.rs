//! Publish independent, masked 256px camera textures and 100km meshes.
//! No cross-camera rasterization or final image composition runs here.
use crate::{AppState, db, geometry, projection};
use anyhow::Result;
use chrono::{TimeZone, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};
fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.pending", uuid::Uuid::new_v4()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
fn copy(from: &Path, to: &Path) -> Result<()> {
    if !to.exists() {
        atomic(to, &std::fs::read(from)?)?
    }
    Ok(())
}
#[derive(Clone, serde::Serialize)]
struct Rules {
    falloff_deg: f64,
    taper_start_deg: f64,
    taper_width_deg: f64,
    mask_fade_px: f64,
    sun_dark_deg: f64,
    sun_light_deg: f64,
    sun_floor: f64,
}
impl Rules {
    fn load() -> Result<Self> {
        let env = |name: &str, default: f64| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(default)
        };
        let r = Self {
            falloff_deg: env("GAIA_MAGNETIC_FALLOFF_DEG", 20.),
            taper_start_deg: env("GAIA_ZENITH_TAPER_START_DEG", 75.),
            taper_width_deg: env("GAIA_ZENITH_TAPER_WIDTH_DEG", 10.),
            mask_fade_px: env("GAIA_MASK_FADE_PX", 10.),
            sun_dark_deg: env("GAIA_SOLAR_DARK_DEG", -12.),
            sun_light_deg: env("GAIA_SOLAR_LIGHT_DEG", 0.),
            sun_floor: env("GAIA_SOLAR_FLOOR", 0.05),
        };
        for v in [r.falloff_deg, r.taper_start_deg, r.taper_width_deg] {
            anyhow::ensure!(
                v.is_finite() && v > 0. && v <= 90.,
                "Invalid angular blend rule"
            )
        }
        anyhow::ensure!(
            r.mask_fade_px.is_finite()
                && r.mask_fade_px >= 0.
                && r.sun_dark_deg.is_finite()
                && r.sun_light_deg.is_finite()
                && r.sun_dark_deg < r.sun_light_deg
                && r.sun_floor.is_finite()
                && r.sun_floor > 0.
                && r.sun_floor <= 1.,
            "Invalid mask/solar blend rule"
        );
        Ok(r)
    }
}
fn weighted_mesh(
    s: &AppState,
    source: &str,
    a: &Value,
    lat: f64,
    lon: f64,
    alt: f64,
    assets: &Path,
    rules: &Rules,
) -> Result<(String, u64)> {
    let cache = s.archive_root.join("projection-cache");
    let name = a["geometry_url"]
        .as_str()
        .unwrap()
        .rsplit('/')
        .next()
        .unwrap();
    let magnetic_key = format!(
        "{:x}",
        Sha256::digest(format!(
            "magnetic-weight-v3:{name}:{}:{:.6}:{:.6}:{:.6}:{:.6}",
            s.igrf_year,
            rules.falloff_deg,
            rules.taper_start_deg,
            rules.taper_width_deg,
            rules.mask_fade_px
        ))
    );
    let key = format!(
        "{:x}",
        Sha256::digest(format!("browser-mesh-v2:{name}:{magnetic_key}"))
    );
    let output = format!("layer-{key}.bin");
    let dest = assets.join(&output);
    // The anonymous audience reuses an allowed camera's immutable preparation;
    // filtering the source list happened before this call.
    if !dest.exists() {
        let shared = s.archive_root.join("public/assets").join(&output);
        if shared.exists() {
            copy(&shared, &dest)?
        }
    }
    if !dest.exists() {
        let conn = db::open(&s.db_path)?;
        let (crop,mask,enabled):(Option<String>,Option<String>,bool)=conn.query_row("SELECT crop_json,mask_json,COALESCE(mask_enabled,1) FROM camera_settings WHERE source_id=?1",[source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap_or((None,None,true));
        let crop = crop
            .and_then(|x| serde_json::from_str::<Value>(&x).ok())
            .map(|v| {
                [
                    v["left"].as_f64().unwrap_or(0.),
                    v["top"].as_f64().unwrap_or(0.),
                    v["right"].as_f64().unwrap_or(1.),
                    v["bottom"].as_f64().unwrap_or(1.),
                ]
            })
            .unwrap_or([0., 0., 1., 1.]);
        let polygons: Vec<Vec<[f64; 2]>> = if enabled {
            mask.and_then(|x| serde_json::from_str::<Value>(&x).ok())
                .and_then(|v| serde_json::from_value(v["polygons"].clone()).ok())
                .unwrap_or_default()
        } else {
            vec![]
        };
        let (raw_w,raw_h):(f64,f64)=conn.query_row("SELECT width,height FROM images WHERE source_id=?1 AND width IS NOT NULL AND height IS NOT NULL ORDER BY observation_utc DESC LIMIT 1",[source],|r|Ok((r.get::<_,i64>(0)? as f64,r.get::<_,i64>(1)? as f64))).unwrap_or((256.,256.));
        let fit = (256. / raw_w).min(256. / raw_h).min(1.);
        let (w, h) = (raw_w * fit, raw_h * fit);
        let cached = std::fs::read(cache.join(format!("magnetic-{magnetic_key}.bin"))).ok();
        let geo = std::fs::read(cache.join(name))?;
        let origin = geometry::observer_ecef(lat, lon, alt / 1000.);
        let igrf = ferromagnetic::igrf::IGRF::default();
        let mut bytes = Vec::with_capacity(geo.len() / 20 * 24);
        if let Some(ref weights) = cached {
            anyhow::ensure!(
                weights.len() == geo.len() / 5,
                "Legacy weight cache length mismatch"
            );
        }
        for (index, vertex) in geo.chunks_exact(20).enumerate() {
            if let Some(ref weights) = cached {
                bytes.extend_from_slice(vertex);
                bytes.extend_from_slice(&weights[index * 4..index * 4 + 4]);
                continue;
            }
            let v: Vec<f32> = vertex
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .collect();
            let hit = [
                v[2] as f64 * 6371.,
                v[0] as f64 * 6371.,
                v[1] as f64 * 6371.,
            ];
            let (la, lo) = geometry::ecef_to_lat_lon(hit);
            let f = igrf.calc(la, lo, 100., s.igrf_year as f64).result;
            let field = geometry::az_el_direction(la, lo, f.declination, -f.inclination);
            let look = [hit[0] - origin[0], hit[1] - origin[1], hit[2] - origin[2]];
            let fade = if rules.mask_fade_px > 0. {
                geometry::smooth_step(
                    crate::pixel_mask::boundary_distance_px(
                        [v[3] as f64, v[4] as f64],
                        crop,
                        &polygons,
                        [w, h],
                    ) / rules.mask_fade_px,
                )
                .max(1e-6)
            } else {
                1.
            };
            let weight =
                (geometry::magnetic_axis_weight(look, field, rules.falloff_deg.to_radians())
                    * geometry::zenith_taper(
                        geometry::zenith_angle(origin, look),
                        rules.taper_start_deg.to_radians(),
                        rules.taper_width_deg.to_radians(),
                    )
                    * fade) as f32;
            bytes.extend_from_slice(vertex);
            bytes.extend_from_slice(&weight.to_le_bytes());
        }
        atomic(&dest, &bytes)?;
    }
    Ok((output, std::fs::metadata(dest)?.len() / 24))
}
pub fn run(s: &AppState) -> Result<()> {
    let rules = Rules::load()?;
    let started = std::time::Instant::now();
    let anonymous = std::env::var("GAIA_PUBLISH_AUDIENCE").as_deref() == Ok("anonymous");
    let audience = if anonymous { "open" } else { "public" };
    let root = s.archive_root.join(audience);
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets)?;
    let conn = db::open(&s.db_path)?;
    let restricted=conn.prepare("SELECT s.id FROM sources s JOIN producers p ON p.id=s.producer_id WHERE lower(s.id || ' ' || s.url || ' ' || p.name || ' ' || COALESCE(p.website,'')) LIKE '%starvis%'")?.query_map([],|r|r.get::<_,String>(0))?.collect::<Result<BTreeSet<_>,_>>()?;
    let mut cameras=conn.prepare("SELECT s.id,s.name,p.name,COALESCE(p.institution,p.name),COALESCE(NULLIF(p.website,''),s.url),s.latitude_deg,s.longitude_deg,COALESCE(s.altitude_m,0),p.acknowledgement,p.copyright,EXISTS(SELECT 1 FROM calibrations c WHERE c.source_id=s.id),COALESCE(cs.quality_exponent,0) FROM sources s JOIN producers p ON p.id=s.producer_id LEFT JOIN camera_settings cs ON cs.source_id=s.id WHERE s.enabled=1 AND NOT EXISTS(SELECT 1 FROM removed_sources r WHERE r.source_id=s.id) ORDER BY s.id")?.query_map([],|r|Ok(json!({"source_id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"producer":r.get::<_,String>(2)?,"institution":r.get::<_,String>(3)?,"website_url":r.get::<_,String>(4)?,"latitude_deg":r.get::<_,Option<f64>>(5)?,"longitude_deg":r.get::<_,Option<f64>>(6)?,"altitude_m":r.get::<_,f64>(7)?,"acknowledgement":r.get::<_,String>(8)?,"copyright":r.get::<_,String>(9)?,"calibrated":r.get::<_,bool>(10)?,"quality_exponent":r.get::<_,i64>(11)?})))?.collect::<Result<Vec<_>,_>>()?;
    for camera in &mut cameras {
        camera["imagery_restricted"] = json!(restricted.contains(camera["source_id"].as_str().unwrap()));
    }
    let full = std::env::var("GAIA_PUBLISH_ALL").as_deref() == Ok("1");
    let filename = if full {
        "archive-manifest.json"
    } else {
        "manifest.json"
    };
    // Never make a live preview wait for an interrupted, not-yet-delivered
    // historical publication. Retain only the last verified public snapshot.
    let previous: Value = std::fs::read(root.join(if full {
        filename
    } else {
        "verified-manifest.json"
    }))
    .ok()
    .and_then(|b| serde_json::from_slice(&b).ok())
    .unwrap_or(Value::Null);
    let end = Utc::now().timestamp() / 60 * 60;
    let lookback = std::env::var("GAIA_PUBLISH_LOOKBACK_SECONDS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(86400)
        .clamp(60, 86400);
    let start = if full { 0 } else { end - lookback };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::env::var("GAIA_PREPROCESS_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(16)
        .clamp(1, 16);
    let batches = std::thread::scope(|scope| -> Result<Vec<Vec<Value>>> {
        let mut handles = vec![];
        for _ in 0..workers {
            handles.push(scope.spawn(||->Result<Vec<Value>>{
        let conn=db::open(&s.db_path)?;let mut out=vec![];
        loop{let index=next.fetch_add(1,std::sync::atomic::Ordering::Relaxed);let Some(camera)=cameras.get(index)else{break};let mut camera=camera.clone();camera["map_index"]=json!(index+1);let id=camera["source_id"].as_str().unwrap().to_string();
            // Public station metadata is allowed, but never prepare or retain its imagery.
            if anonymous && restricted.contains(&id) {out.push(camera);continue;}
            let (Some(lat),Some(lon),Some(true))=(camera["latitude_deg"].as_f64(),camera["longitude_deg"].as_f64(),camera["calibrated"].as_bool())else{out.push(camera);continue};
            let mut query=conn.prepare("SELECT MAX(observation_utc) FROM images WHERE source_id=?1 AND CAST(strftime('%s',observation_utc) AS INTEGER)>=?2 AND CAST(strftime('%s',observation_utc) AS INTEGER)<=?3 GROUP BY CAST(strftime('%s',observation_utc) AS INTEGER)/60 ORDER BY MAX(observation_utc) DESC")?;
            let times=query.query_map(rusqlite::params![id,start-600,end],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;let mut images=vec![];
            for time in times{let at=chrono::DateTime::parse_from_rfc3339(&time)?.with_timezone(&Utc);match projection::assets(s,&id,Some(at)){Ok(a)=>{
                let (mesh,count)=weighted_mesh(s,&id,&a,lat,lon,camera["altitude_m"].as_f64().unwrap_or(0.),&assets,&rules)?;let texture=a["texture_url"].as_str().unwrap().rsplit('/').next().unwrap();copy(&s.archive_root.join("projection-cache").join(texture),&assets.join(texture))?;
                images.push(json!({"source_id":id,"at":a["observation_utc"],"geometry_url":format!("/gaia/{audience}/assets/{mesh}"),"texture_url":format!("/gaia/{audience}/assets/{texture}"),"vertex_count":count,"calibration_id":a["calibration_id"]}));
            },Err(e)=>tracing::debug!(%id,%time,%e,"No calibrated projection")}}
            if !full&&lookback<86400&&previous["composition"]=="browser-layers-v1"{if let Some(old)=previous["cameras"].as_array().and_then(|cs|cs.iter().find(|c|c["source_id"]==id)).and_then(|c|c["projection"]["images"].as_array()){for f in old{if let Some(t)=f["at"].as_str().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()){if t.timestamp()>=end-86400&&t.timestamp()<start-600{images.push(f.clone())}}}}}
            for frame in &mut images {frame["source_id"]=json!(id);}
            images.sort_by(|a,b|a["at"].as_str().cmp(&b["at"].as_str()));images.dedup_by(|a,b|a["at"]==b["at"]);
            camera["projection"]=json!({"stride":24,"images":images});out.push(camera);
        }Ok(out)
    }));
        }
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .map_err(|_| anyhow::anyhow!("layer worker panicked"))?
            })
            .collect()
    })?;
    let mut cameras: Vec<Value> = batches.into_iter().flatten().collect();
    cameras.sort_by(|a, b| a["source_id"].as_str().cmp(&b["source_id"].as_str()));
    let mut epochs = BTreeSet::new();
    for c in &cameras {
        if let Some(images) = c["projection"]["images"].as_array() {
            for f in images {
                let t =
                    chrono::DateTime::parse_from_rfc3339(f["at"].as_str().unwrap())?.timestamp();
                let t = (t / 60 + 1) * 60;
                if t <= end && (full || t >= end - 86400) {
                    epochs.insert(t);
                }
            }
        }
    }
    anyhow::ensure!(
        !epochs.is_empty(),
        "No camera layers available; retaining previous manifest"
    );
    let images: Vec<Value> = epochs
        .into_iter()
        .map(|t| json!({"at":Utc.timestamp_opt(t,0).unwrap().to_rfc3339(),"contributors":[]}))
        .collect();
    let excluded = if anonymous {
        restricted
    } else {
        BTreeSet::new()
    };
    let mut lenses = crate::publish::publish_lens_models(&conn, &assets, &excluded)?;
    if anonymous {
        fn rewrite(v: &mut Value) {
            match v {
                Value::String(s) => *s = s.replace("/gaia/public/assets/", "/gaia/open/assets/"),
                Value::Array(a) => {
                    for v in a {
                        rewrite(v)
                    }
                }
                Value::Object(o) => {
                    for v in o.values_mut() {
                        rewrite(v)
                    }
                }
                _ => {}
            }
        }
        for l in &mut lenses {
            rewrite(l)
        }
    }
    let igrf = format!("igrf-{}.bin", s.igrf_year);
    copy(
        &s.archive_root.join("public/assets").join(&igrf),
        &assets.join(&igrf),
    )
    .or_else(|_| atomic(&assets.join(&igrf), &s.igrf))?;
    let manifest = json!({"composition":"browser-layers-v1","generated_utc":Utc::now().to_rfc3339(),"audience":if anonymous{"anonymous-no-starvisor"}else{"full"},"cameras":cameras,"images":images,"lens_models":lenses,"igrf_url":format!("/gaia/{audience}/assets/{igrf}"),"stitching":{"location":"browser WebGL","model":"IGRF-14 magnetic-axis Laplacian blend","rules":rules,"weight":"exp(-abs(theta_B)/S) * zenith_taper * mask_edge_fade * solar_taper * 2^quality_exponent","normalization":"sum(weight * RGB) / sum(weight); omit nonpositive weights","solar_time":"selected composition epoch, not camera acquisition epoch","altitude_km":100,"maximum_camera_texture_dimension":256,"geometry_stride_bytes":24}});
    atomic(&root.join(filename), &serde_json::to_vec(&manifest)?)?;
    tracing::info!(
        workers,
        seconds = started.elapsed().as_secs_f64(),
        "Independent camera layer publication complete"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_solar_reference() {
        let mut cases = vec![];
        for (lat, lon) in [(69.58, 19.22), (-67., 120.), (0., -75.)] {
            for time in [
                "2026-09-11T00:00:00Z",
                "2026-09-11T12:00:00Z",
                "2026-12-21T06:00:00Z",
            ] {
                let epoch = chrono::DateTime::parse_from_rfc3339(time)
                    .unwrap()
                    .timestamp();
                let elevation = geometry::solar_elevation_deg(lat, lon, epoch as f64);
                for quality in [0, -3, -8] {
                    cases.push(json!({"latitude_deg":lat,"longitude_deg":lon,"quality_exponent":quality,"epoch":epoch*1000,"elevation":elevation,"scale":geometry::solar_taper(elevation,-12.,0.,0.05)*2f64.powi(quality)}));
                }
            }
        }
        println!(
            "GAIA_RULES_REFERENCE={}",
            serde_json::to_string(&cases).unwrap()
        );
    }
}
