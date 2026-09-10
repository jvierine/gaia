//! Stellar photometry for cloud-thickness estimation.
//!
//! Cloud attenuates starlight, so the brightness of a known star measures the
//! optical depth along that line of sight. The chain is: catalogue position ->
//! precess to date -> azimuth/zenith at the station -> image pixel through the
//! camera's own AIDA/WISC lens model -> two-dimensional Gaussian fit -> total
//! flux above background. Comparing that flux with the same star's own clear-sky
//! reference gives the extinction, and hence the cloud thickness, at that point
//! in the field.
//!
//! The ephemeris and the forward projection are deliberate ports of the AIDA
//! tool (`js/aidatools.js` `starAzZe`/`precessJ2000ToDate` and `wisc_lens.py`
//! `az_el_to_pixel`) so that a star lands on the same pixel here as it does in
//! the calibration GUI. Both are pinned by tests against values generated from
//! those reference implementations.
use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::Path;

const DEG: f64 = std::f64::consts::PI / 180.0;
const BROWN_CONRADY_OPTMOD: i32 = 20;

/// One catalogue entry: ICRS/J2000 position and Tycho V_T magnitude.
#[derive(Debug, Clone, Copy)]
pub struct Star {
    pub ra_hours: f64,
    pub dec_deg: f64,
    pub vt_mag: f64,
}

/// Reads the AIDA `WISCAT1` catalogue and keeps stars brighter than
/// `max_magnitude_exclusive`. The payload is gzip with a 16-byte header: magic,
/// star count, and float stride.
pub fn load_catalog(path: &Path, max_magnitude_exclusive: f64) -> Result<Vec<Star>> {
    let compressed = std::fs::read(path)
        .with_context(|| format!("cannot read star catalogue {}", path.display()))?;
    let mut payload = Vec::new();
    flate2::read::GzDecoder::new(&compressed[..])
        .read_to_end(&mut payload)
        .context("star catalogue is not valid gzip")?;
    parse_catalog(&payload, max_magnitude_exclusive)
}

/// Parses an uncompressed `WISCAT1` payload. Split out so tests need no file.
pub fn parse_catalog(payload: &[u8], max_magnitude_exclusive: f64) -> Result<Vec<Star>> {
    if payload.len() < 16 || &payload[0..8] != b"WISCAT1\0" {
        bail!("star catalogue is missing the WISCAT1 magic");
    }
    let count = u32::from_le_bytes(payload[8..12].try_into().unwrap()) as usize;
    let stride = u32::from_le_bytes(payload[12..16].try_into().unwrap()) as usize;
    if stride < 3 {
        bail!("star catalogue stride {stride} is too small");
    }
    let need = 16 + count * stride * 4;
    if payload.len() < need {
        bail!("star catalogue is truncated: {} bytes, need {need}", payload.len());
    }
    let float_at = |offset: usize| -> f64 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap()) as f64
    };
    let mut stars = Vec::new();
    for index in 0..count {
        let base = 16 + index * stride * 4;
        let magnitude = float_at(base + 8);
        if magnitude < max_magnitude_exclusive {
            stars.push(Star {
                ra_hours: float_at(base),
                dec_deg: float_at(base + 4),
                vt_mag: magnitude,
            });
        }
    }
    Ok(stars)
}

fn julian_date(unix_seconds: f64) -> f64 {
    unix_seconds / 86400.0 + 2440587.5
}

/// Greenwich mean sidereal time in degrees, the series AIDA uses.
fn gmst_deg(unix_seconds: f64) -> f64 {
    let jd = julian_date(unix_seconds);
    let t = (jd - 2451545.0) / 36525.0;
    let value = 280.46061837 + 360.98564736629 * (jd - 2451545.0) + 0.000387933 * t * t
        - t * t * t / 38710000.0;
    value.rem_euclid(360.0)
}

