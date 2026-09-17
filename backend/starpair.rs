//! Camera sensitivity from the same star seen by two cameras.
//!
//! A star is a point source of known constancy and, crucially, of *unknown*
//! brightness that cancels. Two cameras that both measure the same star at the
//! same moment record
//!
//! ```text
//!   F_A = G_A . F0 . exp(-beta k_A X_A) . exp(-tau_A)
//!   F_B = G_B . F0 . exp(-beta k_B X_B) . exp(-tau_B)
//! ```
//!
//! so that
//!
//! ```text
//!   ln(F_A/F_B) = ln(G_A/G_B) - beta (k_A X_A - k_B X_B) - (tau_A - tau_B).
//! ```
//!
//! The star's own flux `F0` disappears exactly. No catalogue magnitude, no
//! photometric zero point and no absolute calibration is needed: what is left
//! is the ratio of the two cameras' sensitivities, an air-mass term each camera
//! already measures for itself in `extinction.rs`, and a cloud term that is
//! zero when both views are clear.
//!
//! This is a different instrument from the paired keograms. A keogram compares
//! extended auroral surfaces and yields both a gain ratio and a background
//! offset, at the cost of assuming the two cameras see the same sky. Stars are
//! point sources at known positions, so the comparison needs no such
//! assumption and the star's brightness cancels -- but a star says nothing
//! about the background, so this measures gain alone. The two are complementary
//! and should eventually be solved together.
//!
//! **Where this is still weak.** The cloud term is the whole difficulty. A thin
//! cloud over one station and not the other biases the ratio directly, and the
//! archive cannot yet tell a saturated star under aurora from one under moonlit
//! cloud (see `cloudweight::PRELIMINARY`). The gates below are therefore
//! conservative, and the estimator is a median rather than a mean so that a
//! minority of clouded pairs moves the answer very little.

use crate::extinction::{BETA, airmass, night_index};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashMap;

/// How far apart two measurements may be and still be compared.
///
/// Not the 30 s the paired keograms use. There the limit is real: auroral
/// structures move at about a kilometre a second, so 30 s is 30 km of smear and
/// the two views stop describing the same sky. A star does not move, is
/// constant, and carries its own elevation into the air-mass correction, so
/// none of that applies. Over five minutes a star at 30 degrees rises about a
/// degree, which changes its air mass by under a twentieth -- and that change is
/// corrected, not tolerated.
///
/// What a longer skew does risk is cloud drifting between the two measurements,
/// which is why the frame-clearness gate and the median estimator matter more
/// here than the clock does.
///
/// The value is set by what the archive actually holds. The photometry pass
/// thins each camera's frames to a cadence independently, so two cameras are
/// rarely measured at the same instant: on a well-observed night at Skibotn,
/// 30 s paired 1 of 32 instants and 300 s paired 21. The real fix is to align
/// the pass across cameras; until then this is the difference between a
/// measurement and an empty result.
pub const DEFAULT_MAX_SKEW_SECONDS: f64 = 300.0;

/// Both measurements must clear this in flux signal-to-noise. A ratio of two
/// noisy fluxes is a noisy ratio, and the logarithm makes the low side worse.
pub const MIN_FLUX_SNR: f64 = 8.0;

/// Below this elevation the air-mass correction carries more uncertainty than
/// the gain it is meant to expose: at 15 degrees the air mass is already about
/// 3.8, so a ten percent error in k is a third of a magnitude.
pub const MIN_ELEVATION_DEG: f64 = 15.0;

/// A frame counts as clear when the median of its stars' brightness against
/// their own archive best reaches this. Judging the *frame* rather than the
/// star keeps the gate independent of the measurement being compared: gating
/// on the star's own flux would select the pairs that suit the answer.
pub const CLEAR_FRAME_FADING: f64 = 0.70;

/// Fewest stars a frame needs before its median says anything about the sky.
pub const MIN_FRAME_STARS: usize = 3;

/// Fewest pairs before a sensitivity ratio is reported at all.
pub const MIN_PAIRS: usize = 12;

/// One star measured by both cameras at the same moment.
#[derive(Clone, Debug)]
pub struct Pair {
    pub star_key: String,
    pub at: String,
    pub flux_a: f64,
    pub flux_b: f64,
    pub elevation_a: f64,
    pub elevation_b: f64,
    pub airmass_a: f64,
    pub airmass_b: f64,
    /// ln(F_A/F_B) as measured.
    pub ln_ratio: f64,
    /// The same, with each camera's own extinction removed. `None` where either
    /// night was never fitted, so the correction would be invented.
    pub corrected: Option<f64>,
    /// Seconds between the two measurements. Reported so a reader can see how
    /// simultaneous the comparison actually was rather than assuming the limit.
    pub skew_seconds: f64,
}

