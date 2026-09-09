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

/// Smallest angle between two unoriented axes. This is invariant to B -> -B.
pub fn axial_angle(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (a, b) = (norm(a), norm(b));
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let cross_norm = dot(cross, cross).sqrt();
    let theta = cross_norm.atan2(dot(a, b).clamp(-1.0, 1.0));
    if theta > PI / 2.0 { PI - theta } else { theta }
}

/// Laplacian magnetic-axis preference exp(-|theta_B|/S).
pub fn magnetic_axis_weight(los: [f64; 3], field: [f64; 3], falloff_rad: f64) -> f64 {
    if !falloff_rad.is_finite() || falloff_rad <= 0.0 {
        return 0.0;
    }
    (-axial_angle(los, field).abs() / falloff_rad).exp()
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
    fn axial_angle_uses_nearest_field_direction() {
        let u = [1.0, 0.0, 0.0];
        assert!(axial_angle(u, [-1.0, 0.0, 0.0]) < 1e-12);
        assert!((axial_angle(u, [0.0, 1.0, 0.0]) - PI / 2.0).abs() < 1e-12);
    }
    #[test]
    fn laplacian_weight_has_requested_scale() {
        let s = 20f64.to_radians();
        let u = [1.0, 0.0, 0.0];
        let b = [s.cos(), s.sin(), 0.0];
        assert!((magnetic_axis_weight(u, b, s) - (-1.0f64).exp()).abs() < 1e-12);
        assert!(
            (magnetic_axis_weight(u, [-b[0], -b[1], -b[2]], s) - (-1.0f64).exp()).abs() < 1e-12
        );
    }
}