/// Precess an ICRS/J2000 catalogue position to the equinox of date, returning
/// right ascension and declination in radians.
pub fn precess_j2000_to_date(ra_hours: f64, dec_deg: f64, unix_seconds: f64) -> (f64, f64) {
    let t = (julian_date(unix_seconds) - 2451545.0) / 36525.0;
    let arcsec = DEG / 3600.0;
    let zeta = (2306.2181 * t + 0.30188 * t * t + 0.017998 * t * t * t) * arcsec;
    let z = (2306.2181 * t + 1.09468 * t * t + 0.018203 * t * t * t) * arcsec;
    let theta = (2004.3109 * t - 0.42665 * t * t - 0.041833 * t * t * t) * arcsec;
    let ra = ra_hours / 12.0 * std::f64::consts::PI;
    let dec = dec_deg * DEG;
    let a = dec.cos() * (ra + zeta).sin();
    let b = theta.cos() * dec.cos() * (ra + zeta).cos() - theta.sin() * dec.sin();
    let c = theta.sin() * dec.cos() * (ra + zeta).cos() + theta.cos() * dec.sin();
    (
        (a.atan2(b) + z).rem_euclid(std::f64::consts::TAU),
        c.clamp(-1.0, 1.0).asin(),
    )
}

/// Azimuth and zenith angle in radians of a catalogue star seen from a station.
/// Azimuth is measured from north through east, matching AIDA.
pub fn star_az_ze(
    ra_hours: f64,
    dec_deg: f64,
    unix_seconds: f64,
    lat_deg: f64,
    lon_deg: f64,
) -> (f64, f64) {
    let sidereal = (gmst_deg(unix_seconds) + lon_deg) * DEG;
    let (ra, dec) = precess_j2000_to_date(ra_hours, dec_deg, unix_seconds);
    let lat = lat_deg * DEG;
    let altitude = ((sidereal - ra).cos() * dec.cos() * lat.cos() + dec.sin() * lat.sin())
        .clamp(-1.0, 1.0)
        .asin();
    let zenith = std::f64::consts::FRAC_PI_2 - altitude;
    let cos_alt = altitude.cos().max(1e-12);
    let sin_a = (sidereal - ra).sin() * dec.cos() / cos_alt;
    let cos_a =
        ((sidereal - ra).cos() * dec.cos() * lat.sin() - dec.sin() * lat.cos()) / cos_alt;
    let azimuth =
        (sin_a.atan2(cos_a) + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU);
    (azimuth, zenith)
}