/// What a set of pairs says about the two cameras' relative sensitivity.
#[derive(Clone, Debug)]
pub struct Estimate {
    pub pairs: usize,
    pub corrected_pairs: usize,
    /// G_A/G_B from the extinction-corrected pairs where there are enough of
    /// them, and from the raw ratios otherwise.
    pub gain_ratio: f64,
    /// In magnitudes, which is how a camera's sensitivity is usually quoted.
    pub magnitudes: f64,
    /// Median absolute deviation of the log ratios, a scatter that a handful of
    /// clouded pairs cannot inflate.
    pub scatter: f64,
    /// True when the extinction correction was applied.
    pub extinction_corrected: bool,
}

fn median(values: &mut [f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(values[values.len() / 2])
}

/// Median and median absolute deviation, which is what makes a minority of
/// clouded pairs cost almost nothing.
pub fn robust(values: &[f64]) -> Option<(f64, f64)> {
    let mut sorted = values.to_vec();
    let centre = median(&mut sorted)?;
    let mut spread: Vec<f64> = values.iter().map(|v| (v - centre).abs()).collect();
    let mad = median(&mut spread).unwrap_or(0.0);
    Some((centre, mad))
}

/// Combine pairs into one sensitivity ratio.
pub fn estimate(pairs: &[Pair]) -> Option<Estimate> {
    if pairs.len() < MIN_PAIRS {
        return None;
    }
    let corrected: Vec<f64> = pairs.iter().filter_map(|p| p.corrected).collect();
    let use_corrected = corrected.len() >= MIN_PAIRS;
    let values: Vec<f64> = if use_corrected {
        corrected.clone()
    } else {
        pairs.iter().map(|p| p.ln_ratio).collect()
    };
    let (centre, mad) = robust(&values)?;
    Some(Estimate {
        pairs: pairs.len(),
        corrected_pairs: corrected.len(),
        gain_ratio: centre.exp(),
        // Brighter means a larger flux, and a larger flux is a *smaller*
        // magnitude; the sign here is the usual one and easy to get backwards.
        magnitudes: -2.5 / std::f64::consts::LN_10 * centre,
        scatter: mad,
        extinction_corrected: use_corrected,
    })
}

impl Estimate {
    pub fn to_json(&self) -> Value {
        json!({
            "pairs": self.pairs,
            "extinction_corrected_pairs": self.corrected_pairs,
            "extinction_corrected": self.extinction_corrected,
            "gain_ratio": self.gain_ratio,
            "magnitudes": self.magnitudes,
            "log_scatter_mad": self.scatter,
        })
    }
}

/// Per-frame clearness for one camera: the median of its stars' flux against
/// each star's own best over the archive.
///
/// Judged per frame rather than per star on purpose. A star's own flux is the
/// quantity being compared, so gating on it would choose the pairs that suit
/// the answer; the rest of the frame is independent evidence about the sky.
fn frame_clearness(
    conn: &rusqlite::Connection,
    source_id: &str,
    channel: &str,
    from_utc: &str,
    to_utc: &str,
) -> Result<HashMap<String, f64>> {
    let best = crate::cloudweight::best_flux(conn, source_id, channel)?;
    let mut q = conn.prepare(
        "SELECT observation_utc,star_key,flux FROM star_photometry
         WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 AND observation_utc<?4
           AND flux IS NOT NULL AND flux>0",
    )?;
    let mut per_frame: HashMap<String, Vec<f64>> = HashMap::new();
    let rows = q.query_map(rusqlite::params![source_id, channel, from_utc, to_utc], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
    })?;
    for row in rows {
        let (at, key, flux) = row?;
        if let Some(&reference) = best.get(&key) {
            if reference > 0.0 {
                per_frame.entry(at).or_default().push((flux / reference).clamp(0.0, 1.0));
            }
        }
    }
    Ok(per_frame
        .into_iter()
        .filter(|(_, v)| v.len() >= MIN_FRAME_STARS)
        .filter_map(|(at, mut v)| median(&mut v).map(|m| (at, m)))
        .collect())
}

/// Each camera's fitted extinction coefficient, by night, for one channel.
fn coefficients(
    conn: &rusqlite::Connection,
    source_id: &str,
    channel: &str,
) -> Result<HashMap<i64, f64>> {
    let mut q = conn.prepare(
        "SELECT night,k_mag_per_airmass FROM extinction_nights
         WHERE source_id=?1 AND channel=?2",
    )?;
    let rows = q.query_map(rusqlite::params![source_id, channel], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
    })?;
    Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
}

