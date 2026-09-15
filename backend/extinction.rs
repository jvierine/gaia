//! The clear-sky reference: one extinction coefficient per camera per night,
//! with a zero point per star.
//!
//! A star's flux falls with air mass even in a perfectly clear sky, so a
//! reference taken over a whole night mixes elevations and reads the difference
//! as cloud. The fix is to model the clear sky explicitly,
//!
//! ```text
//!     ln F = ln F0(star) - beta k X,      beta = 0.4 ln 10,
//! ```
//!
//! with one zero point per star and a single coefficient `k` shared by every
//! star the camera saw that night. Sharing `k` is what makes the fit well posed:
//! one star rarely spans enough air mass to separate its own brightness from the
//! atmosphere, but a dozen stars between them always do.
//!
//! The difficulty is that cloud only ever subtracts. An ordinary least-squares
//! fit is pulled down by every attenuated sample and returns an extinction that
//! is really extinction plus average weather; on this archive it gives
//! 0.38 mag per air mass where clear sky is nearer 0.20. The clear sky is the
//! *upper envelope* of the samples, not their middle, so the fit here is a
//! quantile regression at `QUANTILE`. That choice has a convenient consequence:
//! for a fixed `k` the zero point that minimises the pinball loss is exactly the
//! quantile of `ln F + beta k X` for that star, so every zero point profiles out
//! analytically and what remains is a one-dimensional convex function of `k`.

use crate::starphot;
use anyhow::Result;
use std::collections::BTreeMap;

/// Magnitudes to natural log: a magnitude is `-2.5 log10`, so `k` magnitudes per
/// air mass is `0.4 ln(10) k` nepers per air mass.
pub const BETA: f64 = 0.4 * std::f64::consts::LN_10;

/// The envelope this fit follows. High enough to sit above ordinary noise and
/// thin haze, low enough that a single unusually bright sample cannot define the
/// clear sky by itself.
pub const QUANTILE: f64 = 0.90;

/// Below this the fit is refused rather than guessed.
pub const MIN_STARS: usize = 3;
pub const MIN_SAMPLES_PER_STAR: usize = 5;
pub const MIN_SAMPLES: usize = 20;
/// `k` is the slope against air mass, so it is only identifiable if the night
/// actually spans a range of air mass. Zenith to 40 degrees elevation is 0.55.
pub const MIN_AIRMASS_SPAN: f64 = 0.40;
/// The span that matters is the one *each star* sweeps, not the spread across
/// stars. Every star carries its own free zero point, so a difference between
/// two stars at different elevations is absorbed entirely by their zero points
/// and says nothing about the atmosphere. Only a star that itself climbs or
/// descends measures extinction. Stars that sit still are dropped: on this
/// archive they are the majority, they dominate the objective by weight of
/// numbers, and including them leaves `k` unidentified and free to run
/// negative.
pub const MIN_STAR_AIRMASS_SPAN: f64 = 0.30;
/// Stars that actually sweep, needed before a night is fitted at all.
pub const MIN_MOVING_STARS: usize = 3;
/// Extinction cannot be negative. A fit that wants it to be is not measuring
/// the atmosphere, it is measuring a systematic, and is refused.
pub const K_SCAN_LOW: f64 = -0.30;
pub const K_SCAN_HIGH: f64 = 1.50;

/// How much the pinball loss per sample must worsen when `k` is moved by
/// 0.3 mag per air mass. Below this the night does not pin the coefficient and
/// no fit is reported.
pub const MIN_CURVATURE: f64 = 0.002;

/// One measured star brightness, as the fit sees it.
#[derive(Debug, Clone)]
pub struct Sample {
    pub star_key: String,
    pub elevation_deg: f64,
    pub flux: f64,
}

/// The clear sky over one camera on one night, in one colour channel.
#[derive(Debug, Clone)]
pub struct NightFit {
    /// Extinction coefficient, magnitudes per air mass.
    pub k_mag_per_airmass: f64,
    /// `ln F0` per star: the star's flux outside the atmosphere, in this
    /// camera's own units. There is no photometric zero point available, so
    /// these are instrumental and only comparable within a camera and night.
    pub zero_points: BTreeMap<String, f64>,
    pub stars: usize,
    pub samples: usize,
    /// Air mass range the fit actually saw. A narrow range means a soft `k`.
    pub airmass_span: f64,
    /// Scatter of the samples that sit on the envelope, in natural log units.
    /// This is the noise floor of any optical depth derived from the fit.
    pub envelope_scatter: f64,
    /// How much worse the fit is with `k` moved by 0.3 mag per air mass. Small
    /// values mean the night does not really constrain `k`.
    pub curvature: f64,
}