/// Project azimuth/elevation in degrees to 0-based pixel coordinates of the
/// original image, using the camera's AIDA/WISC lens model. `optpar` carries the
/// model number in element zero, as stored in the calibration HDF5.
pub fn az_el_to_pixel(
    az_deg: f64,
    el_deg: f64,
    optpar: &[f64],
    width: f64,
    height: f64,
) -> Option<(f64, f64)> {
    if optpar.len() < 9 {
        return None;
    }
    let model = optpar[0] as i32;
    let p = &optpar[1..];
    // Local east/north/up direction, then into the camera frame.
    let az = az_deg * DEG;
    let ze = (90.0 - el_deg) * DEG;
    let sky = [ze.sin() * az.sin(), ze.sin() * az.cos(), ze.cos()];
    let rot = camera_rotation(p[2], p[3], p[4]);
    // camera_frame_vector multiplies the row vector by the rotation: s_j = sum_i sky_i R_ij.
    let s1 = sky[0] * rot[0][0] + sky[1] * rot[1][0] + sky[2] * rot[2][0];
    let s2 = sky[0] * rot[0][1] + sky[1] * rot[1][1] + sky[2] * rot[2][1];
    let s3 = sky[0] * rot[0][2] + sky[1] * rot[1][2] + sky[2] * rot[2][2];
    let radial = s1.hypot(s2);
    let (f1, f2, du, dv, alpha) = (p[0], p[1], p[5], p[6], p[7]);
    let (u_norm, v_norm) = if radial <= 1e-12 {
        (0.5 + du, 0.5 + dv)
    } else {
        let safe_s3 = if s3.abs() > 1e-12 {
            s3
        } else {
            1e-12_f64.copysign(if s3 != 0.0 { s3 } else { 1.0 })
        };
        let theta = radial.atan2(s3);
        match model {
            1 => (f1 * s1 / safe_s3 + 0.5 + du, f2 * s2 / safe_s3 + 0.5 + dv),
            2 => {
                let r = (alpha * theta).sin();
                (f1 * s1 / radial * r + 0.5 + du, f2 * s2 / radial * r + 0.5 + dv)
            }
            3 => {
                let positive_s3 = s3.max(1e-12);
                (
                    f1 * (1.0 - alpha) * s1 / positive_s3 + f1 * alpha * s1 / radial * theta + 0.5 + du,
                    f2 * (1.0 - alpha) * s2 / positive_s3 + f2 * alpha * s2 / radial * theta + 0.5 + dv,
                )
            }
            4 => {
                let r = theta.abs().powf(alpha);
                (f1 * s1 / radial * r + 0.5 + du, f2 * s2 / radial * r + 0.5 + dv)
            }
            5 => {
                let r = (alpha * theta).tan();
                (f1 * s1 / radial * r + 0.5 + du, f2 * s2 / radial * r + 0.5 + dv)
            }
            6 => {
                let r = (0.5 * theta).sin();
                (f1 * s1 / radial * r + 0.5 + du, f2 * s2 / radial * r + 0.5 + dv)
            }
            12 => {
                let r = if alpha > 0.0 {
                    (alpha * theta).tan() / alpha
                } else if alpha < 0.0 {
                    (alpha * theta).sin() / alpha
                } else {
                    theta.abs()
                };
                (f1 * s1 / radial * r + 0.5 + du, f2 * s2 / radial * r + 0.5 + dv)
            }
            BROWN_CONRADY_OPTMOD => {
                if p.len() < 12 {
                    return None;
                }
                let (xn, yn) = (s1 / safe_s3, s2 / safe_s3);
                let r2 = xn * xn + yn * yn;
                let (k1, k2, k3, p1, p2) = (p[7], p[8], p[9], p[10], p[11]);
                let radial_distortion = 1.0 + k1 * r2 + k2 * r2 * r2 + k3 * r2 * r2 * r2;
                let xd = xn * radial_distortion + 2.0 * p1 * xn * yn + p2 * (r2 + 2.0 * xn * xn);
                let yd = yn * radial_distortion + p1 * (r2 + 2.0 * yn * yn) + 2.0 * p2 * xn * yn;
                (f1 * xd + 0.5 + du, f2 * yd + 0.5 + dv)
            }
            _ => return None,
        }
    };
    let (x, y) = (u_norm * width - 1.0, v_norm * height - 1.0);
    if x.is_finite() && y.is_finite() {
        Some((x, y))
    } else {
        None
    }
}

fn mat3_mul(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut out = [[0.0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            out[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
        }
    }
    out
}

/// The WISC/AIDA camera rotation, Ry(alpha) . Rx(beta) . Rz(gamma), built as an
/// explicit product so it stays verifiably identical to `wisc_lens.camera_rotation`.
pub fn camera_rotation(alpha_deg: f64, beta_deg: f64, gamma_deg: f64) -> [[f64; 3]; 3] {
    let (a, b, g) = (alpha_deg * DEG, beta_deg * DEG, gamma_deg * DEG);
    let rot1 = [[g.cos(), -g.sin(), 0.0], [g.sin(), g.cos(), 0.0], [0.0, 0.0, 1.0]];
    let rot2 = [[a.cos(), 0.0, a.sin()], [0.0, 1.0, 0.0], [-a.sin(), 0.0, a.cos()]];
    let rot3 = [[1.0, 0.0, 0.0], [0.0, b.cos(), b.sin()], [0.0, -b.sin(), b.cos()]];
    mat3_mul(mat3_mul(rot2, rot3), rot1)
}

/// Result of a two-dimensional Gaussian fit to one star image.
#[derive(Debug, Clone, Copy)]
pub struct GaussianFit {
    pub background: f64,
    pub amplitude: f64,
    pub centre_x: f64,
    pub centre_y: f64,
    pub sigma_x: f64,
    pub sigma_y: f64,
    /// Total intensity above background, the analytic integral of the fit.
    pub flux: f64,
    pub rms_residual: f64,
}

/// Solve a small symmetric system by Gaussian elimination with partial pivoting.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for column in 0..n {
        let mut pivot = column;
        for row in column + 1..n {
            if a[row][column].abs() > a[pivot][column].abs() {
                pivot = row;
            }
        }
        if a[pivot][column].abs() < 1e-14 {
            return None;
        }
        a.swap(column, pivot);
        b.swap(column, pivot);
        for row in column + 1..n {
            let factor = a[row][column] / a[column][column];
            if factor == 0.0 {
                continue;
            }
            for k in column..n {
                a[row][k] -= factor * a[column][k];
            }
            b[row] -= factor * b[column];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut sum = b[row];
        for k in row + 1..n {
            sum -= a[row][k] * x[k];
        }
        x[row] = sum / a[row][row];
    }
    if x.iter().all(|v| v.is_finite()) { Some(x) } else { None }
}

