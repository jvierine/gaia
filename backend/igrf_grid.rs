use anyhow::{Context, Result};
use ferromagnetic::igrf::IGRF;
use serde::Serialize;
use std::path::Path;

pub const WIDTH: usize = 360;
pub const HEIGHT: usize = 181;

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
/// 1..13 coefficients, not a centered-dipole approximation. Values are encoded
/// linearly from -90..+90 degrees into bytes 0..255 for a WebGL1 texture.
pub fn load_or_generate(cache_root: &Path, year: i32) -> Result<Vec<u8>> {
    let dir = cache_root.join("igrf");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("igrf14-dip-lat-{year}-{WIDTH}x{HEIGHT}.bin"));
    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == WIDTH * HEIGHT {
            return Ok(bytes);
        }
    }
    let igrf = IGRF::default();
    let mut bytes = Vec::with_capacity(WIDTH * HEIGHT);
    for y in 0..HEIGHT {
        let lat = 90.0 - y as f64;
        for x in 0..WIDTH {
            let lon = -180.0 + x as f64;
            let field = igrf.calc(lat, lon, 0.0, year as f64).result;
            let dip_lat = (0.5 * field.inclination.to_radians().tan())
                .atan()
                .to_degrees();
            bytes.push(
                ((dip_lat + 90.0) * (255.0 / 180.0))
                    .round()
                    .clamp(0.0, 255.0) as u8,
            );
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
        assert_eq!(g.len(), WIDTH * HEIGHT);
        assert!(g.iter().copied().min().unwrap() < 40);
        assert!(g.iter().copied().max().unwrap() > 215);
    }
}