/// Air mass at a given elevation, Kasten and Young (1989). The naive
/// `1/sin(h)` is 3 percent high at 10 degrees and diverges at the horizon;
/// this stays usable down to the horizon, which matters because the samples
/// that constrain `k` are the low ones.
pub fn airmass(elevation_deg: f64) -> f64 {
    let h = elevation_deg.max(-2.0);
    let denominator = (h.to_radians()).sin() + 0.50572 * (h + 6.07995).powf(-1.6364);
    if denominator <= 0.0 { f64::INFINITY } else { 1.0 / denominator }
}

/// The observing night a moment belongs to, as a day number in local solar
/// time offset by twelve hours, so one period of darkness falls in one bucket
/// instead of being split at midnight UTC.
pub fn night_index(unix_seconds: f64, longitude_deg: f64) -> i64 {
    let local = unix_seconds + longitude_deg / 15.0 * 3600.0;
    ((local - 43200.0) / 86400.0).floor() as i64
}

/// Pinball loss at `QUANTILE`: residuals above the line cost `q`, below it
/// `1-q`. With `q` at 0.9 a sample that is too bright is nine times as
/// expensive as one that is too faint, which is what pins the line to the
/// clear-sky envelope rather than the middle of the weather.
fn pinball(residual: f64) -> f64 {
    if residual >= 0.0 { QUANTILE * residual } else { (QUANTILE - 1.0) * residual }
}

/// Zero points and total loss for a fixed `k`.
fn profile(by_star: &[(String, Vec<(f64, f64)>)], k: f64) -> (BTreeMap<String, f64>, f64) {
    let mut zero_points = BTreeMap::new();
    let mut loss = 0.0;
    for (star, points) in by_star {
        // Corrected to the top of the atmosphere at this trial k.
        let corrected: Vec<f64> = points.iter().map(|&(x, ln_f)| ln_f + BETA * k * x).collect();
        let Some(zero) = starphot::percentile(&corrected, QUANTILE) else {
            continue;
        };
        for value in &corrected {
            loss += pinball(value - zero);
        }
        zero_points.insert(star.clone(), zero);
    }
    (zero_points, loss)
}

/// Fits one camera-night-channel. Returns `None` when the night cannot support
/// a fit, which is a normal outcome and not an error: a camera that saw three
/// stars near the zenith for twenty minutes has no information about extinction
/// and must not be given a fabricated coefficient.
pub fn fit_night(samples: &[Sample]) -> Option<NightFit> {
    let mut grouped: BTreeMap<String, Vec<(f64, f64)>> = BTreeMap::new();
    for sample in samples {
        if !(sample.flux > 0.0) || !sample.elevation_deg.is_finite() {
            continue;
        }
        let x = airmass(sample.elevation_deg);
        if !x.is_finite() || x > 12.0 {
            continue;
        }
        grouped
            .entry(sample.star_key.clone())
            .or_default()
            .push((x, sample.flux.ln()));
    }
    grouped.retain(|_, points| points.len() >= MIN_SAMPLES_PER_STAR);
    // Keep only the stars that move through air mass, for the reason given at
    // MIN_STAR_AIRMASS_SPAN.
    grouped.retain(|_, points| {
        let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
        for &(x, _) in points.iter() {
            low = low.min(x);
            high = high.max(x);
        }
        high - low >= MIN_STAR_AIRMASS_SPAN
    });
    if grouped.len() < MIN_STARS || grouped.len() < MIN_MOVING_STARS {
        return None;
    }
    let by_star: Vec<(String, Vec<(f64, f64)>)> = grouped.into_iter().collect();
    let total: usize = by_star.iter().map(|(_, p)| p.len()).sum();
    if total < MIN_SAMPLES {
        return None;
    }
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    for (_, points) in &by_star {
        for &(x, _) in points {
            low = low.min(x);
            high = high.max(x);
        }
    }
    let airmass_span = high - low;
    if airmass_span < MIN_AIRMASS_SPAN {
        return None;
    }

    // The profiled objective is convex in k, so a coarse scan followed by a
    // refinement finds the global minimum without any starting guess.
    let mut best = (f64::INFINITY, 0.0);
    let mut scan = |from: f64, to: f64, step: f64, best: &mut (f64, f64)| {
        let mut k = from;
        while k <= to {
            let (_, loss) = profile(&by_star, k);
            if loss < best.0 {
                *best = (loss, k);
            }
            k += step;
        }
    };
    scan(K_SCAN_LOW, K_SCAN_HIGH, 0.01, &mut best);
    let coarse = best.1;
    scan(coarse - 0.01, coarse + 0.01, 0.0005, &mut best);
    let k = best.1;
    // A solution sitting on the edge of the scan is not a minimum, and a
    // negative coefficient is not extinction. Both mean this night does not
    // constrain the atmosphere, and a reference built on either would be
    // fiction.
    if k <= K_SCAN_LOW + 0.02 || k >= K_SCAN_HIGH - 0.02 || k < 0.0 {
        return None;
    }

    let (zero_points, loss) = profile(&by_star, k);
    // Is k actually pinned? Compare with a clearly different coefficient.
    let curvature = (profile(&by_star, k + 0.3).1 - loss).min(profile(&by_star, k - 0.3).1 - loss)
        / total as f64;
    // The objective has to rise appreciably on both sides, or k is floating.
    if !(curvature > MIN_CURVATURE) {
        return None;
    }

    // Scatter of the samples sitting on the envelope, which is what a derived
    // optical depth has to stand above.
    let mut envelope = Vec::new();
    for (star, points) in &by_star {
        let Some(&zero) = zero_points.get(star) else { continue };
        for &(x, ln_f) in points {
            let residual = ln_f + BETA * k * x - zero;
            if residual > -0.15 {
                envelope.push(residual);
            }
        }
    }
    let mean = envelope.iter().sum::<f64>() / envelope.len().max(1) as f64;
    let envelope_scatter = if envelope.len() > 2 {
        (envelope.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (envelope.len() - 1) as f64)
            .sqrt()
    } else {
        f64::NAN
    };

    Some(NightFit {
        k_mag_per_airmass: k,
        stars: zero_points.len(),
        zero_points,
        samples: total,
        airmass_span,
        envelope_scatter,
        curvature,
    })
}