/// Fit `background + amplitude * exp(-((x-cx)^2/2sx^2 + (y-cy)^2/2sy^2))` to a
/// patch by Levenberg-Marquardt with an analytic Jacobian. `patch` is row-major
/// `width * height`, and the returned centre is in patch coordinates.
pub fn fit_gaussian(patch: &[f64], width: usize, height: usize) -> Option<GaussianFit> {
    if width < 5 || height < 5 || patch.len() != width * height {
        return None;
    }
    // Background from the border ring, which a star should not reach.
    let mut border: Vec<f64> = Vec::new();
    for y in 0..height {
        for x in 0..width {
            if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
                border.push(patch[y * width + x]);
            }
        }
    }
    border.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let background0 = border[border.len() / 2];
    // Peak pixel as the starting centre.
    let mut peak = (0usize, 0usize, f64::NEG_INFINITY);
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let value = patch[y * width + x];
            if value > peak.2 {
                peak = (x, y, value);
            }
        }
    }
    let amplitude0 = (peak.2 - background0).max(1e-6);
    let mut theta = [
        background0,
        amplitude0,
        peak.0 as f64,
        peak.1 as f64,
        1.5,
        1.5,
    ];
    let residuals = |t: &[f64; 6]| -> f64 {
        let mut sum = 0.0;
        for y in 0..height {
            for x in 0..width {
                let model = gaussian_value(t, x as f64, y as f64);
                let d = patch[y * width + x] - model;
                sum += d * d;
            }
        }
        sum
    };
    let mut lambda = 1e-3;
    let mut cost = residuals(&theta);
    for _ in 0..80 {
        // Normal equations with the analytic Jacobian.
        let mut jtj = vec![vec![0.0; 6]; 6];
        let mut jtr = vec![0.0; 6];
        for y in 0..height {
            for x in 0..width {
                let (fx, fy) = (x as f64, y as f64);
                let (model, jac) = gaussian_value_and_jacobian(&theta, fx, fy);
                let residual = patch[y * width + x] - model;
                for i in 0..6 {
                    jtr[i] += jac[i] * residual;
                    for j in 0..6 {
                        jtj[i][j] += jac[i] * jac[j];
                    }
                }
            }
        }
        let mut improved = false;
        for _ in 0..12 {
            let mut damped = jtj.clone();
            for i in 0..6 {
                damped[i][i] *= 1.0 + lambda;
                if damped[i][i].abs() < 1e-14 {
                    damped[i][i] = lambda;
                }
            }
            let Some(step) = solve(damped, jtr.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let mut candidate = theta;
            for i in 0..6 {
                candidate[i] += step[i];
            }
            candidate[4] = candidate[4].abs().clamp(0.3, width as f64);
            candidate[5] = candidate[5].abs().clamp(0.3, height as f64);
            let candidate_cost = residuals(&candidate);
            if candidate_cost < cost {
                let converged = (cost - candidate_cost) < 1e-12 * cost.max(1.0);
                theta = candidate;
                cost = candidate_cost;
                lambda = (lambda * 0.3).max(1e-9);
                improved = true;
                if converged {
                    break;
                }
                break;
            }
            lambda *= 10.0;
        }
        if !improved {
            break;
        }
    }
    if theta[1] <= 0.0 || !theta.iter().all(|v| v.is_finite()) {
        return None;
    }
    let flux = std::f64::consts::TAU * theta[1] * theta[4] * theta[5];
    Some(GaussianFit {
        background: theta[0],
        amplitude: theta[1],
        centre_x: theta[2],
        centre_y: theta[3],
        sigma_x: theta[4],
        sigma_y: theta[5],
        flux,
        rms_residual: (cost / (width * height) as f64).sqrt(),
    })
}

