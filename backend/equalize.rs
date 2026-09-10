//! Cross-camera intensity equalization along equidistant great-circle cuts.
//!
//! Two cameras that both see a shell point can only be compared fairly where
//! neither is favoured by geometry. The locus of points equidistant from two
//! stations is exactly a great circle: for unit station directions a and b, every
//! p with p.a == p.b satisfies p.(a-b) == 0, so the locus is the great circle whose
//! pole is (a-b) normalised, and it passes through the midpoint (a+b) normalised.
//! Equal angular distance means equal slant range and equal zenith angle, so an
//! intensity ratio measured along this cut carries no view-angle or airmass term
//! and is a pure gain ratio.
//!
//! Because the images are projected to a single altitude, field-aligned structure
//! is already largely discarded, so for separated stations there is nothing to
//! gain from orienting the cut on the magnetic meridian rather than on the
//! baseline. Co-located instruments are the exception: there the equidistance
//! condition is satisfied everywhere, every direction is admissible, and the cut
//! is taken along the magnetic meridian plane whose horizontal trace is magnetic
//! north, that is azimuth equal to the local declination.
use crate::geometry::{EARTH_RADIUS_KM, EMISSION_ALTITUDE_KM, az_el_direction, ecef_to_lat_lon};

fn norm(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Midpoint direction and in-plane tangent of the equidistant cut for two
/// stations. Returns None only if the geometry is unusable (antipodal stations).
/// Coincident stations make every point equidistant, so the cut direction is
/// immaterial; the magnetic meridian plane is used, given by
/// `magnetic_declination_deg` at the shared station.
pub fn equidistant_frame(
    a: [f64; 3],
    b: [f64; 3],
    magnetic_declination_deg: f64,
) -> Option<([f64; 3], [f64; 3])> {
    let (ah, bh) = (norm(a), norm(b));
    let difference = [ah[0] - bh[0], ah[1] - bh[1], ah[2] - bh[2]];
    let separation = dot(difference, difference).sqrt();
    let sum = [ah[0] + bh[0], ah[1] + bh[1], ah[2] + bh[2]];
    if dot(sum, sum).sqrt() < 1e-9 {
        return None; // antipodal: no meaningful shared view
    }
    let midpoint = norm(sum);
    if separation < 1e-12 {
        // Coincident stations: cut along the magnetic meridian plane, whose
        // horizontal trace at the station is magnetic north.
        let (lat, lon) = ecef_to_lat_lon(midpoint);
        let tangent = az_el_direction(lat, lon, magnetic_declination_deg, 0.0);
        let tangent = if tangent.iter().all(|v| v.is_finite())
            && dot(tangent, tangent).sqrt() > 1e-9
        {
            norm(tangent)
        } else {
            norm(cross(midpoint, [1.0, 0.0, 0.0]))
        };
        return Some((midpoint, tangent));
    }
    let pole = norm(difference);
    Some((midpoint, norm(cross(pole, midpoint))))
}

/// Shell points along the equidistant cut, centred on the midpoint and spanning
/// +/- `half_width_rad` of arc, in ECEF kilometres at the emission altitude.
pub fn equidistant_cut(
    a: [f64; 3],
    b: [f64; 3],
    magnetic_declination_deg: f64,
    half_width_rad: f64,
    samples: usize,
) -> Option<Vec<[f64; 3]>> {
    let (midpoint, tangent) = equidistant_frame(a, b, magnetic_declination_deg)?;
    if samples == 0 {
        return Some(Vec::new());
    }
    let radius = EARTH_RADIUS_KM + EMISSION_ALTITUDE_KM;
    Some(
        (0..samples)
            .map(|i| {
                let s = if samples == 1 {
                    0.0
                } else {
                    -half_width_rad + 2.0 * half_width_rad * i as f64 / (samples - 1) as f64
                };
                let (c, sn) = (s.cos(), s.sin());
                [
                    radius * (midpoint[0] * c + tangent[0] * sn),
                    radius * (midpoint[1] * c + tangent[1] * sn),
                    radius * (midpoint[2] * c + tangent[2] * sn),
                ]
            })
            .collect(),
    )
}

/// Equirectangular position of a shell vertex, in the same frame publish.rs
/// rasterises in. The stored vertex is the shell hit scaled by 1/6371 with axes
/// permuted, so the ECEF component order is (v2, v0, v1).
fn vertex_lon_lat(v: &[f32]) -> [f64; 2] {
    let (x, y, z) = (v[2] as f64, v[0] as f64, v[1] as f64);
    let r = (x * x + y * y + z * z).sqrt();
    [y.atan2(x), (z / r).clamp(-1.0, 1.0).asin()]
}

/// Texture coordinates of each query point in a camera's projection mesh, or None
/// where that camera does not cover the point. One pass over the triangles serves
/// every point, and the result is valid for every frame of that camera because the
/// mesh depends on the calibration and mask, never on time.
pub fn sample_uv(geometry: &[f32], points: &[[f64; 3]]) -> Vec<Option<[f32; 2]>> {
    let targets: Vec<[f64; 2]> = points
        .iter()
        .map(|p| {
            let r = dot(*p, *p).sqrt();
            [p[1].atan2(p[0]), (p[2] / r).clamp(-1.0, 1.0).asin()]
        })
        .collect();
    let mut out = vec![None; points.len()];
    for triangle in geometry.chunks_exact(15) {
        let mut corner = [[0f64; 2]; 3];
        for k in 0..3 {
            corner[k] = vertex_lon_lat(&triangle[k * 5..]);
        }
        // Unwrap longitude against the first corner so a triangle straddling the
        // antimeridian is still convex in this frame.
        for k in 1..3 {
            while corner[k][0] - corner[0][0] > std::f64::consts::PI {
                corner[k][0] -= std::f64::consts::TAU;
            }
            while corner[k][0] - corner[0][0] < -std::f64::consts::PI {
                corner[k][0] += std::f64::consts::TAU;
            }
        }
        let den = (corner[1][1] - corner[2][1]) * (corner[0][0] - corner[2][0])
            + (corner[2][0] - corner[1][0]) * (corner[0][1] - corner[2][1]);
        if den.abs() < 1e-15 {
            continue;
        }
        let (lo, hi) = (
            corner.iter().map(|c| c[1]).fold(f64::INFINITY, f64::min),
            corner.iter().map(|c| c[1]).fold(f64::NEG_INFINITY, f64::max),
        );
        for (index, target) in targets.iter().enumerate() {
            // The mesh stores f32 vertices, so pad the latitude prefilter by well
            // under the mesh resolution rather than comparing exactly.
            const SLACK: f64 = 1e-6;
            if out[index].is_some() || target[1] < lo - SLACK || target[1] > hi + SLACK {
                continue;
            }
            // Shift the query into the same longitude branch as the triangle.
            let mut lon = target[0];
            while lon - corner[0][0] > std::f64::consts::PI {
                lon -= std::f64::consts::TAU;
            }
            while lon - corner[0][0] < -std::f64::consts::PI {
                lon += std::f64::consts::TAU;
            }
            let a = ((corner[1][1] - corner[2][1]) * (lon - corner[2][0])
                + (corner[2][0] - corner[1][0]) * (target[1] - corner[2][1]))
                / den;
            let b = ((corner[2][1] - corner[0][1]) * (lon - corner[2][0])
                + (corner[0][0] - corner[2][0]) * (target[1] - corner[2][1]))
                / den;
            let c = 1.0 - a - b;
            // Rounding can put a query that lies exactly on an edge or vertex a
            // hair outside every adjacent triangle, which would wrongly report the
            // point as uncovered, so admit a tolerance and clamp before use.
            const EDGE: f64 = -1e-4;
            if a < EDGE || b < EDGE || c < EDGE {
                continue;
            }
            let q = [a.clamp(0.0, 1.0), b.clamp(0.0, 1.0), c.clamp(0.0, 1.0)];
            let total = q[0] + q[1] + q[2];
            let q = if total > 0.0 {
                [q[0] / total, q[1] / total, q[2] / total]
            } else {
                continue;
            };
            let uv = |axis: usize| {
                (0..3)
                    .map(|k| q[k] * triangle[k * 5 + axis] as f64)
                    .sum::<f64>() as f32
            };
            out[index] = Some([uv(3), uv(4)]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::observer_ecef;

    fn angle(p: [f64; 3], q: [f64; 3]) -> f64 {
        dot(norm(p), norm(q)).clamp(-1.0, 1.0).acos()
    }

    #[test]
    fn every_sample_is_equidistant_from_both_stations() {
        // Real separated pairs, including a long east-west and a north-south baseline.
        for (la, lo, lb, lob) in [
            (69.65, 18.96, 67.84, 20.41),   // Tromso - Kiruna
            (67.37, 26.63, 78.15, 15.54),   // Sodankyla - Longyearbyen
            (54.71, 20.51, 54.47, 113.60),  // very long baseline
            (-45.0, 170.0, -40.0, -175.0),  // southern, across the dateline
        ] {
            let a = observer_ecef(la, lo, 0.0);
            let b = observer_ecef(lb, lob, 0.0);
            let cut = equidistant_cut(a, b, 0.0, 3f64.to_radians(), 33).unwrap();
            assert_eq!(cut.len(), 33);
            for p in &cut {
                let (da, db) = (angle(*p, a), angle(*p, b));
                assert!(
                    (da - db).abs() < 1e-12,
                    "sample not equidistant: {da} vs {db}"
                );
            }
            // The centre sample is the great-circle midpoint of the two stations.
            let centre = cut[16];
            assert!((angle(centre, a) - angle(centre, b)).abs() < 1e-12);
            let half = angle(a, b) / 2.0;
            assert!((angle(centre, a) - half).abs() < 1e-9, "centre is not the midpoint");
        }
    }

    #[test]
    fn the_cut_is_perpendicular_to_the_baseline() {
        let a = observer_ecef(69.65, 18.96, 0.0);
        let b = observer_ecef(67.84, 20.41, 0.0);
        let (midpoint, tangent) = equidistant_frame(a, b, 0.0).unwrap();
        let pole = norm([
            norm(a)[0] - norm(b)[0],
            norm(a)[1] - norm(b)[1],
            norm(a)[2] - norm(b)[2],
        ]);
        // The cut plane contains the midpoint and the tangent and excludes the
        // baseline direction, which is the plane's pole.
        assert!(dot(pole, midpoint).abs() < 1e-12);
        assert!(dot(pole, tangent).abs() < 1e-12);
        assert!((dot(tangent, tangent) - 1.0).abs() < 1e-12);
        assert!(dot(midpoint, tangent).abs() < 1e-12);
    }

    #[test]
    fn coincident_stations_cut_along_the_magnetic_meridian() {
        // Co-located instruments, e.g. the Gillam cluster: every point is
        // equidistant, so the cut is oriented on the magnetic meridian plane.
        let a = observer_ecef(56.38, -94.64, 0.0);
        let cut = equidistant_cut(a, a, 0.0, 2f64.to_radians(), 9).unwrap();
        assert_eq!(cut.len(), 9);
        assert!(cut.iter().all(|p| p.iter().all(|v| v.is_finite())));
        // With zero declination the magnetic meridian is the geographic one.
        let (midpoint, tangent) = equidistant_frame(a, a, 0.0).unwrap();
        let east = norm(cross([0.0, 0.0, 1.0], midpoint));
        assert!(dot(tangent, east).abs() < 1e-12, "zero declination is north-south");
        assert!(tangent[2] > 0.0, "tangent should point north");
        assert!(dot(midpoint, tangent).abs() < 1e-12, "tangent must be horizontal");
        // A real declination rotates the cut by exactly that angle in the
        // horizontal plane, which is what following the field's meridian means.
        for declination in [-30.0, -11.5, 0.0, 7.25, 40.0] {
            let (m, t) = equidistant_frame(a, a, declination).unwrap();
            assert!(dot(m, t).abs() < 1e-12, "tangent must stay horizontal");
            assert!((dot(t, t) - 1.0).abs() < 1e-12);
            let north = norm([
                -m[0] * m[2] / (1.0 - m[2] * m[2]).sqrt(),
                -m[1] * m[2] / (1.0 - m[2] * m[2]).sqrt(),
                (1.0 - m[2] * m[2]).sqrt(),
            ]);
            let bearing = dot(t, norm(cross([0.0, 0.0, 1.0], m)))
                .atan2(dot(t, north))
                .to_degrees();
            assert!(
                (bearing - declination).abs() < 1e-9,
                "cut bearing {bearing} should equal the declination {declination}"
            );
        }
        // Directly over the pole the horizontal frame degenerates; stay finite.
        let polar = equidistant_frame(observer_ecef(90.0, 0.0, 0.0), observer_ecef(90.0, 0.0, 0.0), 5.0);
        assert!(polar.is_some_and(|(_, t)| t.iter().all(|v| v.is_finite())));
    }

    /// A single triangle covering a known patch, built the way projection.rs
    /// stores vertices: shell hit divided by 6371 with axes permuted to (y,z,x).
    fn one_triangle(corners: [[f64; 3]; 3], uv: [[f32; 2]; 3]) -> Vec<f32> {
        let mut g = Vec::new();
        for k in 0..3 {
            let c = corners[k];
            g.extend([
                (c[1] / EARTH_RADIUS_KM) as f32,
                (c[2] / EARTH_RADIUS_KM) as f32,
                (c[0] / EARTH_RADIUS_KM) as f32,
                uv[k][0],
                uv[k][1],
            ]);
        }
        g
    }

    #[test]
    fn uv_sampling_finds_points_inside_the_mesh_and_rejects_those_outside() {
        let shell = EARTH_RADIUS_KM + EMISSION_ALTITUDE_KM;
        let at = |lat: f64, lon: f64| {
            let (la, lo) = (lat.to_radians(), lon.to_radians());
            [shell * la.cos() * lo.cos(), shell * la.cos() * lo.sin(), shell * la.sin()]
        };
        // Triangle spanning 60-64N, 18-24E, with uv pinned at its corners.
        let g = one_triangle(
            [at(60.0, 18.0), at(60.0, 24.0), at(64.0, 18.0)],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
        );
        let inside = at(61.0, 19.5);
        let outside = [at(50.0, 19.0), at(61.0, 40.0), at(70.0, 19.0)];
        let mut probes = vec![inside];
        probes.extend(outside);
        let got = sample_uv(&g, &probes);
        let uv = got[0].expect("a point inside the triangle must be located");
        assert!(uv[0] >= 0.0 && uv[0] <= 1.0 && uv[1] >= 0.0 && uv[1] <= 1.0, "uv {uv:?} off the triangle");
        // Barycentric interpolation must reproduce the corner values exactly.
        for (corner, expect) in [(at(60.0, 18.0), [0.0f32, 0.0]), (at(60.0, 24.0), [1.0, 0.0]), (at(64.0, 18.0), [0.0, 1.0])] {
            let c = sample_uv(&g, &[corner])[0].expect("corner is inside its own triangle");
            assert!((c[0] - expect[0]).abs() < 1e-5 && (c[1] - expect[1]).abs() < 1e-5, "corner uv {c:?} != {expect:?}");
        }
        for (i, _) in outside.iter().enumerate() {
            assert!(got[i + 1].is_none(), "point {i} outside the mesh must not be located");
        }
        // An empty mesh covers nothing rather than panicking.
        assert!(sample_uv(&[], &probes).iter().all(Option::is_none));
    }

    #[test]
    fn samples_sit_on_the_emission_shell() {
        let a = observer_ecef(69.65, 18.96, 0.0);
        let b = observer_ecef(67.84, 20.41, 0.0);
        let shell = EARTH_RADIUS_KM + EMISSION_ALTITUDE_KM;
        for p in equidistant_cut(a, b, 0.0, 4f64.to_radians(), 17).unwrap() {
            assert!((dot(p, p).sqrt() - shell).abs() < 1e-9);
        }
        assert!(equalize_antipodal_is_rejected());
    }

    fn equalize_antipodal_is_rejected() -> bool {
        let a = observer_ecef(10.0, 20.0, 0.0);
        let b = observer_ecef(-10.0, -160.0, 0.0);
        equidistant_frame(a, b, 0.0).is_none()
    }
}
