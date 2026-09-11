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
/// The lunar series expresses its semi-major axis in equatorial Earth radii,
/// not the mean radius used elsewhere in GAIA; 60.2666 of these gives the
/// familiar 384400 km.
const MOON_EARTH_RADIUS_KM: f64 = 6378.14;

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
    let (ra, dec) = precess_j2000_to_date(ra_hours, dec_deg, unix_seconds);
    equatorial_to_az_ze(ra, dec, unix_seconds, lat_deg, lon_deg)
}

/// Azimuth and zenith angle of an equator-of-date position. Azimuth is measured
/// from north through east.
pub fn equatorial_to_az_ze(
    ra: f64,
    dec: f64,
    unix_seconds: f64,
    lat_deg: f64,
    lon_deg: f64,
) -> (f64, f64) {
    let sidereal = (gmst_deg(unix_seconds) + lon_deg) * DEG;
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

/// Local east/north/up direction rotated into the camera frame, the row-vector
/// product `sky . R` that `wisc_lens.camera_frame_vector` computes. `p` is the
/// optpar with the leading model number already stripped.
pub fn camera_frame_vector(az_deg: f64, el_deg: f64, p: &[f64]) -> [f64; 3] {
    let az = az_deg * DEG;
    let ze = (90.0 - el_deg) * DEG;
    let sky = [ze.sin() * az.sin(), ze.sin() * az.cos(), ze.cos()];
    let rot = camera_rotation(p[2], p[3], p[4]);
    [
        sky[0] * rot[0][0] + sky[1] * rot[1][0] + sky[2] * rot[2][0],
        sky[0] * rot[0][1] + sky[1] * rot[1][1] + sky[2] * rot[2][1],
        sky[0] * rot[0][2] + sky[1] * rot[1][2] + sky[2] * rot[2][2],
    ]
}

/// Rectilinear models project through `s1/s3`, so they only see the forward
/// hemisphere and diverge as a direction approaches their focal plane. The
/// fisheye models map `theta = atan2(radial, s3)` and legitimately image
/// directions with `s3 <= 0`, which must not be discarded.
fn is_rectilinear(model: i32) -> bool {
    model == 1 || model == BROWN_CONRADY_OPTMOD
}

/// Pixel of a star that this camera can actually measure, or None. The archive
/// holds six different lens models, from all-sky fisheyes that cover the whole
/// sky to a narrow rectilinear lens covering under a third of it, so the field
/// test has to come from the camera's own model rather than be assumed. `margin`
/// is the room a fitting patch needs inside the frame.
pub fn star_pixel(
    az_deg: f64,
    el_deg: f64,
    optpar: &[f64],
    width: f64,
    height: f64,
    margin: f64,
) -> Option<(f64, f64)> {
    if optpar.len() < 9 {
        return None;
    }
    let model = optpar[0] as i32;
    if is_rectilinear(model) {
        // Behind or on the focal plane a rectilinear projection is meaningless;
        // returning it would hand back coordinates of order 1e10.
        let s3 = camera_frame_vector(az_deg, el_deg, &optpar[1..])[2];
        if !(s3 > 1e-6) {
            return None;
        }
    }
    let (x, y) = az_el_to_pixel(az_deg, el_deg, optpar, width, height)?;
    let inside = x >= margin
        && y >= margin
        && x <= width - 1.0 - margin
        && y <= height - 1.0 - margin;
    if inside { Some((x, y)) } else { None }
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
    let [s1, s2, s3] = camera_frame_vector(az_deg, el_deg, p);
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

/// Result of a two-dimensional Gaussian fit to one star image, over a bilinear
/// background.
#[derive(Debug, Clone, Copy)]
pub struct GaussianFit {
    /// Background evaluated at the fitted centroid, i.e. the sky under the star.
    pub background: f64,
    /// Bilinear background coefficients per pixel, about the patch centre:
    /// `b0 + bx X + by Y + bxy X Y`.
    pub background_level: f64,
    pub background_dx: f64,
    pub background_dy: f64,
    pub background_dxy: f64,
    pub amplitude: f64,
    pub centre_x: f64,
    pub centre_y: f64,
    /// Major axis width. Always the larger, with `angle_deg` naming its direction.
    pub sigma_x: f64,
    /// Minor axis width.
    pub sigma_y: f64,
    /// Position angle of the major axis from the image x axis, in degrees,
    /// wrapped to [-90, 90). Meaningless for a round star; see `elongation`.
    pub angle_deg: f64,
    /// Total intensity above background, `2 pi A sx sy`. Rotation preserves area,
    /// so this is unchanged by the angle.
    pub flux: f64,
    /// Standard deviation of the fit residuals: the pixel noise this frame
    /// actually shows, against which a detection has to be judged.
    pub residual_std: f64,
    pub rms_residual: f64,
    /// Peak height in units of the residual scatter.
    pub amplitude_snr: f64,
    /// Integrated flux over its uncertainty. For white noise of scale `sigma`
    /// the error on an integrated Gaussian is `sigma sqrt(4 pi sx sy)`, the
    /// effective area of the profile, so this is the meaningful detection
    /// measure for a photometric point.
    pub flux_snr: f64,
}

impl GaussianFit {
    /// Ratio of major to minor width. At 1 the star is round and the angle
    /// carries no information.
    pub fn elongation(&self) -> f64 {
        if self.sigma_y > 0.0 { self.sigma_x / self.sigma_y } else { f64::INFINITY }
    }

    /// Whether the star stands far enough above the frame's own noise to be
    /// called a measurement. A peak comparable to the residual scatter is not a
    /// star, and its flux must not be fed to the cloud estimate as though it
    /// were: that would read noise as a clear sky.
    pub fn detectable(&self, min_snr: f64) -> bool {
        self.amplitude_snr >= min_snr && self.flux_snr >= min_snr && self.amplitude > 0.0
    }

    /// Background gradient magnitude per pixel, a warning that moonlight,
    /// twilight or an auroral arc is sloping across the patch.
    pub fn background_slope(&self) -> f64 {
        self.background_dx.hypot(self.background_dy)
    }
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

/// Fit a rotated elliptical Gaussian over a bilinear background:
/// `b0 + bx X + by Y + bxy X Y + A exp(-0.5 (u^2 + v^2))`, where `X` and `Y` are
/// offsets from the patch centre and `u`, `v` are offsets along the ellipse axes
/// divided by their widths. Ten parameters, by Levenberg-Marquardt with an
/// analytic Jacobian.
///
/// A flat background is not good enough around a real star. Moonlight, twilight
/// and auroral arcs all slope across a patch, and a constant term forced to
/// cover a gradient pushes the difference into the amplitude, biasing the flux.
/// The background is initialised by least squares on the border ring alone,
/// where the star does not reach, and then refined jointly so that flux is not
/// biased by star light leaking into the border estimate.
pub fn fit_gaussian(patch: &[f64], width: usize, height: usize) -> Option<GaussianFit> {
    if width < 7 || height < 7 || patch.len() != width * height {
        return None;
    }
    let (xc, yc) = ((width - 1) as f64 / 2.0, (height - 1) as f64 / 2.0);
    // Bilinear background from the border ring only.
    let mut ata = vec![vec![0.0; 4]; 4];
    let mut atb = vec![0.0; 4];
    for y in 0..height {
        for x in 0..width {
            if !(x == 0 || y == 0 || x == width - 1 || y == height - 1) {
                continue;
            }
            let (dx, dy) = (x as f64 - xc, y as f64 - yc);
            let basis = [1.0, dx, dy, dx * dy];
            let value = patch[y * width + x];
            for i in 0..4 {
                atb[i] += basis[i] * value;
                for j in 0..4 {
                    ata[i][j] += basis[i] * basis[j];
                }
            }
        }
    }
    let plane = solve(ata, atb).unwrap_or_else(|| vec![0.0, 0.0, 0.0, 0.0]);
    let background_at = |t: &[f64], x: f64, y: f64| {
        let (dx, dy) = (x - xc, y - yc);
        t[0] + t[1] * dx + t[2] * dy + t[3] * dx * dy
    };
    // Peak above that background, and intensity-weighted moments for the shape.
    let mut peak = (0usize, 0usize, f64::NEG_INFINITY);
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let above = patch[y * width + x] - background_at(&plane, x as f64, y as f64);
            if above > peak.2 {
                peak = (x, y, above);
            }
        }
    }
    let amplitude0 = peak.2.max(1e-6);
    let (mut sum, mut sx, mut sy) = (0.0, 0.0, 0.0);
    for y in 0..height {
        for x in 0..width {
            let w = (patch[y * width + x] - background_at(&plane, x as f64, y as f64)).max(0.0);
            sum += w;
            sx += w * x as f64;
            sy += w * y as f64;
        }
    }
    let (cx0, cy0) = if sum > 0.0 {
        (sx / sum, sy / sum)
    } else {
        (peak.0 as f64, peak.1 as f64)
    };
    let (mut mxx, mut myy, mut mxy) = (0.0, 0.0, 0.0);
    if sum > 0.0 {
        for y in 0..height {
            for x in 0..width {
                let w =
                    (patch[y * width + x] - background_at(&plane, x as f64, y as f64)).max(0.0);
                let (dx, dy) = (x as f64 - cx0, y as f64 - cy0);
                mxx += w * dx * dx;
                myy += w * dy * dy;
                mxy += w * dx * dy;
            }
        }
        mxx /= sum;
        myy /= sum;
        mxy /= sum;
    }
    let angle0 = 0.5 * (2.0 * mxy).atan2(mxx - myy);
    let half = 0.5 * (mxx + myy);
    let spread = (0.25 * (mxx - myy) * (mxx - myy) + mxy * mxy).max(0.0).sqrt();
    let limit = (width.min(height) as f64) / 3.0;
    let major0 = (half + spread).max(0.09).sqrt().clamp(0.4, limit);
    let minor0 = (half - spread).max(0.09).sqrt().clamp(0.4, limit);
    let mut theta = [
        plane[0], plane[1], plane[2], plane[3], amplitude0, cx0, cy0, major0, minor0, angle0,
    ];
    let residuals = |t: &[f64; 10]| -> f64 {
        let mut total = 0.0;
        for y in 0..height {
            for x in 0..width {
                let d = patch[y * width + x] - gaussian_value(t, x as f64, y as f64, xc, yc);
                total += d * d;
            }
        }
        total
    };
    let mut lambda = 1e-3;
    let mut cost = residuals(&theta);
    for _ in 0..140 {
        let mut jtj = vec![vec![0.0; 10]; 10];
        let mut jtr = vec![0.0; 10];
        for y in 0..height {
            for x in 0..width {
                let (model, jac) =
                    gaussian_value_and_jacobian(&theta, x as f64, y as f64, xc, yc);
                let residual = patch[y * width + x] - model;
                for i in 0..10 {
                    jtr[i] += jac[i] * residual;
                    for j in 0..10 {
                        jtj[i][j] += jac[i] * jac[j];
                    }
                }
            }
        }
        let mut improved = false;
        for _ in 0..14 {
            let mut damped = jtj.clone();
            for i in 0..10 {
                damped[i][i] *= 1.0 + lambda;
                if damped[i][i].abs() < 1e-14 {
                    // A round star leaves the angle unconstrained; damping keeps
                    // the system solvable and that step near zero.
                    damped[i][i] = lambda.max(1e-12);
                }
            }
            let Some(step) = solve(damped, jtr.clone()) else {
                lambda *= 10.0;
                continue;
            };
            let mut candidate = theta;
            for i in 0..10 {
                candidate[i] += step[i];
            }
            candidate[7] = candidate[7].abs().clamp(0.3, width as f64);
            candidate[8] = candidate[8].abs().clamp(0.3, height as f64);
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
    if theta[4] <= 0.0 || !theta.iter().all(|v| v.is_finite()) {
        return None;
    }
    // Residual scatter: the noise this frame actually shows.
    let count = (width * height) as f64;
    let mut mean = 0.0;
    for y in 0..height {
        for x in 0..width {
            mean += patch[y * width + x] - gaussian_value(&theta, x as f64, y as f64, xc, yc);
        }
    }
    mean /= count;
    let mut variance = 0.0;
    for y in 0..height {
        for x in 0..width {
            let d =
                patch[y * width + x] - gaussian_value(&theta, x as f64, y as f64, xc, yc) - mean;
            variance += d * d;
        }
    }
    let residual_std = (variance / count.max(1.0)).sqrt();
    // One canonical form: major axis first, angle wrapped to [-90, 90).
    let (mut major, mut minor, mut angle) = (theta[7], theta[8], theta[9]);
    if minor > major {
        std::mem::swap(&mut major, &mut minor);
        angle += std::f64::consts::FRAC_PI_2;
    }
    let mut angle_deg = (angle / DEG).rem_euclid(180.0);
    if angle_deg >= 90.0 {
        angle_deg -= 180.0;
    }
    let amplitude = theta[4];
    let flux = std::f64::consts::TAU * amplitude * major * minor;
    // For white noise the error on an integrated Gaussian is sigma sqrt(4 pi sx sy).
    let flux_sigma = residual_std * (4.0 * std::f64::consts::PI * major * minor).sqrt();
    Some(GaussianFit {
        background: background_at(&theta, theta[5], theta[6]),
        background_level: theta[0],
        background_dx: theta[1],
        background_dy: theta[2],
        background_dxy: theta[3],
        amplitude,
        centre_x: theta[5],
        centre_y: theta[6],
        sigma_x: major,
        sigma_y: minor,
        angle_deg,
        flux,
        residual_std,
        rms_residual: (cost / count).sqrt(),
        amplitude_snr: if residual_std > 0.0 { amplitude / residual_std } else { f64::INFINITY },
        flux_snr: if flux_sigma > 0.0 { flux / flux_sigma } else { f64::INFINITY },
    })
}

/// Offsets along the ellipse axes, divided by their widths.
fn gaussian_axes(t: &[f64; 10], x: f64, y: f64) -> (f64, f64) {
    let (dx, dy) = (x - t[5], y - t[6]);
    let (c, s) = (t[9].cos(), t[9].sin());
    ((dx * c + dy * s) / t[7], (-dx * s + dy * c) / t[8])
}

fn gaussian_value(t: &[f64; 10], x: f64, y: f64, xc: f64, yc: f64) -> f64 {
    let (u, v) = gaussian_axes(t, x, y);
    let (dx, dy) = (x - xc, y - yc);
    t[0] + t[1] * dx + t[2] * dy + t[3] * dx * dy + t[4] * (-0.5 * (u * u + v * v)).exp()
}

fn gaussian_value_and_jacobian(
    t: &[f64; 10],
    x: f64,
    y: f64,
    xc: f64,
    yc: f64,
) -> (f64, [f64; 10]) {
    let (u, v) = gaussian_axes(t, x, y);
    let (sx, sy) = (t[7], t[8]);
    let (c, s) = (t[9].cos(), t[9].sin());
    let e = (-0.5 * (u * u + v * v)).exp();
    let peak = t[4] * e;
    let (dx, dy) = (x - xc, y - yc);
    (
        t[0] + t[1] * dx + t[2] * dy + t[3] * dx * dy + peak,
        [
            1.0,
            dx,
            dy,
            dx * dy,
            e,
            peak * (u * c / sx - v * s / sy),
            peak * (u * s / sx + v * c / sy),
            peak * u * u / sx,
            peak * v * v / sy,
            // Vanishes when the widths are equal: a round star fixes no angle.
            peak * u * v * (sx / sy - sy / sx),
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

/// Write one frame's measurements. Re-measuring a frame replaces its rows, so
/// the pass is idempotent and can be re-run after a calibration change.
pub fn record_frame(
    conn: &rusqlite::Connection,
    source_id: &str,
    image_id: &str,
    observation_utc: &str,
    measurements: &[StarMeasurement],
) -> Result<usize> {
    let mut statement = conn.prepare(
        "INSERT INTO star_photometry(source_id,image_id,observation_utc,star_key,channel,
            ra_hours_j2000,dec_deg_j2000,vt_mag,azimuth_deg,elevation_deg,
            predicted_x,predicted_y,centroid_x,centroid_y,centroid_offset_px,
            background,amplitude,sigma_major,sigma_minor,angle_deg,flux,rms_residual,
            residual_std,amplitude_snr,flux_snr,background_dx,background_dy,background_dxy)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,
                ?23,?24,?25,?26,?27,?28)
         ON CONFLICT(source_id,image_id,star_key,channel) DO UPDATE SET
            observation_utc=excluded.observation_utc,azimuth_deg=excluded.azimuth_deg,
            elevation_deg=excluded.elevation_deg,predicted_x=excluded.predicted_x,
            predicted_y=excluded.predicted_y,centroid_x=excluded.centroid_x,
            centroid_y=excluded.centroid_y,centroid_offset_px=excluded.centroid_offset_px,
            background=excluded.background,amplitude=excluded.amplitude,
            sigma_major=excluded.sigma_major,sigma_minor=excluded.sigma_minor,
            angle_deg=excluded.angle_deg,flux=excluded.flux,rms_residual=excluded.rms_residual,
            residual_std=excluded.residual_std,amplitude_snr=excluded.amplitude_snr,
            flux_snr=excluded.flux_snr,background_dx=excluded.background_dx,
            background_dy=excluded.background_dy,background_dxy=excluded.background_dxy",
    )?;
    let mut written = 0;
    for m in measurements {
        let fit = m.fit.as_ref();
        statement.execute(rusqlite::params![
            source_id,
            image_id,
            observation_utc,
            m.star_key,
            m.channel,
            m.ra_hours,
            m.dec_deg,
            m.vt_mag,
            m.azimuth_deg,
            m.elevation_deg,
            m.predicted_x,
            m.predicted_y,
            fit.map(|f| f.centre_x),
            fit.map(|f| f.centre_y),
            m.centroid_offset_px,
            fit.map(|f| f.background),
            fit.map(|f| f.amplitude),
            fit.map(|f| f.sigma_x),
            fit.map(|f| f.sigma_y),
            fit.map(|f| f.angle_deg),
            fit.map(|f| f.flux),
            fit.map(|f| f.rms_residual),
            fit.map(|f| f.residual_std),
            fit.map(|f| f.amplitude_snr),
            fit.map(|f| f.flux_snr),
            fit.map(|f| f.background_dx),
            fit.map(|f| f.background_dy),
            fit.map(|f| f.background_dxy),
        ])?;
        written += 1;
    }
    Ok(written)
}

/// Record the sky conditions for one frame: the Moon, and the solar elevation
/// that gates everything. Idempotent per frame.
pub fn record_sky(
    conn: &rusqlite::Connection,
    source_id: &str,
    image_id: &str,
    observation_utc: &str,
    sun_elevation_deg: f64,
    moon: &MoonState,
) -> Result<()> {
    conn.execute(
        "INSERT INTO frame_sky(source_id,image_id,observation_utc,sun_elevation_deg,
            moon_azimuth_deg,moon_elevation_deg,moon_illuminated_fraction,moon_phase_angle_deg,
            moon_distance_km,moon_apparent_magnitude,moon_sky_brightness)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
         ON CONFLICT(source_id,image_id) DO UPDATE SET
            observation_utc=excluded.observation_utc,sun_elevation_deg=excluded.sun_elevation_deg,
            moon_azimuth_deg=excluded.moon_azimuth_deg,moon_elevation_deg=excluded.moon_elevation_deg,
            moon_illuminated_fraction=excluded.moon_illuminated_fraction,
            moon_phase_angle_deg=excluded.moon_phase_angle_deg,
            moon_distance_km=excluded.moon_distance_km,
            moon_apparent_magnitude=excluded.moon_apparent_magnitude,
            moon_sky_brightness=excluded.moon_sky_brightness",
        rusqlite::params![
            source_id,
            image_id,
            observation_utc,
            sun_elevation_deg,
            moon.azimuth_deg,
            moon.elevation_deg,
            moon.illuminated_fraction,
            moon.phase_angle_deg,
            moon.distance_km,
            moon.apparent_magnitude,
            moon.sky_brightness,
        ],
    )?;
    Ok(())
}

/// How much a star's brightness has varied over its series: 0 when steady, and
/// approaching 1 when it is sometimes almost extinguished. Uses percentiles
/// rather than the extremes so one bad frame cannot dominate the colour scale.
pub fn brightness_variation(fluxes: &[f64]) -> Option<f64> {
    let high = percentile(fluxes, 0.9)?;
    let low = percentile(fluxes, 0.1)?;
    if high <= 0.0 {
        return None;
    }
    Some((1.0 - (low / high)).clamp(0.0, 1.0))
}

/// Geocentric lunar position: right ascension and declination of date in
/// radians, and distance in kilometres.
///
/// A truncated orbital series with the principal perturbations (evection,
/// variation, the yearly and parallactic equations and the main latitude and
/// distance terms), accurate to a few arcminutes. That is far finer than needed
/// to decide whether the sky is moonlit, which is what this is for.
pub fn moon_equatorial(unix_seconds: f64) -> (f64, f64, f64) {
    // Days since 2000 Jan 0.0, the epoch this series is written for.
    let d = julian_date(unix_seconds) - 2451543.5;
    let norm = |v: f64| v.rem_euclid(360.0);
    // Solar elements, needed for the perturbations and the elongation.
    let ws = 282.9404 + 4.70935e-5 * d;
    let ms = norm(356.0470 + 0.9856002585 * d);
    // Lunar elements.
    let nm = norm(125.1228 - 0.0529538083 * d);
    let inc = 5.1454;
    let wm = norm(318.0634 + 0.1643573223 * d);
    let a = 60.2666; // Earth radii
    let e = 0.054900;
    let mm = norm(115.3654 + 13.0649929509 * d);
    // Kepler, iterated; the eccentricity is small so this converges at once.
    let mut ecc = mm * DEG + e * (mm * DEG).sin() * (1.0 + e * (mm * DEG).cos());
    for _ in 0..12 {
        let next = ecc - (ecc - e * ecc.sin() - mm * DEG) / (1.0 - e * ecc.cos());
        if (next - ecc).abs() < 1e-12 {
            ecc = next;
            break;
        }
        ecc = next;
    }
    let (xv, yv) = (a * (ecc.cos() - e), a * (1.0 - e * e).sqrt() * ecc.sin());
    let v = yv.atan2(xv);
    let r = xv.hypot(yv);
    // Into ecliptic coordinates.
    let (n, i, w) = (nm * DEG, inc * DEG, wm * DEG);
    let xe = r * (n.cos() * (v + w).cos() - n.sin() * (v + w).sin() * i.cos());
    let ye = r * (n.sin() * (v + w).cos() + n.cos() * (v + w).sin() * i.cos());
    let ze = r * (v + w).sin() * i.sin();
    let mut lon = ye.atan2(xe) / DEG;
    let mut lat = ze.atan2(xe.hypot(ye)) / DEG;
    let mut dist = r;
    // Principal perturbations, in degrees and Earth radii.
    let ls = norm(ms + ws);
    let lm = norm(nm + wm + mm);
    let elong = norm(lm - ls);
    let f = norm(lm - nm);
    let (s_mm, s_ms, s_d, s_f) = (mm * DEG, ms * DEG, elong * DEG, f * DEG);
    lon += -1.274 * (s_mm - 2.0 * s_d).sin()
        + 0.658 * (2.0 * s_d).sin()
        - 0.186 * s_ms.sin()
        - 0.059 * (2.0 * s_mm - 2.0 * s_d).sin()
        - 0.057 * (s_mm - 2.0 * s_d + s_ms).sin()
        + 0.053 * (s_mm + 2.0 * s_d).sin()
        + 0.046 * (2.0 * s_d - s_ms).sin()
        + 0.041 * (s_mm - s_ms).sin()
        - 0.035 * s_d.sin()
        - 0.031 * (s_mm + s_ms).sin()
        - 0.015 * (2.0 * s_f - 2.0 * s_d).sin()
        + 0.011 * (s_mm - 4.0 * s_d).sin();
    lat += -0.173 * (s_f - 2.0 * s_d).sin()
        - 0.055 * (s_mm - s_f - 2.0 * s_d).sin()
        - 0.046 * (s_mm + s_f - 2.0 * s_d).sin()
        + 0.033 * (s_f + 2.0 * s_d).sin()
        + 0.017 * (2.0 * s_mm + s_f).sin();
    dist += -0.58 * (s_mm - 2.0 * s_d).cos() - 0.46 * (2.0 * s_d).cos();
    // Ecliptic to equatorial.
    let obliquity = (23.4393 - 3.563e-7 * d) * DEG;
    let (lo, la) = (lon * DEG, lat * DEG);
    let (x, y, z) = (
        la.cos() * lo.cos(),
        la.cos() * lo.sin(),
        la.sin(),
    );
    let (yq, zq) = (
        y * obliquity.cos() - z * obliquity.sin(),
        y * obliquity.sin() + z * obliquity.cos(),
    );
    (
        yq.atan2(x).rem_euclid(std::f64::consts::TAU),
        zq.atan2(x.hypot(yq)),
        dist * MOON_EARTH_RADIUS_KM,
    )
}

/// Geocentric lunar direction in the same ECEF frame as `solar_direction_ecef`,
/// so the two can be compared directly.
pub fn moon_direction_ecef(unix_seconds: f64) -> [f64; 3] {
    let (ra, dec, _) = moon_equatorial(unix_seconds);
    let longitude = ra - gmst_deg(unix_seconds) * DEG;
    [
        dec.cos() * longitude.cos(),
        dec.cos() * longitude.sin(),
        dec.sin(),
    ]
}

/// Sun-Moon elongation seen from Earth, in radians. Frame independent, so the
/// two ECEF directions can be compared without undoing Earth rotation.
pub fn moon_elongation(unix_seconds: f64) -> f64 {
    let moon = moon_direction_ecef(unix_seconds);
    let sun = crate::geometry::solar_direction_ecef(unix_seconds);
    (moon[0] * sun[0] + moon[1] * sun[1] + moon[2] * sun[2])
        .clamp(-1.0, 1.0)
        .acos()
}

/// Illuminated fraction of the lunar disc. With the Sun far away the phase angle
/// is the supplement of the elongation, so `k = (1 - cos elongation)/2`: zero at
/// new moon and one at full.
pub fn moon_illuminated_fraction(unix_seconds: f64) -> f64 {
    ((1.0 - moon_elongation(unix_seconds).cos()) / 2.0).clamp(0.0, 1.0)
}

/// Everything about the Moon that bears on how bright the sky is.
#[derive(Debug, Clone, Copy)]
pub struct MoonState {
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub illuminated_fraction: f64,
    pub phase_angle_deg: f64,
    pub distance_km: f64,
    /// Apparent visual magnitude, the standard phase-angle approximation with a
    /// distance correction. More negative is brighter.
    pub apparent_magnitude: f64,
    /// A geometric proxy for how much moonlight reaches the sky at this station:
    /// the illuminated fraction times the sine of the lunar elevation, and zero
    /// while the Moon is down. This is a proxy, not a photometric sky model.
    pub sky_brightness: f64,
}

/// Lunar state at one station and time.
pub fn moon_state(unix_seconds: f64, lat_deg: f64, lon_deg: f64) -> MoonState {
    let (ra, dec, distance_km) = moon_equatorial(unix_seconds);
    let (az, ze) = equatorial_to_az_ze(ra, dec, unix_seconds, lat_deg, lon_deg);
    let elevation_deg = 90.0 - ze / DEG;
    let elongation = moon_elongation(unix_seconds);
    let phase_angle = std::f64::consts::PI - elongation;
    let illuminated = ((1.0 + phase_angle.cos()) / 2.0).clamp(0.0, 1.0);
    let phase_deg = phase_angle / DEG;
    // Allen's phase law, plus the inverse-square distance term.
    let magnitude = -12.73
        + 0.026 * phase_deg.abs()
        + 4.0e-9 * phase_deg.powi(4)
        + 5.0 * (distance_km / 384_400.0).log10();
    MoonState {
        azimuth_deg: az / DEG,
        elevation_deg,
        illuminated_fraction: illuminated,
        phase_angle_deg: phase_deg,
        distance_km,
        apparent_magnitude: magnitude,
        sky_brightness: if elevation_deg <= 0.0 {
            0.0
        } else {
            illuminated * (elevation_deg * DEG).sin()
        },
    }
}

/// A stable identity for a catalogue star. The WISCAT payload carries no
/// identifier, only position and magnitude, so the key is derived from the
/// J2000 position and is therefore reproducible across catalogue rebuilds.
pub fn star_key(ra_hours: f64, dec_deg: f64) -> String {
    format!("{ra_hours:.5}{dec_deg:+.5}")
}

/// One star measured in one frame, in one colour channel.
#[derive(Debug, Clone)]
pub struct StarMeasurement {
    pub star_key: String,
    pub channel: &'static str,
    pub ra_hours: f64,
    pub dec_deg: f64,
    pub vt_mag: f64,
    /// Sky position at the observation time, from the AIDA ephemeris.
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    /// Where the lens model says the star should land.
    pub predicted_x: f64,
    pub predicted_y: f64,
    /// The fit, present only when it converged within the acceptance radius.
    pub fit: Option<GaussianFit>,
    pub centroid_offset_px: Option<f64>,
}

/// The colour channels measured per star, plus the panchromatic mean. Fitting
/// each channel separately matters because cloud extinction is wavelength
/// dependent and the cameras differ in passband.
pub const CHANNELS: [&str; 4] = ["r", "g", "b", "mean"];

fn channel_value(pixel: &[u8], channel: usize) -> f64 {
    match channel {
        0 => pixel[0] as f64,
        1 => pixel[1] as f64,
        2 => pixel[2] as f64,
        _ => (pixel[0] as f64 + pixel[1] as f64 + pixel[2] as f64) / 3.0,
    }
}

/// Measure every catalogue star that this camera can see in one frame.
///
/// `patch_half` sets the fitting box, `max_offset_px` the acceptance radius
/// between the fitted centroid and the predicted position, and
/// `min_elevation_deg` excludes the horizon where obstruction and extinction
/// dominate. A star whose fit lands outside the radius is retained with
/// `fit: None` so the record shows it was looked for and not found, which is
/// itself evidence of cloud.
#[allow(clippy::too_many_arguments)]
pub fn measure_frame(
    image: &image::RgbImage,
    optpar: &[f64],
    lat_deg: f64,
    lon_deg: f64,
    unix_seconds: f64,
    stars: &[Star],
    patch_half: usize,
    max_offset_px: f64,
    min_elevation_deg: f64,
) -> Vec<StarMeasurement> {
    let (width, height) = (image.width() as f64, image.height() as f64);
    let mut out = Vec::new();
    let side = patch_half * 2 + 1;
    for star in stars {
        let (az, ze) = star_az_ze(star.ra_hours, star.dec_deg, unix_seconds, lat_deg, lon_deg);
        let (az_deg, el_deg) = (az / DEG, 90.0 - ze / DEG);
        if el_deg < min_elevation_deg {
            continue;
        }
        let Some((px, py)) =
            star_pixel(az_deg, el_deg, optpar, width, height, patch_half as f64)
        else {
            continue;
        };
        let (x0, y0) = (px.round() as i64 - patch_half as i64, py.round() as i64 - patch_half as i64);
        let key = star_key(star.ra_hours, star.dec_deg);
        for (index, channel) in CHANNELS.iter().enumerate() {
            let mut patch = Vec::with_capacity(side * side);
            for row in 0..side {
                for column in 0..side {
                    let x = (x0 + column as i64).clamp(0, image.width() as i64 - 1) as u32;
                    let y = (y0 + row as i64).clamp(0, image.height() as i64 - 1) as u32;
                    patch.push(channel_value(&image.get_pixel(x, y).0, index));
                }
            }
            let fit = fit_gaussian(&patch, side, side);
            // Patch coordinates back to image coordinates before comparing.
            let (fit, offset) = match fit {
                Some(f) => {
                    let cx = x0 as f64 + f.centre_x;
                    let cy = y0 as f64 + f.centre_y;
                    let offset = ((cx - px).powi(2) + (cy - py).powi(2)).sqrt();
                    let placed = GaussianFit { centre_x: cx, centre_y: cy, ..f };
                    if offset <= max_offset_px {
                        (Some(placed), Some(offset))
                    } else {
                        (None, Some(offset))
                    }
                }
                None => (None, None),
            };
            out.push(StarMeasurement {
                star_key: key.clone(),
                channel,
                ra_hours: star.ra_hours,
                dec_deg: star.dec_deg,
                vt_mag: star.vt_mag,
                azimuth_deg: az_deg,
                elevation_deg: el_deg,
                predicted_x: px,
                predicted_y: py,
                fit,
                centroid_offset_px: offset,
            });
        }
    }
    out
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

    /// One real archived calibration per lens model actually in use, with golden
    /// pixel positions generated from wisc_lens.py. The archive holds optmod 2,
    /// 3, 4, 6, 12 and Brown-Conrady 20, so nothing may assume a single model.
    #[allow(clippy::type_complexity)]
    fn real_models() -> Vec<(&'static str, Vec<f64>, f64, f64, Vec<(f64, f64, f64, f64)>)> {
        vec![
            ("optmod 2 irf-kiruna-alis", vec![2.0,0.7068216520250072,0.7070094774383674,-1.5099938871193461,-1.288201852920821,-170.19172827706473,0.007550383264794,-0.00418607265861641,0.47912975681992587], 2832.0, 2832.0,
             vec![(0.0,5.0,1208.1293507590,100.3540532616),(140.0,20.0,868.4994713983,2352.3628253871),(280.0,35.0,2248.7508276463,1092.3441571193),(60.0,55.0,866.7666930445,1188.2963405433),(200.0,70.0,1574.3312327532,1675.3506049381)]),
            ("optmod 3 ucalgary-smileasi_pfrr", vec![3.0,-0.2927734251609883,0.29277342529462974,0.39928394244543053,1.469875411915926,188.00220882691005,0.00488281064159167,0.0019531271725367,0.9999999991977904], 512.0, 512.0,
             vec![(0.0,5.0,287.7903703882,39.5735160170),(140.0,20.0,354.5204876389,414.0169181857),(280.0,35.0,119.4762874209,213.9360812184),(60.0,55.0,340.7081793976,225.0432211135),(200.0,70.0,231.3465227808,305.8530340165)]),
            ("optmod 4 ucalgary-smileasi_gill", vec![4.0,-0.2927734252507357,0.29277342541599244,4.48477356487659,-3.4591008800656313,-175.5039014342125,0.00878906320826154,-0.00756835917505016,1.000000001573116], 512.0, 512.0,
             vec![(0.0,5.0,276.0716715797,20.2480407232),(140.0,20.0,356.5455229967,389.0910058630),(280.0,35.0,108.4718439133,207.8421335669),(60.0,55.0,331.8890106161,203.0510327405),(200.0,70.0,227.3931137260,289.1632329920)]),
            ("optmod 6 ucalgary-rego_gill", vec![6.0,-0.6172272925103574,-0.617222824368368,-0.1168252781882293,-0.9592419871126202,0.7506821914958062,0.01336127470522225,-0.00051179405820558,1.0], 256.0, 256.0,
             vec![(0.0,5.0,128.9899826615,19.1576700943),(140.0,20.0,73.2332189628,196.2162922483),(280.0,35.0,202.0039652904,112.4009882182),(60.0,55.0,88.7428547251,102.4838647728),(200.0,70.0,139.9581141169,151.2288247804)]),
            ("optmod 12 ucalgary-smileasi_fsmi", vec![12.0,-0.2927734250138068,0.29277342555496505,1.7275466741913352,-0.6447136811292984,147.01600398809163,0.00830078169684825,-0.00048827916267577,0.0], 512.0, 512.0,
             vec![(0.0,5.0,136.7682971632,67.1139121949),(140.0,20.0,430.0520136606,308.1236237311),(280.0,35.0,122.1941974308,312.2766149537),(60.0,55.0,296.5394777910,174.4162770068),(200.0,70.0,266.5000717120,306.7454354179)]),
            ("optmod 20 starvisor-bagdarin", vec![20.0,0.6194983949956074,1.0995315567474164,-17.782510224065245,57.41456874089842,17.661231702934742,0.02370202736662068,0.00878559708063632,-0.4004940010351374,0.16025130705733037,-0.03120627655512529,-0.00023586181440057,2.509839747437e-05], 3840.0, 2160.0,
             vec![(0.0,5.0,2414.1238806459,2152.8467575460),(310.0,15.0,508.0795724172,1412.1870352804),(340.0,25.0,1661.2828218212,1308.3301436310),(0.0,40.0,2382.3279630361,721.2113972910),(30.0,50.0,3092.5236236882,160.8146407287)]),
        ]
    }

    #[test]
    fn every_lens_model_in_the_archive_projects_to_its_reference() {
        for (name, optpar, w, h, cases) in real_models() {
            for (az, el, x, y) in cases {
                let (gx, gy) = az_el_to_pixel(az, el, &optpar, w, h)
                    .unwrap_or_else(|| panic!("{name}: no projection at az {az} el {el}"));
                assert!((gx - x).abs() < 1e-6, "{name}: x {gx} vs reference {x}");
                assert!((gy - y).abs() < 1e-6, "{name}: y {gy} vs reference {y}");
            }
        }
    }

    #[test]
    fn the_field_test_comes_from_each_camera_own_model() {
        let models = real_models();
        // The all-sky fisheyes cover the whole sky above 5 degrees. Optmod 4 in
        // particular images six directions with s3 <= 0, so a blanket
        // forward-hemisphere test would wrongly discard real pixels.
        for (name, optpar, w, h, _) in models.iter().filter(|m| !m.0.contains("optmod 20")) {
            let mut accepted = 0;
            let mut behind = 0;
            for el in (5..90).step_by(5) {
                for az in (0..360).step_by(10) {
                    if star_pixel(az as f64, el as f64, optpar, *w, *h, 0.0).is_some() {
                        accepted += 1;
                    }
                    if camera_frame_vector(az as f64, el as f64, &optpar[1..])[2] <= 0.0 {
                        behind += 1;
                    }
                }
            }
            assert_eq!(accepted, 612, "{name}: an all-sky lens should see the whole sky");
            if name.contains("optmod 4") {
                assert!(behind > 0, "{name}: expected in-field directions with s3 <= 0");
            }
        }
        // The rectilinear camera sees under a third of the sky, and the
        // directions it cannot see must be refused, not returned as huge numbers.
        let (_, optpar, w, h, _) = models.iter().find(|m| m.0.contains("optmod 20")).unwrap();
        let mut accepted = 0;
        for el in (5..90).step_by(5) {
            for az in (0..360).step_by(10) {
                if star_pixel(az as f64, el as f64, optpar, *w, *h, 0.0).is_some() {
                    accepted += 1;
                }
            }
        }
        assert!((150..=200).contains(&accepted), "rectilinear coverage {accepted}/612");
        // These two diverge to 1e8 and 1e10 pixels in the raw projection.
        for (az, el) in [(120.0, 40.0), (250.0, 20.0)] {
            let raw = az_el_to_pixel(az, el, optpar, *w, *h).unwrap();
            assert!(raw.0.abs() > 1e6 || raw.1.abs() > 1e6, "expected divergence at {az},{el}");
            assert!(
                star_pixel(az, el, optpar, *w, *h, 0.0).is_none(),
                "a direction outside the rectilinear field must be refused"
            );
        }
        // The margin keeps room for a fitting patch inside the frame.
        let kiruna = &models[0];
        let centre = star_pixel(0.0, 89.0, &kiruna.1, kiruna.2, kiruna.3, 9.0);
        assert!(centre.is_some(), "a star near zenith has room for its patch");
        assert!(
            star_pixel(0.0, 5.0, &kiruna.1, kiruna.2, kiruna.3, 120.0).is_none(),
            "a star too close to the frame edge for its patch must be refused"
        );
    }

    /// Render a noiseless star patch for fitting tests.
    fn synth(w: usize, h: usize, t: [f64; 10]) -> Vec<f64> {
        let (xc, yc) = ((w - 1) as f64 / 2.0, (h - 1) as f64 / 2.0);
        (0..w * h)
            .map(|i| gaussian_value(&t, (i % w) as f64, (i / w) as f64, xc, yc))
            .collect()
    }

    #[test]
    fn gaussian_fit_recovers_a_known_star() {
        let truth = [12.0, 0.0, 0.0, 0.0, 240.0, 8.3, 7.6, 2.1, 1.8, 0.0];
        let patch = synth(17, 17, truth);
        let fit = fit_gaussian(&patch, 17, 17).expect("fit");
        assert!((fit.background - truth[0]).abs() < 0.05, "background {}", fit.background);
        assert!((fit.amplitude - truth[4]).abs() < 0.5, "amplitude {}", fit.amplitude);
        assert!((fit.centre_x - truth[5]).abs() < 0.01, "cx {}", fit.centre_x);
        assert!((fit.centre_y - truth[6]).abs() < 0.01, "cy {}", fit.centre_y);
        assert!((fit.sigma_x - truth[7]).abs() < 0.02, "sx {}", fit.sigma_x);
        assert!((fit.sigma_y - truth[8]).abs() < 0.02, "sy {}", fit.sigma_y);
        // Axis aligned, so the recovered angle must be near zero.
        assert!(fit.angle_deg.abs() < 2.0, "angle {}", fit.angle_deg);
        // Total intensity above background is the analytic integral.
        let expected = std::f64::consts::TAU * truth[4] * truth[7] * truth[8];
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
        let truth = [30.0, 0.0, 0.0, 0.0, 180.0, 9.0, 9.0, 2.0, 2.0, 0.0];
        let mut patch = synth(19, 19, truth);
        let mut seed = 12345u64;
        for value in patch.iter_mut() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let unit = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 0.5;
            *value += unit * 6.0;
        }
        let fit = fit_gaussian(&patch, 19, 19).expect("fit under noise");
        let expected = std::f64::consts::TAU * truth[4] * truth[7] * truth[8];
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
        let truth = [10.0, 0.0, 0.0, 0.0, 200.0, 13.5, 4.5, 1.6, 1.6, 0.0];
        let patch = synth(19, 19, truth);
        let fit = fit_gaussian(&patch, 19, 19).expect("fit");
        let offset = ((fit.centre_x - 9.0).powi(2) + (fit.centre_y - 9.0).powi(2)).sqrt();
        assert!(offset > 3.0, "centroid offset {offset} should exceed the 3 px limit");
        assert!((fit.centre_x - truth[5]).abs() < 0.05);
        assert!((fit.centre_y - truth[6]).abs() < 0.05);
    }

    #[test]
    fn gaussian_fit_recovers_a_rotated_elongated_star() {
        // A trailed or astigmatic image: clearly elliptical and clearly rotated.
        for truth_angle_deg in [-70.0, -35.0, -5.0, 0.0, 20.0, 52.0, 80.0] {
            let truth = [
                18.0, 0.0, 0.0, 0.0, 300.0, 10.4, 9.7, 3.2, 1.3, truth_angle_deg * DEG,
            ];
            let patch = synth(25, 25, truth);
            let fit = fit_gaussian(&patch, 25, 25).expect("fit");
            assert!((fit.sigma_x - 3.2).abs() < 0.05, "major {} at {truth_angle_deg}", fit.sigma_x);
            assert!((fit.sigma_y - 1.3).abs() < 0.05, "minor {} at {truth_angle_deg}", fit.sigma_y);
            assert!((fit.centre_x - 10.4).abs() < 0.02 && (fit.centre_y - 9.7).abs() < 0.02);
            // Compare angles modulo 180, since an ellipse has no head or tail.
            let delta = (fit.angle_deg - truth_angle_deg + 90.0).rem_euclid(180.0) - 90.0;
            assert!(
                delta.abs() < 1.0,
                "angle {} vs truth {truth_angle_deg}",
                fit.angle_deg
            );
            // Canonical form: major first, angle in [-90, 90).
            assert!(fit.sigma_x >= fit.sigma_y);
            assert!(fit.angle_deg >= -90.0 && fit.angle_deg < 90.0);
            assert!((fit.elongation() - 3.2 / 1.3).abs() < 0.1);
        }
    }

    #[test]
    fn the_bilinear_background_is_recovered_and_does_not_bias_the_flux() {
        // Moonlight, twilight and auroral arcs slope across a patch. A constant
        // background forced to cover a gradient pushes the difference into the
        // amplitude, so the gradient has to be part of the model.
        for (bx, by, bxy) in [(0.0, 0.0, 0.0), (1.8, -1.1, 0.0), (-2.4, 0.9, 0.05), (0.6, 0.6, -0.04)] {
            let truth = [40.0, bx, by, bxy, 260.0, 12.3, 11.6, 2.5, 1.7, 24.0 * DEG];
            let patch = synth(25, 25, truth);
            let fit = fit_gaussian(&patch, 25, 25).expect("fit");
            assert!((fit.background_level - 40.0).abs() < 0.4, "level {}", fit.background_level);
            assert!((fit.background_dx - bx).abs() < 0.05, "dx {} vs {bx}", fit.background_dx);
            assert!((fit.background_dy - by).abs() < 0.05, "dy {} vs {by}", fit.background_dy);
            assert!((fit.background_dxy - bxy).abs() < 0.01, "dxy {} vs {bxy}", fit.background_dxy);
            // The flux must be right whatever the gradient does.
            let expected = std::f64::consts::TAU * 260.0 * 2.5 * 1.7;
            assert!(
                (fit.flux - expected).abs() / expected < 0.02,
                "flux {} with gradient ({bx},{by},{bxy}) vs {expected}",
                fit.flux
            );
            // And the amplitude must not have absorbed part of the slope.
            assert!((fit.amplitude - 260.0).abs() < 4.0, "amplitude {}", fit.amplitude);
            // background is reported under the star, so it follows the slope.
            let at_star = 40.0 + bx * (12.3 - 12.0) + by * (11.6 - 12.0) + bxy * (12.3 - 12.0) * (11.6 - 12.0);
            assert!((fit.background - at_star).abs() < 0.5, "background under star {}", fit.background);
            assert!(fit.background_slope() >= 0.0);
        }
    }

    #[test]
    fn residual_scatter_decides_whether_a_star_is_a_measurement() {
        // Deterministic noise so the test cannot flake.
        let noisy = |amplitude: f64, span: f64| {
            let truth = [50.0, 0.3, -0.2, 0.0, amplitude, 11.0, 11.0, 2.0, 2.0, 0.0];
            let mut patch = synth(23, 23, truth);
            let mut seed = 99u64;
            for value in patch.iter_mut() {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                let unit = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 0.5;
                *value += unit * span;
            }
            fit_gaussian(&patch, 23, 23)
        };
        // Uniform noise of width w has standard deviation w/sqrt(12).
        let span = 20.0;
        let expected_noise = span / 12f64.sqrt();
        let bright = noisy(400.0, span).expect("bright fit");
        assert!(
            (bright.residual_std - expected_noise).abs() / expected_noise < 0.3,
            "residual_std {} vs injected {expected_noise}",
            bright.residual_std
        );
        assert!(bright.amplitude_snr > 20.0, "bright amplitude snr {}", bright.amplitude_snr);
        assert!(bright.flux_snr > 20.0, "bright flux snr {}", bright.flux_snr);
        assert!(bright.detectable(5.0), "a bright star must be a measurement");
        // A peak comparable to the scatter is not a star. Its flux must not be
        // handed to the cloud estimate, which would read noise as clear sky.
        let faint = noisy(expected_noise * 1.2, span).expect("faint fit");
        assert!(faint.amplitude_snr < 5.0, "faint amplitude snr {}", faint.amplitude_snr);
        assert!(!faint.detectable(5.0), "a star at the noise level is not detectable");
        // Monotone in brightness once above the noise. Below it the fitted peak
        // is whatever the noise offers, so no ordering can be expected there,
        // which is precisely why a detectability test is needed.
        let mut previous = 0.0;
        for amplitude in [20.0, 60.0, 150.0, 400.0] {
            let snr = noisy(amplitude, span).expect("fit").amplitude_snr;
            assert!(snr > previous, "snr {snr} should exceed {previous} at amplitude {amplitude}");
            previous = snr;
        }
    }

    #[test]
    fn rotation_does_not_change_the_flux() {
        // Rotation is area preserving, so the integral stays 2 pi A sx sy. An
        // axis-aligned fit would instead misestimate an elongated rotated star.
        let expected = std::f64::consts::TAU * 260.0 * 3.0 * 1.2;
        for angle_deg in [0.0, 15.0, 30.0, 45.0, 60.0, 75.0, 90.0, 135.0] {
            let truth = [9.0, 0.0, 0.0, 0.0, 260.0, 12.0, 12.0, 3.0, 1.2, angle_deg * DEG];
            let patch = synth(27, 27, truth);
            let fit = fit_gaussian(&patch, 27, 27).expect("fit");
            assert!(
                (fit.flux - expected).abs() / expected < 0.02,
                "flux {} at {angle_deg} deg vs analytic {expected}",
                fit.flux
            );
        }
    }

    #[test]
    fn a_round_star_still_fits_though_its_angle_is_meaningless() {
        // With equal widths the angle Jacobian vanishes; the solve must stay
        // stable and the flux must still be right.
        let truth = [22.0, 0.0, 0.0, 0.0, 150.0, 8.0, 8.0, 2.4, 2.4, 0.0];
        let patch = synth(21, 21, truth);
        let fit = fit_gaussian(&patch, 21, 21).expect("round fit");
        assert!((fit.elongation() - 1.0).abs() < 0.05, "elongation {}", fit.elongation());
        assert!(fit.angle_deg.is_finite());
        let expected = std::f64::consts::TAU * 150.0 * 2.4 * 2.4;
        assert!((fit.flux - expected).abs() / expected < 0.02, "flux {}", fit.flux);
        assert!((fit.centre_x - 8.0).abs() < 0.02 && (fit.centre_y - 8.0).abs() < 0.02);
    }

    #[test]
    fn measure_frame_finds_planted_stars_and_flags_missing_ones() {
        // Plant Gaussians where the real Kiruna lens model predicts, then check
        // recovery. The candidate list is filtered by the same elevation floor
        // measure_frame uses, so the test cannot plant a star the code excludes.
        let optpar = KIRUNA.to_vec();
        let (w, h) = (2832u32, 2832u32);
        let (lat, lon, unix) = (67.84, 20.41, 1789077600.0);
        let min_elevation = 10.0;
        let candidates = [
            Star { ra_hours: 2.5301944, dec_deg: 89.2641111, vt_mag: 2.02 },
            Star { ra_hours: 18.6156, dec_deg: 38.7837, vt_mag: 0.03 },
            Star { ra_hours: 20.6905, dec_deg: 45.2803, vt_mag: 1.25 },
            Star { ra_hours: 5.2782, dec_deg: 45.9980, vt_mag: 0.08 },
            Star { ra_hours: 14.2610, dec_deg: 19.1824, vt_mag: -0.05 },
        ];
        let mut usable = Vec::new();
        for star in candidates {
            let (az, ze) = star_az_ze(star.ra_hours, star.dec_deg, unix, lat, lon);
            let (az_deg, el_deg) = (az / DEG, 90.0 - ze / DEG);
            if el_deg < min_elevation {
                continue;
            }
            if let Some((px, py)) =
                star_pixel(az_deg, el_deg, &optpar, w as f64, h as f64, 9.0)
            {
                usable.push((star, px, py));
            }
        }
        assert!(usable.len() >= 3, "need three usable stars, got {}", usable.len());
        // Displace the last usable star past the acceptance radius.
        let displaced = usable.len() - 1;
        let mut image = image::RgbImage::from_pixel(w, h, image::Rgb([20, 18, 22]));
        for (index, (_, px, py)) in usable.iter().enumerate() {
            let shift = if index == displaced { 7.0 } else { 0.0 };
            for dy in -9i64..=9 {
                for dx in -9i64..=9 {
                    let (x, y) = ((px + shift).round() as i64 + dx, py.round() as i64 + dy);
                    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
                        continue;
                    }
                    let r2 = (dx * dx + dy * dy) as f64;
                    let value = 200.0 * (-0.5 * r2 / 4.0).exp();
                    let pixel = image.get_pixel_mut(x as u32, y as u32);
                    for c in 0..3 {
                        pixel.0[c] = (pixel.0[c] as f64 + value).min(255.0) as u8;
                    }
                }
            }
        }
        let stars: Vec<Star> = usable.iter().map(|(s, _, _)| *s).collect();
        let measured =
            measure_frame(&image, &optpar, lat, lon, unix, &stars, 9, 3.0, min_elevation);
        assert_eq!(
            measured.len(),
            usable.len() * CHANNELS.len(),
            "one row per usable star and channel"
        );
        let displaced_key = star_key(usable[displaced].0.ra_hours, usable[displaced].0.dec_deg);
        let mut found = 0;
        for m in &measured {
            if m.star_key == displaced_key {
                // Looked for and not found: the record and the distance are kept,
                // which is itself evidence about the sky.
                assert!(m.fit.is_none(), "a displaced star must not be accepted");
                assert!(m.centroid_offset_px.unwrap() > 3.0);
                continue;
            }
            let fit = m.fit.as_ref().unwrap_or_else(|| {
                panic!("planted star {} channel {} should be found", m.star_key, m.channel)
            });
            found += 1;
            assert!(m.centroid_offset_px.unwrap() <= 3.0);
            assert!(fit.flux > 0.0);
            assert!((fit.background - 20.0).abs() < 12.0, "background {}", fit.background);
            // Centroid is reported in image coordinates, not patch coordinates.
            assert!((fit.centre_x - m.predicted_x).abs() < 3.0);
            assert!((fit.centre_y - m.predicted_y).abs() < 3.0);
        }
        assert_eq!(found, (usable.len() - 1) * CHANNELS.len());
        // Sky and image position are retained on every row, found or not.
        assert!(measured.iter().all(|m| m.elevation_deg >= min_elevation
            && (0.0..360.0).contains(&m.azimuth_deg)
            && m.predicted_x.is_finite()
            && m.predicted_y.is_finite()));
        assert!(measured.iter().any(|m| m.channel == "mean"));
    }

    #[test]
    fn recorded_rows_keep_every_parameter_and_re_measuring_replaces_them() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // This exercises the photometry table's own shape, so the parent source
        // and image rows are not created and the references are not enforced.
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        let fit = GaussianFit {
            background: 31.5,
            background_level: 31.0,
            background_dx: 0.12,
            background_dy: -0.08,
            background_dxy: 0.001,
            residual_std: 1.9,
            amplitude_snr: 110.5,
            flux_snr: 48.2,
            amplitude: 210.0,
            centre_x: 1402.25,
            centre_y: 1380.75,
            sigma_x: 2.6,
            sigma_y: 1.9,
            angle_deg: -42.5,
            flux: 6520.0,
            rms_residual: 2.1,
        };
        let found = StarMeasurement {
            star_key: star_key(2.5301944, 89.2641111),
            channel: "g",
            ra_hours: 2.5301944,
            dec_deg: 89.2641111,
            vt_mag: 2.02,
            azimuth_deg: 1.536,
            elevation_deg: 68.08,
            predicted_x: 1402.0,
            predicted_y: 1381.0,
            fit: Some(fit),
            centroid_offset_px: Some(0.35),
        };
        let missing = StarMeasurement {
            star_key: star_key(18.6156, 38.7837),
            channel: "g",
            fit: None,
            centroid_offset_px: Some(9.4),
            ..found.clone()
        };
        let written =
            record_frame(&conn, "cam", "img-1", "2026-09-10T22:00:00+00:00", &[found.clone(), missing])
                .unwrap();
        assert_eq!(written, 2);
        // Everything the plots and the cloud estimate need comes back intact.
        let (bg, amp, major, minor, angle, flux, rms, cx, cy, px, py, off, az, el): (
            f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64,
        ) = conn
            .query_row(
                "SELECT background,amplitude,sigma_major,sigma_minor,angle_deg,flux,rms_residual,
                        centroid_x,centroid_y,predicted_x,predicted_y,centroid_offset_px,
                        azimuth_deg,elevation_deg
                 FROM star_photometry WHERE star_key=?1 AND channel='g'",
                [&found.star_key],
                |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                        r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?, r.get(10)?, r.get(11)?,
                        r.get(12)?, r.get(13)?))
                },
            )
            .unwrap();
        assert!((bg - 31.5).abs() < 1e-9, "background must be stored");
        assert!((amp - 210.0).abs() < 1e-9);
        assert!((major - 2.6).abs() < 1e-9 && (minor - 1.9).abs() < 1e-9);
        assert!((angle - -42.5).abs() < 1e-9, "the position angle must be stored");
        assert!((flux - 6520.0).abs() < 1e-9 && (rms - 2.1).abs() < 1e-9);
        // Both image positions, predicted and fitted, plus their separation.
        assert!((cx - 1402.25).abs() < 1e-9 && (cy - 1380.75).abs() < 1e-9);
        assert!((px - 1402.0).abs() < 1e-9 && (py - 1381.0).abs() < 1e-9);
        assert!((off - 0.35).abs() < 1e-9);
        // And the sky position.
        assert!((az - 1.536).abs() < 1e-9 && (el - 68.08).abs() < 1e-9);
        // A star that was not found is still on record, with a null fit.
        let (nulls, offset): (i64, f64) = conn
            .query_row(
                "SELECT flux IS NULL, centroid_offset_px FROM star_photometry WHERE star_key=?1",
                [star_key(18.6156, 38.7837)],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(nulls, 1, "an unfound star keeps a row with no flux");
        assert!(offset > 3.0);
        // Re-measuring the same frame replaces rather than duplicates.
        let mut improved = found.clone();
        improved.fit = Some(GaussianFit { flux: 7000.0, ..fit });
        record_frame(&conn, "cam", "img-1", "2026-09-10T22:00:00+00:00", &[improved]).unwrap();
        let (count, newest): (i64, f64) = conn
            .query_row(
                "SELECT count(*), max(flux) FROM star_photometry WHERE star_key=?1 AND channel='g'",
                [&found.star_key],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(count, 1, "re-measuring must not duplicate the row");
        assert!((newest - 7000.0).abs() < 1e-9);
    }

    #[test]
    fn brightness_variation_separates_steady_stars_from_obscured_ones() {
        // A star of constant brightness has no variation.
        assert!(brightness_variation(&[100.0; 20]).unwrap() < 1e-12);
        // One that is sometimes almost extinguished approaches 1.
        let mut intermittent: Vec<f64> = vec![100.0; 10];
        intermittent.extend(vec![1.0; 10]);
        let v = brightness_variation(&intermittent).unwrap();
        assert!(v > 0.9, "intermittent star variation {v}");
        // Percentiles, not extremes: a single dropout barely moves it.
        let mut one_bad = vec![100.0; 40];
        one_bad[7] = 0.5;
        assert!(brightness_variation(&one_bad).unwrap() < 0.05);
        assert!(brightness_variation(&[]).is_none());
        assert!(brightness_variation(&[0.0, 0.0]).is_none());
    }

    #[test]
    fn the_lunar_ephemeris_obeys_its_physical_invariants() {
        // No ephemeris library is available here, so the Moon is checked against
        // properties of its actual orbit rather than golden values.
        let start = 1789077600.0; // 2026-09-10T22:00:00Z
        let mut min_dist = f64::INFINITY;
        let mut max_dist: f64 = 0.0;
        let mut max_lat: f64 = 0.0;
        for step in 0..400 {
            let unix = start + step as f64 * 86400.0;
            let (ra, dec, distance) = moon_equatorial(unix);
            assert!(ra.is_finite() && (0.0..std::f64::consts::TAU).contains(&ra));
            assert!(dec.abs() < 30.0 * DEG, "declination {} out of range", dec / DEG);
            min_dist = min_dist.min(distance);
            max_dist = max_dist.max(distance);
            // Ecliptic latitude is bounded by the orbital inclination plus the
            // perturbation terms, about 5.3 degrees.
            let obliquity = 23.4393 * DEG;
            let (x, y, z) = (dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin());
            let ecliptic_z = -y * obliquity.sin() + z * obliquity.cos();
            max_lat = max_lat.max((ecliptic_z.asin() / DEG).abs());
            let k = moon_illuminated_fraction(unix);
            assert!((0.0..=1.0).contains(&k), "illuminated fraction {k}");
        }
        assert!(max_lat < 5.6, "ecliptic latitude reached {max_lat} deg");
        // Perigee and apogee bracket the real range, 356400 to 406700 km.
        assert!((350_000.0..372_000.0).contains(&min_dist), "perigee {min_dist} km");
        assert!((398_000.0..415_000.0).contains(&max_dist), "apogee {max_dist} km");
    }

    #[test]
    fn the_lunar_and_solar_longitudes_beat_at_the_synodic_month() {
        // The strongest check available without an ephemeris library: the phase
        // cycle is the beat between two independently implemented bodies, so if
        // either the lunar or the solar series were wrong this period would not
        // come out at 29.53 days.
        let start = 1789077600.0;
        let step = 1800.0; // half hour
        let mut fulls = Vec::new();
        let mut previous = moon_illuminated_fraction(start);
        // Take the initial direction from the data. Assuming the Moon is waxing
        // registers a false maximum on the first sample whenever it is waning.
        let mut rising = moon_illuminated_fraction(start + step) > previous;
        for i in 1..(70.0 * 86400.0 / step) as usize {
            let unix = start + i as f64 * step;
            let k = moon_illuminated_fraction(unix);
            if rising && k < previous {
                // Only a genuine full moon counts, which also rejects any
                // numerical wiggle in the nearly flat peak.
                if previous > 0.99 {
                    fulls.push(unix - step);
                }
                rising = false;
            } else if !rising && k > previous {
                rising = true;
            }
            previous = k;
        }
        assert!(fulls.len() >= 2, "expected at least two full moons in 60 days");
        let period = (fulls[1] - fulls[0]) / 86400.0;
        assert!(
            (period - 29.53).abs() < 0.4,
            "synodic period came out {period} days, not 29.53"
        );
        // At full moon the Moon is opposite the Sun; at new moon, beside it.
        let full = fulls[0];
        assert!(moon_elongation(full) / DEG > 172.0, "full moon elongation {}", moon_elongation(full) / DEG);
        assert!(moon_illuminated_fraction(full) > 0.995);
        let new = full + 0.5 * 29.53 * 86400.0;
        assert!(moon_elongation(new) / DEG < 12.0, "new moon elongation {}", moon_elongation(new) / DEG);
        assert!(moon_illuminated_fraction(new) < 0.02);
    }

    #[test]
    fn moon_state_reports_brightness_only_while_the_moon_is_up() {
        let (lat, lon) = (67.84, 20.41);
        let start = 1789077600.0;
        let mut saw_up = false;
        let mut saw_down = false;
        let mut brightest = f64::NEG_INFINITY;
        for step in 0..(24 * 30) {
            let unix = start + step as f64 * 3600.0;
            let m = moon_state(unix, lat, lon);
            assert!((0.0..360.0).contains(&m.azimuth_deg));
            assert!(m.elevation_deg > -95.0 && m.elevation_deg < 95.0);
            assert!((0.0..=1.0).contains(&m.illuminated_fraction));
            assert!((0.0..=180.0).contains(&m.phase_angle_deg));
            // Brightness is zero while the Moon is below the horizon, and never
            // negative, so it can be used directly as a weighting input.
            if m.elevation_deg <= 0.0 {
                assert_eq!(m.sky_brightness, 0.0, "the Moon is down");
                saw_down = true;
            } else {
                assert!(m.sky_brightness >= 0.0 && m.sky_brightness <= 1.0);
                saw_up = true;
            }
            // Full Moon is about magnitude -12.7, new Moon far fainter.
            assert!(m.apparent_magnitude > -13.5 && m.apparent_magnitude < 0.0);
            brightest = brightest.max(-m.apparent_magnitude);
        }
        assert!(saw_up && saw_down, "the Moon should rise and set over a month");
        assert!(brightest > 12.0, "a full Moon should reach about magnitude -12.7");
    }

    #[test]
    fn star_keys_are_stable_and_distinct() {
        assert_eq!(star_key(6.7524, -16.7161), star_key(6.75240, -16.71610));
        assert_ne!(star_key(6.7524, -16.7161), star_key(6.7524, 16.7161));
        assert!(star_key(2.5301944, 89.2641111).contains('+'));
        assert!(star_key(6.7524, -16.7161).contains('-'));
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