fn gaussian_value(t: &[f64; 6], x: f64, y: f64) -> f64 {
    let dx = (x - t[2]) / t[4];
    let dy = (y - t[3]) / t[5];
    t[0] + t[1] * (-0.5 * (dx * dx + dy * dy)).exp()
}

fn gaussian_value_and_jacobian(t: &[f64; 6], x: f64, y: f64) -> (f64, [f64; 6]) {
    let dx = x - t[2];
    let dy = y - t[3];
    let (sx, sy) = (t[4], t[5]);
    let e = (-0.5 * ((dx / sx).powi(2) + (dy / sy).powi(2))).exp();
    let peak = t[1] * e;
    (
        t[0] + peak,
        [
            1.0,
            e,
            peak * dx / (sx * sx),
            peak * dy / (sy * sy),
            peak * dx * dx / (sx * sx * sx),
            peak * dy * dy / (sy * sy * sy),
        ],
    )
}

/// Percentile of an unsorted sample by linear interpolation, `fraction` in [0,1].
pub fn percentile(values: &[f64], fraction: f64) -> Option<f64> {
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let position = fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let low = position.floor() as usize;
    let high = position.ceil() as usize;
    let weight = position - low as f64;
    Some(sorted[low] * (1.0 - weight) + sorted[high] * weight)
}

/// Vertical cloud optical depth from a star's attenuation. `clear_flux` is that
/// star's own clear-sky reference, taken as a high percentile of its time
/// series. The slant path through a plane-parallel layer grows as sec(z), so the
/// vertical depth is the slant depth times cos(z). Negative values, which noise
/// can produce when a sample exceeds the reference, are clamped to zero.
pub fn optical_depth(flux: f64, clear_flux: f64, zenith_rad: f64) -> Option<f64> {
    if !(flux > 0.0) || !(clear_flux > 0.0) || !zenith_rad.is_finite() {
        return None;
    }
    let slant = -(flux / clear_flux).ln();
    Some((slant * zenith_rad.cos().max(0.0)).max(0.0))
}

