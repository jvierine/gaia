use anyhow::{Context, Result};
use ferromagnetic::igrf::IGRF;
use serde::Serialize;
use std::path::Path;

pub const WIDTH: usize = 720;
pub const HEIGHT: usize = 361;

#[derive(Serialize)]
pub struct GridMetadata {
    pub model: &'static str,
    pub coordinate: &'static str,
    pub year: i32,
    pub width: usize,
    pub height: usize,
    pub spacing_degrees: i32,
}

/// IGRF-14 magnetic dip latitude at the WGS84 surface. This uses all degree/order
/// 1..13 coefficients, not a centered-dipole approximation. Half-degree samples
/// are little-endian float32 degrees, preserving precision for vector contours.
pub fn load_or_generate(cache_root: &Path, year: i32) -> Result<Vec<u8>> {
    let dir = cache_root.join("igrf");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("igrf14-dip-lat-f32-{year}-{WIDTH}x{HEIGHT}.bin"));
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == WIDTH * HEIGHT * 4 {
            return Ok(bytes);
        }
    }
    let igrf = IGRF::default();
    let mut bytes = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for y in 0..HEIGHT {
        let lat = 90.0 - y as f64 * 0.5;
        for x in 0..WIDTH {
            let lon = -180.0 + x as f64 * 0.5;
            let field = igrf.calc(lat, lon, 0.0, year as f64).result;
            let dip_lat = (0.5 * field.inclination.to_radians().tan())
                .atan()
                .to_degrees();
            bytes.extend_from_slice(&(dip_lat as f32).to_le_bytes());
        }
    }
    let tmp = dir.join(format!(".igrf14-{year}.tmp"));
    std::fs::write(&tmp, &bytes).context("write IGRF grid")?;
    std::fs::rename(tmp, path)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn full_grid_has_expected_shape_and_range() {
        let d = tempfile::tempdir().unwrap();
        let g = load_or_generate(d.path(), 2026).unwrap();
        assert_eq!(g.len(), WIDTH * HEIGHT * 4);
        let values:Vec<f32>=g.chunks_exact(4).map(|b|f32::from_le_bytes(b.try_into().unwrap())).collect();
        assert!(values.iter().all(|v|v.is_finite()&&v.abs()<=90.));
        assert!(values.iter().any(|v|*v < -60.));
        assert!(values.iter().any(|v|*v > 60.));
        assert_eq!(load_or_generate(d.path(),2026).unwrap(),g);
    }
}