/// Every star both cameras measured at the same moment, through the gates.
#[allow(clippy::too_many_arguments)]
pub fn pairs(
    conn: &rusqlite::Connection,
    a_id: &str,
    b_id: &str,
    a_longitude: f64,
    b_longitude: f64,
    channel: &str,
    from_utc: &str,
    to_utc: &str,
    max_skew_seconds: f64,
) -> Result<Vec<Pair>> {
    let clear_a = frame_clearness(conn, a_id, channel, from_utc, to_utc)?;
    let clear_b = frame_clearness(conn, b_id, channel, from_utc, to_utc)?;
    let k_a = coefficients(conn, a_id, channel)?;
    let k_b = coefficients(conn, b_id, channel)?;

    let mut q = conn.prepare(
        "SELECT p.star_key,p.observation_utc,q.observation_utc,p.flux,q.flux,
                p.elevation_deg,q.elevation_deg
         FROM star_photometry p
         JOIN star_photometry q
           ON q.star_key=p.star_key AND q.channel=p.channel AND q.source_id=?2
          AND q.observation_utc>=?4 AND q.observation_utc<?5
         WHERE p.source_id=?1 AND p.channel=?3
           AND p.observation_utc>=?4 AND p.observation_utc<?5
           AND p.flux>0 AND q.flux>0
           AND p.flux_snr>=?6 AND q.flux_snr>=?6
           AND p.elevation_deg>=?7 AND q.elevation_deg>=?7
           AND p.background IS NOT NULL AND p.amplitude IS NOT NULL
           AND q.background IS NOT NULL AND q.amplitude IS NOT NULL
           AND p.background+p.amplitude<?8 AND q.background+q.amplitude<?8",
    )?;
    let rows = q.query_map(
        rusqlite::params![
            a_id, b_id, channel, from_utc, to_utc,
            MIN_FLUX_SNR, MIN_ELEVATION_DEG, crate::starphot::SATURATION_LEVEL
        ],
        |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?, r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?, r.get::<_, f64>(6)?,
            ))
        },
    )?;

    // The join admits every q inside the window; keep only the nearest in time
    // to each p, and only where that is inside the simultaneity tolerance.
    let mut best: HashMap<(String, String), (f64, Pair)> = HashMap::new();
    for row in rows {
        let (key, at_a, at_b, flux_a, flux_b, el_a, el_b) = row?;
        let (Ok(ta), Ok(tb)) = (
            chrono::DateTime::parse_from_rfc3339(&at_a),
            chrono::DateTime::parse_from_rfc3339(&at_b),
        ) else {
            continue;
        };
        let skew = (ta - tb).num_milliseconds().abs() as f64 / 1000.0;
        if skew > max_skew_seconds {
            continue;
        }
        // Both frames have to look clear, judged by the rest of their stars.
        if clear_a.get(&at_a).copied().unwrap_or(0.0) < CLEAR_FRAME_FADING
            || clear_b.get(&at_b).copied().unwrap_or(0.0) < CLEAR_FRAME_FADING
        {
            continue;
        }
        let (x_a, x_b) = (airmass(el_a), airmass(el_b));
        if !x_a.is_finite() || !x_b.is_finite() {
            continue;
        }
        let ln_ratio = (flux_a / flux_b).ln();
        let night_a = night_index(ta.timestamp() as f64, a_longitude);
        let night_b = night_index(tb.timestamp() as f64, b_longitude);
        // Only where *both* nights were fitted: half a correction is worse
        // than none, because it silently attributes one camera's atmosphere to
        // the other camera's gain.
        let corrected = match (k_a.get(&night_a), k_b.get(&night_b)) {
            (Some(&ka), Some(&kb)) => Some(ln_ratio + BETA * (ka * x_a - kb * x_b)),
            _ => None,
        };
        let pair = Pair {
            star_key: key.clone(), at: at_a.clone(),
            flux_a, flux_b, elevation_a: el_a, elevation_b: el_b,
            airmass_a: x_a, airmass_b: x_b, ln_ratio, corrected,
            skew_seconds: skew,
        };
        let slot = best.entry((key, at_a)).or_insert((skew, pair.clone()));
        if skew < slot.0 {
            *slot = (skew, pair);
        }
    }
    let mut out: Vec<Pair> = best.into_values().map(|(_, p)| p).collect();
    out.sort_by(|p, q| p.at.cmp(&q.at).then(p.star_key.cmp(&q.star_key)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(ln_ratio: f64, corrected: Option<f64>) -> Pair {
        Pair {
            star_key: "s".into(), at: "2026-09-12T20:00:00+00:00".into(),
            flux_a: 1.0, flux_b: 1.0, elevation_a: 60.0, elevation_b: 60.0,
            airmass_a: 1.15, airmass_b: 1.15, ln_ratio, corrected, skew_seconds: 0.0,
        }
    }

    #[test]
    fn a_known_gain_ratio_is_recovered() {
        // Camera A is 1.6 times as sensitive as B. Every pair says so.
        let truth = 1.6_f64;
        let pairs: Vec<Pair> = (0..40).map(|_| pair(truth.ln(), Some(truth.ln()))).collect();
        let e = estimate(&pairs).unwrap();
        assert!((e.gain_ratio - truth).abs() < 1e-9, "got {}", e.gain_ratio);
        // 1.6x brighter is half a magnitude, and the sign is the usual one:
        // more flux is a smaller magnitude.
        assert!((e.magnitudes + 0.51).abs() < 0.01, "got {} mag", e.magnitudes);
        assert!(e.extinction_corrected);
    }

    #[test]
    fn a_minority_of_clouded_pairs_barely_moves_the_answer() {
        // Thirty clear pairs at a ratio of 2, and ten where cloud over B has
        // made A look far brighter than it is. A mean would be dragged well
        // off; the median must not be.
        let mut pairs: Vec<Pair> = (0..30).map(|_| pair(2f64.ln(), Some(2f64.ln()))).collect();
        pairs.extend((0..10).map(|_| pair(20f64.ln(), Some(20f64.ln()))));
        let e = estimate(&pairs).unwrap();
        assert!((e.gain_ratio - 2.0).abs() < 1e-9, "got {}", e.gain_ratio);
        let mean: f64 = pairs.iter().map(|p| p.ln_ratio).sum::<f64>() / pairs.len() as f64;
        assert!(mean.exp() > 3.0, "the fixture should defeat a mean, got {}", mean.exp());
    }

    #[test]
    fn scatter_is_reported_and_survives_an_outlier() {
        let mut pairs: Vec<Pair> = (0..20)
            .map(|n| pair(0.1 * ((n % 5) as f64 - 2.0), Some(0.1 * ((n % 5) as f64 - 2.0))))
            .collect();
        let clean = estimate(&pairs).unwrap().scatter;
        pairs.push(pair(9.0, Some(9.0)));
        let dirtied = estimate(&pairs).unwrap().scatter;
        assert!((clean - dirtied).abs() < 0.05, "{clean} -> {dirtied}");
    }

    #[test]
    fn too_few_pairs_says_nothing() {
        let pairs: Vec<Pair> = (0..MIN_PAIRS - 1).map(|_| pair(0.0, Some(0.0))).collect();
        assert!(estimate(&pairs).is_none());
    }

    #[test]
    fn the_raw_ratio_is_used_when_the_nights_were_never_fitted() {
        // Without both coefficients the correction would be invented, so the
        // estimate falls back and says so rather than silently half-correcting.
        let pairs: Vec<Pair> = (0..30).map(|_| pair(1.4_f64.ln(), None)).collect();
        let e = estimate(&pairs).unwrap();
        assert!(!e.extinction_corrected);
        assert_eq!(e.corrected_pairs, 0);
        assert!((e.gain_ratio - 1.4).abs() < 1e-9);
    }

    #[test]
    fn the_extinction_term_removes_a_pure_air_mass_difference() {
        // Same camera twice, so the true gain ratio is 1, but one view is at a
        // much higher air mass. Uncorrected the ratio is wrong by the whole
        // extinction difference; corrected it is 1.
        let (k, x_a, x_b) = (0.18, 1.05, 2.60);
        let ln_ratio = -BETA * (k * x_a - k * x_b);
        let corrected = ln_ratio + BETA * (k * x_a - k * x_b);
        assert!(ln_ratio.abs() > 0.2, "fixture has no air-mass difference");
        assert!(corrected.abs() < 1e-12, "correction did not cancel: {corrected}");
        let pairs: Vec<Pair> = (0..20).map(|_| pair(ln_ratio, Some(corrected))).collect();
        let e = estimate(&pairs).unwrap();
        assert!((e.gain_ratio - 1.0).abs() < 1e-9, "got {}", e.gain_ratio);
    }
}