/// Weight for a projected image given the cloud optical depth over it, so a
/// cloudy view yields to a clear one. Transmission is exp(-tau); the floor keeps
/// the weight strictly positive, which is what confines the effect to overlaps:
/// with a single contributor the per-pixel normalization divides it out again.
pub fn cloud_weight(optical_depth: f64, floor: f64) -> f64 {
    if !optical_depth.is_finite() {
        return floor;
    }
    floor + (1.0 - floor) * (-optical_depth.max(0.0)).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Kiruna all-sky calibration actually in the archive: optmod 2, 2832 px.
    const KIRUNA: [f64; 9] = [
        2.0,
        0.70682165202500724,
        0.7070094774383674,
        -1.5099938871193461,
        -1.288201852920821,
        -170.19172827706473,
        0.007550383264794,
        -0.0041860726586164052,
        0.47912975681992587,
    ];

    fn wiscat(records: &[(f32, f32, f32)]) -> Vec<u8> {
        let mut out = b"WISCAT1\0".to_vec();
        out.extend((records.len() as u32).to_le_bytes());
        out.extend(3u32.to_le_bytes());
        for (ra, dec, mag) in records {
            out.extend(ra.to_le_bytes());
            out.extend(dec.to_le_bytes());
            out.extend(mag.to_le_bytes());
        }
        out
    }

    #[test]
    fn catalogue_parsing_filters_by_magnitude_and_rejects_damage() {
        let payload = wiscat(&[
            (6.7524, -16.7161, -1.09),
            (18.6156, 38.7837, 0.03),
            (2.5302, 89.2641, 2.02),
            (1.0, 1.0, 3.99),
            (2.0, 2.0, 4.0),
            (3.0, 3.0, 7.5),
        ]);
        let bright = parse_catalog(&payload, 4.0).unwrap();
        assert_eq!(bright.len(), 4, "magnitude 4.0 itself is excluded");
        assert!(bright.iter().all(|s| s.vt_mag < 4.0));
        assert!((bright[0].ra_hours - 6.7524).abs() < 1e-4);
        assert_eq!(parse_catalog(&payload, 8.0).unwrap().len(), 6);
        assert_eq!(parse_catalog(&payload, -2.0).unwrap().len(), 0);
        // A wrong magic or a truncated payload must be refused, not misread.
        let mut bad = payload.clone();
        bad[3] = b'X';
        assert!(parse_catalog(&bad, 4.0).is_err());
        assert!(parse_catalog(&payload[..40], 4.0).is_err());
        assert!(parse_catalog(&[], 4.0).is_err());
    }

    #[test]
    fn ephemeris_matches_the_aida_reference() {
        // Values generated from js/aidatools.js radecToAzZe at Kiruna.
        let (lat, lon) = (67.84, 20.41);
        let polaris = (2.5301944, 89.2641111);
        let sirius = (6.7524, -16.7161);
        let vega = (18.6156, 38.7837);
        let arcturus = (14.2610, 19.1824);
        let cases: [(f64, (f64, f64), f64, f64); 12] = [
            (1789077600.0, polaris, 1.536036616, 21.916279422),
            (1789077600.0, sirius, 66.792340581, 117.006694660),
            (1789077600.0, vega, 260.446078238, 43.666660135),
            (1789077600.0, arcturus, 309.943451852, 84.708662983),
            (1768447800.0, polaris, 358.974229968, 22.651778033),
            (1768447800.0, sirius, 259.820399137, 103.959410688),
            (1768447800.0, vega, 71.485422508, 55.217340143),
            (1768447800.0, arcturus, 147.185888315, 51.614519311),
            (1782000000.0, polaris, 1.461768906, 22.451282779),
            (1782000000.0, sirius, 9.867646180, 128.644720744),
            (1782000000.0, vega, 196.172898923, 29.571995080),
            (1782000000.0, arcturus, 263.908585877, 66.898435276),
        ];
        for (unix, (ra, dec), az_deg, ze_deg) in cases {
            let (az, ze) = star_az_ze(ra, dec, unix, lat, lon);
            assert!(
                (az / DEG - az_deg).abs() < 1e-6,
                "azimuth {} vs reference {az_deg}",
                az / DEG
            );
            assert!(
                (ze / DEG - ze_deg).abs() < 1e-6,
                "zenith {} vs reference {ze_deg}",
                ze / DEG
            );
        }
        // Precession alone, also from the reference implementation.
        let (ra, dec) = precess_j2000_to_date(6.7524, -16.7161, 1789077600.0);
        assert!((ra - 1.772979399528).abs() < 1e-9);
        assert!((dec - (-0.292265185426)).abs() < 1e-9);
    }

    #[test]
    fn polaris_sits_at_the_observer_latitude_all_day() {
        // A physical invariant, independent of the reference: Polaris is 0.74 deg
        // from the pole, so its altitude tracks the latitude within that.
        for lat in [67.84, 78.15, 45.0, -33.0] {
            for hour in 0..24 {
                let unix = 1789077600.0 + hour as f64 * 3600.0;
                let (az, ze) = star_az_ze(2.5301944, 89.2641111, unix, lat, 20.41);
                let altitude = 90.0 - ze / DEG;
                assert!(
                    (altitude - lat).abs() < 0.8,
                    "Polaris altitude {altitude} should track latitude {lat}"
                );
                if lat > 0.0 {
                    // Polaris swings about north by asin(sin p / cos lat), where p
                    // is its 0.74 deg polar distance, so the bound widens with
                    // latitude rather than being a fixed few degrees.
                    let swing = ((0.74 * DEG).sin() / lat.to_radians().cos().abs())
                        .min(1.0)
                        .asin()
                        / DEG;
                    let from_north = (az / DEG + 180.0).rem_euclid(360.0) - 180.0;
                    assert!(
                        from_north.abs() < swing + 0.25,
                        "Polaris azimuth {} exceeds its {swing} deg swing at latitude {lat}",
                        az / DEG
                    );
                }
            }
        }
    }

    #[test]
    fn forward_projection_matches_wisc_lens() {
        // Values generated from wisc_lens.az_el_to_pixel with the Kiruna optpar.
        let (w, h) = (2832.0, 2832.0);
        let cases: [(f64, f64, f64, f64); 10] = [
            (0.0, 90.0, 1407.8028100870, 1386.2041888481),
            (0.0, 60.0, 1325.7496092668, 896.6362275490),
            (90.0, 60.0, 919.3471345692, 1473.1436364140),
            (180.0, 60.0, 1495.1957679594, 1876.2174990699),
            (270.0, 60.0, 1898.5723394228, 1303.3561288375),
            (45.0, 30.0, 624.9589532380, 832.9533157451),
            (135.0, 15.0, 749.5383817343, 2359.1843504918),
            (300.0, 5.0, 2430.4099024481, 568.8388440071),
            (210.0, 75.0, 1568.5301013892, 1578.5876778081),
            (0.0, 0.0, 1200.6270154438, 39.0767373671),
        ];
        for (az, el, x, y) in cases {
            let (gx, gy) = az_el_to_pixel(az, el, &KIRUNA, w, h).expect("projection");
            assert!((gx - x).abs() < 1e-6, "x {gx} vs reference {x} at az {az} el {el}");
            assert!((gy - y).abs() < 1e-6, "y {gy} vs reference {y} at az {az} el {el}");
        }
        // The rotation is orthonormal, as any rotation must be.
        let r = camera_rotation(-1.51, -1.29, -170.19);
        for i in 0..3 {
            let norm: f64 = (0..3).map(|k| r[i][k] * r[i][k]).sum();
            assert!((norm - 1.0).abs() < 1e-12);
            for j in i + 1..3 {
                let dot: f64 = (0..3).map(|k| r[i][k] * r[j][k]).sum();
                assert!(dot.abs() < 1e-12);
            }
        }
        // Too short an optpar and an unknown model are refused rather than guessed.
        assert!(az_el_to_pixel(0.0, 45.0, &KIRUNA[..5], w, h).is_none());
        let mut unknown = KIRUNA;
        unknown[0] = 99.0;
        assert!(az_el_to_pixel(0.0, 45.0, &unknown, w, h).is_none());
    }

    /// Render a noiseless star patch for fitting tests.
    fn synth(w: usize, h: usize, t: [f64; 6]) -> Vec<f64> {
        (0..w * h)
            .map(|i| gaussian_value(&t, (i % w) as f64, (i / w) as f64))
            .collect()
    }

    #[test]
    fn gaussian_fit_recovers_a_known_star() {
        let truth = [12.0, 240.0, 8.3, 7.6, 1.8, 2.1];
        let patch = synth(17, 17, truth);
        let fit = fit_gaussian(&patch, 17, 17).expect("fit");
        assert!((fit.background - truth[0]).abs() < 0.05, "background {}", fit.background);
        assert!((fit.amplitude - truth[1]).abs() < 0.5, "amplitude {}", fit.amplitude);
        assert!((fit.centre_x - truth[2]).abs() < 0.01, "cx {}", fit.centre_x);
        assert!((fit.centre_y - truth[3]).abs() < 0.01, "cy {}", fit.centre_y);
        assert!((fit.sigma_x - truth[4]).abs() < 0.02, "sx {}", fit.sigma_x);
        assert!((fit.sigma_y - truth[5]).abs() < 0.02, "sy {}", fit.sigma_y);
        // Total intensity above background is the analytic integral.
        let expected = std::f64::consts::TAU * truth[1] * truth[4] * truth[5];
        assert!(
            (fit.flux - expected).abs() / expected < 0.02,
            "flux {} vs analytic {expected}",
            fit.flux
        );
        assert!(fit.rms_residual < 1.0);
    }

    #[test]
    fn gaussian_fit_is_stable_against_noise_and_refuses_junk() {
        // Deterministic pseudo-noise so the test cannot flake.
        let truth = [30.0, 180.0, 9.0, 9.0, 2.0, 2.0];
        let mut patch = synth(19, 19, truth);
        let mut seed = 12345u64;
        for value in patch.iter_mut() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let unit = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 0.5;
            *value += unit * 6.0;
        }
        let fit = fit_gaussian(&patch, 19, 19).expect("fit under noise");
        let expected = std::f64::consts::TAU * truth[1] * truth[4] * truth[5];
        assert!(
            (fit.flux - expected).abs() / expected < 0.06,
            "noisy flux {} vs {expected}",
            fit.flux
        );
        assert!((fit.centre_x - 9.0).abs() < 0.15 && (fit.centre_y - 9.0).abs() < 0.15);
        // A flat patch has no star: the fit must not invent a positive amplitude.
        let flat = vec![25.0; 15 * 15];
        assert!(fit_gaussian(&flat, 15, 15).is_none_or(|f| f.flux < 1.0));
        // Malformed input is refused.
        assert!(fit_gaussian(&[1.0, 2.0], 2, 1).is_none());
        assert!(fit_gaussian(&patch, 4, 4).is_none());
    }

    #[test]
    fn gaussian_fit_locates_an_offset_star_so_the_caller_can_reject_it() {
        // The 3-pixel acceptance is a caller policy; the fit must report the true
        // centre so that policy can be applied.
        let truth = [10.0, 200.0, 13.5, 4.5, 1.6, 1.6];
        let patch = synth(19, 19, truth);
        let fit = fit_gaussian(&patch, 19, 19).expect("fit");
        let offset = ((fit.centre_x - 9.0).powi(2) + (fit.centre_y - 9.0).powi(2)).sqrt();
        assert!(offset > 3.0, "centroid offset {offset} should exceed the 3 px limit");
        assert!((fit.centre_x - truth[2]).abs() < 0.05);
        assert!((fit.centre_y - truth[3]).abs() < 0.05);
    }

    #[test]
    fn percentile_interpolates_and_survives_gaps() {
        let sample = [4.0, 1.0, 3.0, 2.0, 5.0];
        assert_eq!(percentile(&sample, 0.0).unwrap(), 1.0);
        assert_eq!(percentile(&sample, 1.0).unwrap(), 5.0);
        assert_eq!(percentile(&sample, 0.5).unwrap(), 3.0);
        assert!((percentile(&sample, 0.9).unwrap() - 4.6).abs() < 1e-12);
        assert!(percentile(&[], 0.9).is_none());
        assert_eq!(percentile(&[7.0, f64::NAN], 0.9).unwrap(), 7.0);
    }

    #[test]
    fn optical_depth_measures_extinction_and_scales_with_airmass() {
        // A star at its own reference brightness sees no cloud.
        assert_eq!(optical_depth(100.0, 100.0, 0.0).unwrap(), 0.0);
        // One e-folding of dimming at the zenith is unit optical depth.
        let tau = optical_depth(100.0 / std::f64::consts::E, 100.0, 0.0).unwrap();
        assert!((tau - 1.0).abs() < 1e-12, "tau {tau}");
        // The same dimming seen through a longer slant path implies a thinner
        // vertical layer, by cos(z).
        let sixty = optical_depth(100.0 / std::f64::consts::E, 100.0, 60.0 * DEG).unwrap();
        assert!((sixty - 0.5).abs() < 1e-9, "tau at 60 deg {sixty}");
        // Brighter than the reference is noise, not negative cloud.
        assert_eq!(optical_depth(150.0, 100.0, 0.0).unwrap(), 0.0);
        assert!(optical_depth(0.0, 100.0, 0.0).is_none());
        assert!(optical_depth(100.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn cloud_weight_falls_with_thickness_but_stays_positive() {
        let floor = 0.05;
        assert!((cloud_weight(0.0, floor) - 1.0).abs() < 1e-12, "clear sky keeps full weight");
        let mut previous = 1.0;
        for step in 0..=60 {
            let tau = step as f64 * 0.2;
            let w = cloud_weight(tau, floor);
            assert!(w <= previous + 1e-15, "weight must not rise with thickness");
            assert!(w >= floor && w <= 1.0);
            previous = w;
        }
        // Thick cloud approaches the floor, never zero: a single contributor is
        // normalized back to full strength, so no hole is punched.
        assert!(cloud_weight(50.0, floor) > 0.0);
        assert!((cloud_weight(50.0, floor) - floor).abs() < 1e-9);
        assert!((cloud_weight(1.0, floor) - (floor + (1.0 - floor) / std::f64::consts::E)).abs() < 1e-12);
    }
}
