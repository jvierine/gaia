//! Geometry and weighting for projection onto the 100 km emission shell.
use std::f64::consts::PI;

pub const EARTH_RADIUS_KM: f64 = 6371.0;
pub const EMISSION_ALTITUDE_KM: f64 = 100.0;

fn norm(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn add(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2]]
}

pub fn observer_ecef(lat_deg: f64, lon_deg: f64, altitude_km: f64) -> [f64; 3] {
    let (lat, lon) = (lat_deg.to_radians(), lon_deg.to_radians());
    let r = EARTH_RADIUS_KM + altitude_km;
    [
        r * lat.cos() * lon.cos(),
        r * lat.cos() * lon.sin(),
        r * lat.sin(),
    ]
}

pub fn az_el_direction(lat_deg: f64, lon_deg: f64, az_deg: f64, el_deg: f64) -> [f64; 3] {
    let (lat, lon, az, el) = (
        lat_deg.to_radians(),
        lon_deg.to_radians(),
        az_deg.to_radians(),
        el_deg.to_radians(),
    );
    let east = [-lon.sin(), lon.cos(), 0.0];
    let north = [-lat.sin() * lon.cos(), -lat.sin() * lon.sin(), lat.cos()];
    let up = [lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin()];
    norm([
        east[0] * el.cos() * az.sin() + north[0] * el.cos() * az.cos() + up[0] * el.sin(),
        east[1] * el.cos() * az.sin() + north[1] * el.cos() * az.cos() + up[1] * el.sin(),
        east[2] * el.cos() * az.sin() + north[2] * el.cos() * az.cos() + up[2] * el.sin(),
    ])
}

pub fn intersect_emission_shell(
    origin: [f64; 3],
    direction: [f64; 3],
    altitude_km: f64,
) -> Option<[f64; 3]> {
    let r = EARTH_RADIUS_KM + altitude_km;
    let b = 2.0 * dot(origin, direction);
    let c = dot(origin, origin) - r * r;
    let disc = b * b - 4.0 * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b + disc.sqrt()) / 2.0;
    if t <= 0.0 {
        return None;
    }
    Some(add(origin, direction, t))
}

pub fn ecef_to_lat_lon(p: [f64; 3]) -> (f64, f64) {
    let p = norm(p);
    (p[2].asin() * 180.0 / PI, p[1].atan2(p[0]) * 180.0 / PI)
}

/// Centered-dipole magnetic zenith. Replace through the IGRF adapter when epoch coefficients are available.
pub fn magnetic_zenith_direction(lat_deg: f64, lon_deg: f64) -> [f64; 3] {
    let p = norm(observer_ecef(lat_deg, lon_deg, 0.0));
    let pole = observer_ecef(80.65, -72.68, 0.0);
    let m = norm(pole);
    // Dipole field direction, flipped outward for the visible magnetic zenith.
    let b = [
        3.0 * p[0] * dot(m, p) - m[0],
        3.0 * p[1] * dot(m, p) - m[1],
        3.0 * p[2] * dot(m, p) - m[2],
    ];
    let b = norm(b);
    if dot(b, p) < 0.0 {
        [-b[0], -b[1], -b[2]]
    } else {
        b
    }
}

pub fn magnetic_zenith_weight(
    los: [f64; 3],
    lat_deg: f64,
    lon_deg: f64,
    clear_probability: f64,
    obstruction_probability: f64,
) -> f64 {
    let alignment = dot(norm(los), magnetic_zenith_direction(lat_deg, lon_deg)).max(0.0);
    alignment.powf(6.0)
        * clear_probability.clamp(0.0, 1.0)
        * (1.0 - obstruction_probability.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zenith_hits_above_station() {
        let o = observer_ecef(69.0, 20.0, 0.0);
        let d = az_el_direction(69.0, 20.0, 0.0, 90.0);
        let p = intersect_emission_shell(o, d, 100.0).unwrap();
        let (llat, llon) = ecef_to_lat_lon(p);
        assert!((llat - 69.0).abs() < 1e-6);
        assert!((llon - 20.0).abs() < 1e-6)
    }
    #[test]
    fn magnetic_weight_rejects_obstruction() {
        let d = magnetic_zenith_direction(69.0, 20.0);
        assert!(magnetic_zenith_weight(d, 69.0, 20.0, 1.0, 0.0) > 0.99);
        assert_eq!(magnetic_zenith_weight(d, 69.0, 20.0, 1.0, 1.0), 0.0)
    }
}