impl NightFit {
    /// The clear-sky flux this star would have shown at this elevation, had the
    /// sky been clear. This is what `starphot::optical_depth` needs as its
    /// reference: it now varies with elevation instead of being one number for
    /// the night, which was the whole defect.
    pub fn clear_flux(&self, star_key: &str, elevation_deg: f64) -> Option<f64> {
        let zero = *self.zero_points.get(star_key)?;
        let x = airmass(elevation_deg);
        if !x.is_finite() {
            return None;
        }
        Some((zero - BETA * self.k_mag_per_airmass * x).exp())
    }

    /// Vertical cloud optical depth for one measurement, over and above the
    /// clear-sky extinction already accounted for by `k`.
    pub fn optical_depth(&self, star_key: &str, elevation_deg: f64, flux: f64) -> Option<f64> {
        let clear = self.clear_flux(star_key, elevation_deg)?;
        let zenith = (90.0 - elevation_deg).to_radians();
        starphot::optical_depth(flux, clear, zenith)
    }
}

/// A measurement has to clear this in both signal-to-noise ratios before it is
/// allowed to become an optical depth. A fit to noise returns a small positive
/// flux, and read against a clear-sky reference that is a thin cloud; refusing
/// it is the difference between an overcast sky reading as overcast and reading
/// as hazy.
pub const MIN_DEPTH_SNR: f64 = 5.0;

