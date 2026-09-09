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

/// psi(x) = exp(-1/|x|) for x > 0 and 0 otherwise. The building block of a
/// C-infinity transition; the absolute value is redundant on the x > 0 branch.
fn psi(x: f64) -> f64 {
    if x > 0.0 { (-1.0 / x.abs()).exp() } else { 0.0 }
}

/// Smooth step f_s(t) = psi(t) / (psi(t) + psi(1 - t)): exactly 0 for t <= 0,
/// exactly 1 for t >= 1, infinitely differentiable in between, 1/2 at t = 1/2.
pub fn smooth_step(t: f64) -> f64 {
    let rise = psi(t);
    if rise == 0.0 {
        return 0.0;
    }
    rise / (rise + psi(1.0 - t))
}

/// Tapers a contribution off at large zenith angle: 1 out to `start_rad`, then
/// smoothly to 0 at `start_rad + width_rad`. This is 1 - f_s(z_a), so the
/// horizon-grazing edge of an image is removed rather than isolated.
pub fn zenith_taper(zenith_rad: f64, start_rad: f64, width_rad: f64) -> f64 {
    if !zenith_rad.is_finite() {
        return 0.0;
    }
    if !width_rad.is_finite() || width_rad <= 0.0 {
        return if zenith_rad >= start_rad { 0.0 } else { 1.0 };
    }
    1.0 - smooth_step((zenith_rad - start_rad) / width_rad)
}

/// Zenith angle of a look direction at the observer: 0 straight up, PI/2 at the
/// horizon. The Earth model is spherical, so the local vertical at the observer
/// is its own radius vector.
pub fn zenith_angle(observer: [f64; 3], look: [f64; 3]) -> f64 {
    dot(norm(observer), norm(look)).clamp(-1.0, 1.0).acos()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zenith_angle_is_zero_overhead_and_right_angle_at_the_horizon() {
        let o = observer_ecef(69.0, 20.0, 0.0);
        let up = az_el_direction(69.0, 20.0, 0.0, 90.0);
        assert!(zenith_angle(o, up).to_degrees().abs() < 1e-9);
        for az in [0.0, 90.0, 187.0, 305.0] {
            let flat = az_el_direction(69.0, 20.0, az, 0.0);
            assert!((zenith_angle(o, flat).to_degrees() - 90.0).abs() < 1e-9);
        }
        let half = az_el_direction(69.0, 20.0, 45.0, 60.0);
        assert!((zenith_angle(o, half).to_degrees() - 30.0).abs() < 1e-9);
    }

    #[test]
    fn zenith_taper_holds_then_falls_smoothly_to_zero() {
        let (start, width) = (75f64.to_radians(), 10f64.to_radians());
        let taper = |deg: f64| zenith_taper(deg.to_radians(), start, width);
        // Flat and exactly one out to the knee, so nothing below 75 deg is altered.
        for deg in [0.0, 30.0, 60.0, 74.9, 75.0] {
            assert_eq!(taper(deg), 1.0, "{deg} deg must be untouched");
        }
        // Exactly zero from the far knee onwards, so the horizon is dropped.
        for deg in [85.0, 85.1, 89.0, 90.0] {
            assert_eq!(taper(deg), 0.0, "{deg} deg must be removed");
        }
        // Half weight at the midpoint, and strictly decreasing across the ramp.
        assert!((taper(80.0) - 0.5).abs() < 1e-12);
        let mut previous = 1.0;
        for step in 0..=100 {
            let value = taper(75.0 + step as f64 * 0.1);
            assert!(value <= previous + 1e-15, "taper must not increase");
            assert!((0.0..=1.0).contains(&value));
            previous = value;
        }
    }

    #[test]
    fn zenith_taper_is_one_minus_the_specified_smooth_step() {
        // psi and f_s exactly as specified, recomputed independently here.
        let psi_ref = |x: f64| if x > 0.0 { (-1.0 / x.abs()).exp() } else { 0.0 };
        let f_s = |z_deg: f64| {
            let t = (z_deg - 75.0) / 10.0;
            let (a, b) = (psi_ref(t), psi_ref(1.0 - t));
            if a + b == 0.0 { 0.0 } else { a / (a + b) }
        };
        let (start, width) = (75f64.to_radians(), 10f64.to_radians());
        for step in 0..=180 {
            let deg = step as f64 * 0.5;
            let got = zenith_taper(deg.to_radians(), start, width);
            assert!((got - (1.0 - f_s(deg))).abs() < 1e-12, "mismatch at {deg} deg");
        }
    }

    #[test]
    fn magnetic_weight_is_tapered_only_near_the_horizon() {
        // A camera at 69N: straight up keeps its full magnetic weight, a
        // horizon-grazing ray loses all of it.
        let o = observer_ecef(69.0, 20.0, 0.0);
        let (start, width) = (75f64.to_radians(), 10f64.to_radians());
        let field = az_el_direction(69.0, 20.0, 0.0, 78.0);
        for (el, expect_kept) in [(90.0, true), (60.0, true), (16.0, true), (4.0, false), (0.0, false)] {
            let look = az_el_direction(69.0, 20.0, 0.0, el);
            let base = magnetic_axis_weight(look, field, 20f64.to_radians());
            let tapered = base * zenith_taper(zenith_angle(o, look), start, width);
            assert!(base > 0.0);
            assert_eq!(tapered > 0.0, expect_kept, "elevation {el} deg");
        }
    }

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