/// Writes the optical depth of every measurement this fit covers, and clears it
/// from those it does not.
///
/// Three kinds of measurement are deliberately left null rather than given a
/// number. A star that was looked for and not found has no flux. One whose
/// peak is clipped reports a flux that is an underestimate, and bright aurora
/// makes that common. One indistinguishable from the noise is not a
/// measurement. In each case the honest answer is no depth, which downstream
/// becomes no weight rather than a guessed one.
///
/// Every row in the window is cleared first, so a refit that newly refuses a
/// star does not leave its old depth behind to be read as current.
pub fn record_depths(
    conn: &rusqlite::Connection,
    source_id: &str,
    channel: &str,
    from_utc: &str,
    to_utc: &str,
    fit: &NightFit,
) -> Result<usize> {
    conn.execute(
        "UPDATE star_photometry SET optical_depth=NULL
         WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 AND observation_utc<?4",
        rusqlite::params![source_id, channel, from_utc, to_utc],
    )?;
    let mut rows = conn.prepare(
        "SELECT rowid,star_key,elevation_deg,flux FROM star_photometry
         WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 AND observation_utc<?4
           AND amplitude IS NOT NULL AND flux IS NOT NULL AND flux>0
           AND amplitude_snr >= ?5 AND flux_snr >= ?5
           AND background IS NOT NULL AND background + amplitude < ?6",
    )?;
    let found: Vec<(i64, String, f64, f64)> = rows
        .query_map(
            rusqlite::params![
                source_id,
                channel,
                from_utc,
                to_utc,
                MIN_DEPTH_SNR,
                crate::starphot::SATURATION_LEVEL
            ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let mut set = conn.prepare("UPDATE star_photometry SET optical_depth=?2 WHERE rowid=?1")?;
    let mut written = 0usize;
    for (rowid, star_key, elevation, flux) in found {
        // A star with no zero point in this fit was dropped by the guards, so
        // the fit says nothing about it and it keeps no depth.
        let Some(depth) = fit.optical_depth(&star_key, elevation, flux) else {
            continue;
        };
        set.execute(rusqlite::params![rowid, depth])?;
        written += 1;
    }
    Ok(written)
}

/// Persists a fit. The zero points go with it: without them the coefficient
/// cannot be turned back into a reference flux.
pub fn record(
    conn: &rusqlite::Connection,
    source_id: &str,
    night: i64,
    channel: &str,
    fit: &NightFit,
) -> Result<()> {
    conn.execute(
        "INSERT INTO extinction_nights(source_id,night,channel,k_mag_per_airmass,stars,samples,
            airmass_span,envelope_scatter,curvature,fitted_utc)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(source_id,night,channel) DO UPDATE SET
            k_mag_per_airmass=excluded.k_mag_per_airmass,stars=excluded.stars,
            samples=excluded.samples,airmass_span=excluded.airmass_span,
            envelope_scatter=excluded.envelope_scatter,curvature=excluded.curvature,
            fitted_utc=excluded.fitted_utc",
        rusqlite::params![
            source_id,
            night,
            channel,
            fit.k_mag_per_airmass,
            fit.stars as i64,
            fit.samples as i64,
            fit.airmass_span,
            if fit.envelope_scatter.is_finite() { Some(fit.envelope_scatter) } else { None },
            fit.curvature,
            chrono::Utc::now().to_rfc3339(),
        ],
    )?;
    let mut statement = conn.prepare(
        "INSERT INTO star_zero_points(source_id,night,channel,star_key,ln_flux_zero)
         VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(source_id,night,channel,star_key) DO UPDATE SET
            ln_flux_zero=excluded.ln_flux_zero",
    )?;
    for (star, zero) in &fit.zero_points {
        statement.execute(rusqlite::params![source_id, night, channel, star, zero])?;
    }
    Ok(())
}

/// Reads a stored fit back.
pub fn load(
    conn: &rusqlite::Connection,
    source_id: &str,
    night: i64,
    channel: &str,
) -> Result<Option<NightFit>> {
    let row = conn
        .query_row(
            "SELECT k_mag_per_airmass,stars,samples,airmass_span,envelope_scatter,curvature
             FROM extinction_nights WHERE source_id=?1 AND night=?2 AND channel=?3",
            rusqlite::params![source_id, night, channel],
            |r| {
                Ok((
                    r.get::<_, f64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, Option<f64>>(4)?,
                    r.get::<_, f64>(5)?,
                ))
            },
        )
        .ok();
    let Some((k, stars, samples, span, scatter, curvature)) = row else {
        return Ok(None);
    };
    let mut q = conn.prepare(
        "SELECT star_key,ln_flux_zero FROM star_zero_points
         WHERE source_id=?1 AND night=?2 AND channel=?3",
    )?;
    let rows = q.query_map(rusqlite::params![source_id, night, channel], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
    })?;
    let mut zero_points = BTreeMap::new();
    for row in rows {
        let (star, zero) = row?;
        zero_points.insert(star, zero);
    }
    Ok(Some(NightFit {
        k_mag_per_airmass: k,
        zero_points,
        stars: stars as usize,
        samples: samples as usize,
        airmass_span: span,
        envelope_scatter: scatter.unwrap_or(f64::NAN),
        curvature,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic noise, so a test can never flake.
    struct Noise(u64);
    impl Noise {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 33) as f64 / (1u64 << 31) as f64) - 0.5
        }
    }

    /// A synthetic night: `stars` stars sweeping a range of elevation, with a
    /// known extinction, optional cloud on a fraction of the samples, and a
    /// little measurement noise.
    fn synthetic_night(
        stars: usize,
        per_star: usize,
        k_true: f64,
        cloud_fraction: f64,
        noise_scale: f64,
        seed: u64,
    ) -> (Vec<Sample>, Vec<f64>, Vec<f64>) {
        synthetic_night_with_trend(stars, per_star, k_true, cloud_fraction, 0.0, noise_scale, seed)
    }

    /// `cloud_trend` makes cloud steadily *more likely* as the night goes on.
    /// Stars descend as it does, so this is the realistic confound: weather
    /// correlated with air mass, which is what biases an ordinary regression.
    /// It raises the probability, not the amount, so clear moments survive at
    /// every air mass --- which is the condition under which the clear sky is
    /// recoverable at all. A cloud that only ever thickens leaves no clear sky
    /// at high air mass and no estimator can invent one.
    #[allow(clippy::too_many_arguments)]
    fn synthetic_night_with_trend(
        stars: usize,
        per_star: usize,
        k_true: f64,
        cloud_fraction: f64,
        cloud_trend: f64,
        noise_scale: f64,
        seed: u64,
    ) -> (Vec<Sample>, Vec<f64>, Vec<f64>) {
        let mut rng = Noise(seed);
        let mut samples = Vec::new();
        let mut zero_points = Vec::new();
        let mut planted_cloud = Vec::new();
        for star in 0..stars {
            // Instrumental zero points spread over a factor of a hundred.
            let ln_f0 = 8.0 + star as f64 * 0.5;
            zero_points.push(ln_f0);
            for step in 0..per_star {
                // Each star sweeps from high to low, different stars differently,
                // so together they span air mass well.
                let elevation = 75.0 - (step as f64 / per_star as f64) * 55.0
                    + (star as f64 * 3.0) % 11.0;
                let x = airmass(elevation);
                let progress = step as f64 / per_star as f64;
                let probability = (cloud_fraction + cloud_trend * progress).min(0.85);
                let cloud = if (rng.next() + 0.5) < probability {
                    // Cloud takes between a little and a lot.
                    (rng.next() + 0.5) * 1.5
                } else {
                    0.0
                };
                planted_cloud.push(cloud);
                let ln_f = ln_f0 - BETA * k_true * x - cloud * x + rng.next() * noise_scale;
                samples.push(Sample {
                    star_key: format!("star-{star}"),
                    elevation_deg: elevation,
                    flux: ln_f.exp(),
                });
            }
        }
        (samples, zero_points, planted_cloud)
    }

    #[test]
    fn air_mass_matches_the_standard_values() {
        // Kasten-Young against its own published behaviour.
        assert!((airmass(90.0) - 1.0).abs() < 1e-3, "{}", airmass(90.0));
        assert!((airmass(30.0) - 1.995).abs() < 0.01, "{}", airmass(30.0));
        assert!((airmass(10.0) - 5.60).abs() < 0.05, "{}", airmass(10.0));
        // The naive secant diverges at the horizon; this must not.
        assert!(airmass(0.0) < 40.0 && airmass(0.0) > 30.0, "{}", airmass(0.0));
        // Monotone, which the estimator relies on.
        for el in 1..90 {
            assert!(airmass(el as f64) < airmass(el as f64 - 1.0));
        }
    }

    #[test]
    fn the_night_runs_from_local_noon_to_local_noon() {
        // A night at 20 E: 22:00 and 02:00 UTC either side of midnight belong
        // to the same night, and the following noon starts a new one.
        let lon = 20.0;
        let evening = 1789077600.0; // 2026-09-10T22:00:00Z
        let after_midnight = evening + 4.0 * 3600.0;
        let next_evening = evening + 24.0 * 3600.0;
        assert_eq!(night_index(evening, lon), night_index(after_midnight, lon));
        assert_ne!(night_index(evening, lon), night_index(next_evening, lon));
        // The boundary is local noon, not midnight.
        let noon_utc = 1789034400.0; // 2026-09-10T10:00:00Z, which is noon at 30 E
        assert_ne!(night_index(noon_utc - 600.0, 30.0), night_index(noon_utc + 600.0, 30.0));
        // Longitude matters: at an hour that straddles the boundary, the same
        // instant belongs to different nights on opposite sides of the globe.
        let morning = evening - 16.0 * 3600.0; // 06:00 UTC
        assert_ne!(night_index(morning, 0.0), night_index(morning, 179.0));
    }

    #[test]
    fn a_clear_night_recovers_the_planted_extinction() {
        let (samples, planted, _) = synthetic_night(8, 30, 0.25, 0.0, 0.02, 11);
        let fit = fit_night(&samples).expect("fit");
        assert!(
            (fit.k_mag_per_airmass - 0.25).abs() < 0.03,
            "k {} vs planted 0.25",
            fit.k_mag_per_airmass
        );
        assert_eq!(fit.stars, 8);
        // The zero points come back too, up to the quantile sitting slightly
        // above the true value because it tracks the top of the noise.
        for (index, truth) in planted.iter().enumerate() {
            let got = fit.zero_points[&format!("star-{index}")];
            assert!((got - truth).abs() < 0.05, "star {index}: {got} vs {truth}");
        }
    }

    #[test]
    fn cloud_does_not_inflate_the_coefficient() {
        // This is the reason the fit is a quantile regression. Half the samples
        // are attenuated by up to 1.5 optical depths; the clear-sky coefficient
        // must be unmoved, because cloud is not extinction.
        let (samples, _, _) = synthetic_night(10, 40, 0.22, 0.5, 0.02, 7);
        let fit = fit_night(&samples).expect("fit");
        assert!(
            (fit.k_mag_per_airmass - 0.22).abs() < 0.05,
            "quantile fit gave k {} under heavy cloud",
            fit.k_mag_per_airmass
        );

        // Random cloud does not bias a least-squares slope, but it does drag
        // the reference level down, and the reference is what an optical depth
        // is measured against. The envelope must sit at the clear sky, not in
        // the middle of the weather.
        let mean_level = {
            let points: Vec<f64> = samples
                .iter()
                .filter(|s| s.star_key == "star-0")
                .map(|s| s.flux.ln() + BETA * fit.k_mag_per_airmass * airmass(s.elevation_deg))
                .collect();
            points.iter().sum::<f64>() / points.len() as f64
        };
        let envelope_level = fit.zero_points["star-0"];
        assert!(
            envelope_level > mean_level + 0.2,
            "the envelope {envelope_level} must stand above the cloudy mean {mean_level}"
        );
    }

    #[test]
    fn weather_correlated_with_air_mass_does_not_become_extinction() {
        // The real confound. Stars descend through the night while the cloud
        // thickens, so an ordinary regression cannot tell the atmosphere from
        // the weather and charges the difference to k. On this archive that is
        // what turns a clear-sky 0.20 into a measured 0.38.
        // Ten per cent cloud at the zenith rising to eighty near the horizon.
        let (samples, _, _) = synthetic_night_with_trend(10, 40, 0.20, 0.10, 0.70, 0.02, 23);
        let fit = fit_night(&samples).expect("fit");

        let naive_k = {
            let (mut sx, mut sy, mut sxx, mut sxy, mut n) = (0.0, 0.0, 0.0, 0.0, 0.0);
            for sample in samples.iter().filter(|s| s.star_key == "star-0") {
                let x = airmass(sample.elevation_deg);
                let y = sample.flux.ln();
                sx += x;
                sy += y;
                sxx += x * x;
                sxy += x * y;
                n += 1.0;
            }
            -((n * sxy - sx * sy) / (n * sxx - sx * sx)) / BETA
        };
        assert!(
            naive_k > 0.35,
            "the naive fit should have absorbed the weather, got {naive_k}"
        );
        assert!(
            (fit.k_mag_per_airmass - 0.20).abs() < 0.06,
            "the quantile fit must stay near the planted 0.20, got {}",
            fit.k_mag_per_airmass
        );
        assert!(
            fit.k_mag_per_airmass < naive_k - 0.1,
            "quantile {} should be well below naive {naive_k}",
            fit.k_mag_per_airmass
        );
    }

    #[test]
    fn optical_depth_is_zero_in_the_clear_and_measures_the_cloud_otherwise() {
        let (samples, _, planted) = synthetic_night(8, 30, 0.25, 0.35, 0.01, 3);
        let fit = fit_night(&samples).expect("fit");
        let mut clear_depths = Vec::new();
        let mut cloudy_errors = Vec::new();
        for (sample, cloud) in samples.iter().zip(&planted) {
            let tau = fit
                .optical_depth(&sample.star_key, sample.elevation_deg, sample.flux)
                .expect("depth");
            if *cloud == 0.0 {
                clear_depths.push(tau);
            } else if *cloud > 0.3 {
                // The planted cloud is a slant depth; the estimator reports the
                // vertical one, so the planted value converts the same way.
                cloudy_errors.push((tau - cloud).abs());
            }
        }
        let mean_clear = clear_depths.iter().sum::<f64>() / clear_depths.len() as f64;
        assert!(mean_clear < 0.06, "clear sky read as tau {mean_clear}");
        let mean_error = cloudy_errors.iter().sum::<f64>() / cloudy_errors.len() as f64;
        assert!(mean_error < 0.10, "cloud depth off by {mean_error} on average");
    }

    #[test]
    fn a_night_that_cannot_constrain_k_is_refused() {
        // Every star parked near the zenith: no air mass range, no information.
        let mut samples = Vec::new();
        for star in 0..6 {
            for step in 0..10 {
                samples.push(Sample {
                    star_key: format!("star-{star}"),
                    elevation_deg: 84.0 + (step % 3) as f64,
                    flux: (9.0 + star as f64 * 0.3_f64).exp(),
                });
            }
        }
        assert!(fit_night(&samples).is_none(), "a zenith-only night must not yield a k");

        // Too few stars.
        let (few, _, _) = synthetic_night(2, 30, 0.25, 0.0, 0.02, 5);
        assert!(fit_night(&few).is_none(), "two stars must not yield a k");

        // Too few samples per star: each star is dropped, so the night is empty.
        let (sparse, _, _) = synthetic_night(8, 3, 0.25, 0.0, 0.02, 5);
        assert!(fit_night(&sparse).is_none(), "three samples a star must not yield a k");
    }

    #[test]
    fn stars_spread_over_the_sky_but_standing_still_do_not_identify_k() {
        // The failure this guard exists for, taken from the real archive. A
        // camera sees 148 stars spanning air mass 1.1 to 4.6 across the field,
        // which looks like ample leverage, but each star barely moves during
        // the night. Every star has a free zero point, so a difference between
        // two stars is absorbed there and says nothing about the atmosphere.
        // Fitted anyway, k is unpinned and wanders negative.
        let mut samples = Vec::new();
        for star in 0..40 {
            // Fixed elevation per star, spread widely; only a few degrees of
            // drift each, as a nearly circumpolar star would show.
            let base = 15.0 + star as f64 * 1.8;
            for step in 0..12 {
                let elevation = base + (step as f64 * 0.15);
                let x = airmass(elevation);
                samples.push(Sample {
                    star_key: format!("star-{star}"),
                    elevation_deg: elevation,
                    // Brightness set by the star, not by the atmosphere.
                    flux: (9.0 + (star % 7) as f64 * 0.4 - BETA * 0.2 * x).exp(),
                });
            }
        }
        let span = {
            let xs: Vec<f64> = samples.iter().map(|s| airmass(s.elevation_deg)).collect();
            xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min)
        };
        assert!(span > 2.5, "the test needs a wide apparent span, got {span}");
        assert!(
            fit_night(&samples).is_none(),
            "a night whose stars do not move must be refused however many there are"
        );

        // The same stars, now actually sweeping, are accepted.
        let (moving, _, _) = synthetic_night(8, 20, 0.20, 0.0, 0.02, 31);
        assert!(fit_night(&moving).is_some());
    }

    #[test]
    fn the_fit_reports_how_well_the_night_pins_k() {
        let (wide, _, _) = synthetic_night(8, 30, 0.25, 0.0, 0.02, 13);
        let fit = fit_night(&wide).expect("fit");
        assert!(fit.airmass_span > 1.0, "span {}", fit.airmass_span);
        assert!(fit.curvature > 0.0, "curvature {}", fit.curvature);
        assert!(fit.envelope_scatter < 0.05, "envelope scatter {}", fit.envelope_scatter);
    }

    /// Plants one night of one star and returns the connection, so a test can
    /// ask what became of each measurement.
    fn night_in_a_table(conn: &rusqlite::Connection) {
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        let cols = "source_id,image_id,observation_utc,star_key,channel,ra_hours_j2000,\
            dec_deg_j2000,vt_mag,azimuth_deg,elevation_deg,predicted_x,predicted_y,\
            background,amplitude,flux,amplitude_snr,flux_snr";
        let mut insert = conn
            .prepare(&format!(
                "INSERT INTO star_photometry({cols}) VALUES(?1,?2,?3,?4,'mean',1.0,45.0,2.0,\
                 120.0,?5,100.0,100.0,?6,?7,?8,?9,?10)"
            ))
            .unwrap();
        // Four stars sweeping air mass, clear, so the fit is well determined.
        for star in 0..4 {
            for step in 0..12 {
                let elevation = 70.0 - step as f64 * 4.5 - star as f64 * 2.0;
                let x = airmass(elevation);
                let flux = (9.0 + star as f64 * 0.3 - BETA * 0.20 * x).exp();
                insert
                    .execute(rusqlite::params![
                        "cam",
                        format!("img-{step}"),
                        format!("2026-09-14T{:02}:00:00+00:00", 18 + step / 3),
                        format!("star-{star}"),
                        elevation,
                        50.0,
                        120.0,
                        flux,
                        40.0,
                        40.0
                    ])
                    .unwrap();
            }
        }
        // One saturated: background plus peak fills the range.
        insert
            .execute(rusqlite::params![
                "cam", "img-sat", "2026-09-14T19:30:00+00:00", "star-0", 60.0,
                200.0_f64, 60.0_f64, 5000.0_f64, 40.0_f64, 40.0_f64
            ])
            .unwrap();
        // One in the noise: detected, but neither ratio clears the gate.
        insert
            .execute(rusqlite::params![
                "cam", "img-noise", "2026-09-14T19:31:00+00:00", "star-0", 60.0,
                50.0_f64, 3.0_f64, 90.0_f64, 1.2_f64, 1.5_f64
            ])
            .unwrap();
        // One never found: no flux at all.
        conn.execute(
            "INSERT INTO star_photometry(source_id,image_id,observation_utc,star_key,channel,\
                ra_hours_j2000,dec_deg_j2000,vt_mag,azimuth_deg,elevation_deg,predicted_x,predicted_y)\
             VALUES('cam','img-miss','2026-09-14T19:32:00+00:00','star-0','mean',1.0,45.0,2.0,120.0,60.0,100.0,100.0)",
            [],
        )
        .unwrap();
    }

    #[test]
    fn a_depth_is_stored_only_where_one_can_honestly_be_claimed() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        night_in_a_table(&conn);
        let mut samples = Vec::new();
        let mut q = conn
            .prepare(
                "SELECT star_key,elevation_deg,flux FROM star_photometry \
                 WHERE amplitude IS NOT NULL AND flux_snr>=5",
            )
            .unwrap();
        for row in q
            .query_map([], |r| {
                Ok(Sample { star_key: r.get(0)?, elevation_deg: r.get(1)?, flux: r.get(2)? })
            })
            .unwrap()
        {
            samples.push(row.unwrap());
        }
        let fit = fit_night(&samples).expect("the planted night must fit");
        let written = record_depths(
            &conn, "cam", "mean", "2026-09-14T00:00:00+00:00", "2026-09-15T00:00:00+00:00", &fit,
        )
        .unwrap();
        assert_eq!(written, 48, "every clear measurement should get a depth");

        let depth = |image: &str| -> Option<f64> {
            conn.query_row(
                "SELECT optical_depth FROM star_photometry WHERE image_id=?1 LIMIT 1",
                [image],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert!(depth("img-0").is_some(), "a clear measurement must carry a depth");
        assert!(depth("img-sat").is_none(), "a clipped peak must not become a fading");
        assert!(depth("img-noise").is_none(), "noise must not become thin cloud");
        assert!(depth("img-miss").is_none(), "a star never found has no depth");

        // The planted sky is clear, so the depths sit at zero rather than
        // inventing cloud out of the fit's own scatter.
        let worst: f64 = conn
            .query_row("SELECT max(optical_depth) FROM star_photometry", [], |r| r.get(0))
            .unwrap();
        assert!(worst < 0.05, "clear sky came out as tau {worst}");
    }

    #[test]
    fn a_refit_does_not_leave_a_stale_depth_behind() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        night_in_a_table(&conn);
        conn.execute("UPDATE star_photometry SET optical_depth=9.9", []).unwrap();
        let mut samples = Vec::new();
        let mut q = conn
            .prepare("SELECT star_key,elevation_deg,flux FROM star_photometry WHERE flux_snr>=5")
            .unwrap();
        for row in q
            .query_map([], |r| {
                Ok(Sample { star_key: r.get(0)?, elevation_deg: r.get(1)?, flux: r.get(2)? })
            })
            .unwrap()
        {
            samples.push(row.unwrap());
        }
        let fit = fit_night(&samples).expect("fit");
        record_depths(
            &conn, "cam", "mean", "2026-09-14T00:00:00+00:00", "2026-09-15T00:00:00+00:00", &fit,
        )
        .unwrap();
        let stale: i64 = conn
            .query_row(
                "SELECT count(*) FROM star_photometry WHERE optical_depth=9.9",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stale, 0, "an old depth must not survive a refit that refuses the row");
    }

    #[test]
    fn a_stored_fit_comes_back_unchanged() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        // These tables reference sources; this test exercises them alone.
        conn.pragma_update(None, "foreign_keys", "OFF").unwrap();
        let (samples, _, _) = synthetic_night(6, 20, 0.31, 0.2, 0.02, 19);
        let fit = fit_night(&samples).expect("fit");
        record(&conn, "cam", 20342, "mean", &fit).unwrap();
        let back = load(&conn, "cam", 20342, "mean").unwrap().expect("stored fit");
        assert!((back.k_mag_per_airmass - fit.k_mag_per_airmass).abs() < 1e-12);
        assert_eq!(back.zero_points, fit.zero_points);
        assert_eq!(back.stars, fit.stars);
        // And a rewrite replaces rather than duplicates.
        record(&conn, "cam", 20342, "mean", &fit).unwrap();
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM extinction_nights", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }
}
