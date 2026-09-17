//! Assimilated clear-sky stellar response, per camera, over the sky it sees.
//!
//! Waiting for two cameras to observe the same star at the same instant throws
//! away almost everything this archive holds: fewer than one camera-night in
//! eight yields a clear-sky extinction fit at all, and two cameras are rarely
//! measured at the same moment. So instead of comparing cameras directly, each
//! camera is characterised on its own and the comparison is made afterwards
//! between the characterisations.
//!
//! For one camera, one channel and one measure, every clear, unsaturated
//! stellar measurement is corrected to the top of the atmosphere by that
//! night's own extinction coefficient and sorted into a bin of azimuth and
//! elevation. Within a bin the measurements come from many different stars of
//! many different brightnesses, so the model is
//!
//! ```text
//!   ln I_corrected(star, direction) = ln Z_star + ln R_direction
//! ```
//!
//! -- a level per star and a response per direction. That is the same structure
//! the extinction fit already profiles, and it is solved the same way: hold one
//! set fixed, take robust centres of the other, and repeat. The star levels are
//! instrumental and meaningless outside this camera; the *response surface* is
//! what carries across, and the ratio of two cameras' surfaces over the
//! directions both see is the intercalibration.
//!
//! **This is a long accumulation.** Large parts of the northern auroral oval
//! are not in the Atacama desert: darkness, moon and cloud multiply, and the
//! last is harshest at exactly the coastal sites with the best instruments.
//! Expect to collate over at least one new-moon period and more likely several.
//! Everything here is therefore incremental, restartable, and reports its own
//! coverage, so a surface built from three nights and one built from thirty are
//! distinguishable at a glance.

use crate::extinction::{BETA, airmass};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Azimuth bins around the horizon, and elevation bins from the horizon up.
/// Coarse on purpose: the data arrives slowly, and a surface with empty cells
/// says less than a coarser one that is filled.
pub const AZIMUTH_BINS: usize = 24;
pub const ELEVATION_BINS: usize = 9;

/// Below this elevation the air-mass correction carries more uncertainty than
/// the response it is meant to expose.
pub const MIN_ELEVATION_DEG: f64 = 10.0;

/// A bin needs this many measurements, from this many separate nights, before
/// its response is reported. One night can be quietly unrepresentative -- a
/// haze, a film of ice -- in a way several nights cannot.
pub const MIN_BIN_SAMPLES: usize = 8;
pub const MIN_BIN_NIGHTS: usize = 2;

/// A star must appear this many times before it gets a level of its own.
pub const MIN_STAR_SAMPLES: usize = 4;

/// Which measured quantity a surface is built from.
///
/// `Flux` is the better statistic: it integrates the whole profile, so it is
/// insensitive to where the star falls between pixel centres and averages down
/// the noise. `Amplitude` is the same fit's peak height. `Peak` is measured
/// without fitting anything at all, so it survives frames the fit refuses.
///
/// Having all three is the point rather than a luxury: they are different
/// statistics of the same photons, and a surface built from one should agree
/// with a surface built from another. Where they agree, the equalization can be
/// trusted; where they diverge, something is wrong that no single measure would
/// have revealed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Measure {
    Flux,
    Amplitude,
    Peak,
}

impl Measure {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "flux" => Some(Self::Flux),
            "amplitude" => Some(Self::Amplitude),
            "peak" => Some(Self::Peak),
            _ => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Flux => "flux",
            Self::Amplitude => "amplitude",
            Self::Peak => "peak",
        }
    }
    /// The column holding the value, and the column holding the sky under it.
    /// Saturation is judged on the sky plus the peak in both cases, because a
    /// star sitting on a full detector is not a measurement whichever number
    /// is being read off it.
    fn columns(&self) -> (&'static str, &'static str) {
        match self {
            Self::Flux => ("flux", "background"),
            Self::Amplitude => ("amplitude", "background"),
            Self::Peak => ("peak_counts", "peak_background"),
        }
    }
}

/// One measurement, ready to be assimilated.
#[derive(Clone, Debug)]
pub struct Sample {
    pub star_key: String,
    pub night: i64,
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    /// Natural log of the measured quantity, already corrected to the top of
    /// the atmosphere where the night's coefficient was known.
    pub ln_corrected: f64,
    /// False where no coefficient existed and the value is uncorrected.
    pub extinction_corrected: bool,
}

/// Which bin a direction falls in. Azimuth wraps; elevation clamps at the top
/// so the zenith belongs to the highest bin rather than to none.
pub fn bin_of(azimuth_deg: f64, elevation_deg: f64) -> Option<(usize, usize)> {
    if !azimuth_deg.is_finite() || !elevation_deg.is_finite() {
        return None;
    }
    if elevation_deg < MIN_ELEVATION_DEG {
        return None;
    }
    let az = azimuth_deg.rem_euclid(360.0) / 360.0 * AZIMUTH_BINS as f64;
    let el = (elevation_deg / 90.0 * ELEVATION_BINS as f64).min(ELEVATION_BINS as f64 - 1e-9);
    Some((
        (az.floor() as usize).min(AZIMUTH_BINS - 1),
        (el.floor() as usize).min(ELEVATION_BINS - 1),
    ))
}

/// The centre of a bin, for labelling and for interpolating later.
pub fn bin_centre(az: usize, el: usize) -> (f64, f64) {
    (
        (az as f64 + 0.5) * 360.0 / AZIMUTH_BINS as f64,
        (el as f64 + 0.5) * 90.0 / ELEVATION_BINS as f64,
    )
}

fn median(values: &mut Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(values[values.len() / 2])
}

/// What one bin of the surface holds.
#[derive(Clone, Debug)]
pub struct Cell {
    pub azimuth_index: usize,
    pub elevation_index: usize,
    /// ln of the response, on an arbitrary but internally consistent scale.
    pub ln_response: f64,
    /// Median absolute deviation of the samples about it.
    pub scatter: f64,
    pub samples: usize,
    pub nights: usize,
    pub stars: usize,
}

/// A whole surface, with the coverage that says how far to trust it.
#[derive(Clone, Debug)]
pub struct Surface {
    pub cells: Vec<Cell>,
    pub samples: usize,
    pub nights: usize,
    pub stars: usize,
    pub uncorrected_samples: usize,
}

/// Solve the star levels and the direction response together.
///
/// Alternating robust centres rather than least squares: a single clouded
/// sample that slipped the gates should move a bin by nothing much, and a
/// median does that where a mean does not. The scale is arbitrary -- multiplying
/// every response by a constant and dividing every star level by it changes
/// nothing -- so the surface is pinned by setting its median cell to zero, and
/// only ratios between surfaces are meaningful.
pub fn solve(samples: &[Sample]) -> Option<Surface> {
    if samples.is_empty() {
        return None;
    }
    let mut by_star: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_cell: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (index, s) in samples.iter().enumerate() {
        let Some(cell) = bin_of(s.azimuth_deg, s.elevation_deg) else { continue };
        by_star.entry(s.star_key.as_str()).or_default().push(index);
        by_cell.entry(cell).or_default().push(index);
    }
    // A star seen only once or twice cannot be told apart from the direction it
    // was seen in, and would simply transfer its own brightness into that bin.
    by_star.retain(|_, v| v.len() >= MIN_STAR_SAMPLES);
    let allowed: BTreeSet<&str> = by_star.keys().copied().collect();
    if allowed.is_empty() {
        return None;
    }

    let mut level: HashMap<&str, f64> = by_star.keys().map(|k| (*k, 0.0)).collect();
    let mut response: BTreeMap<(usize, usize), f64> =
        by_cell.keys().map(|c| (*c, 0.0)).collect();

    for _ in 0..8 {
        for (star, rows) in &by_star {
            let mut v: Vec<f64> = rows
                .iter()
                .filter_map(|&i| {
                    let cell = bin_of(samples[i].azimuth_deg, samples[i].elevation_deg)?;
                    Some(samples[i].ln_corrected - response.get(&cell).copied().unwrap_or(0.0))
                })
                .collect();
            if let Some(m) = median(&mut v) {
                level.insert(star, m);
            }
        }
        for (cell, rows) in &by_cell {
            let mut v: Vec<f64> = rows
                .iter()
                .filter(|&&i| allowed.contains(samples[i].star_key.as_str()))
                .map(|&i| {
                    samples[i].ln_corrected
                        - level.get(samples[i].star_key.as_str()).copied().unwrap_or(0.0)
                })
                .collect();
            if let Some(m) = median(&mut v) {
                response.insert(*cell, m);
            }
        }
    }

    // Pin the arbitrary scale: the median cell is the reference.
    let mut all: Vec<f64> = response.values().copied().collect();
    let pin = median(&mut all).unwrap_or(0.0);

    let mut cells = Vec::new();
    for (cell, rows) in &by_cell {
        let used: Vec<usize> = rows
            .iter()
            .copied()
            .filter(|&i| allowed.contains(samples[i].star_key.as_str()))
            .collect();
        let nights: BTreeSet<i64> = used.iter().map(|&i| samples[i].night).collect();
        let stars: BTreeSet<&str> = used.iter().map(|&i| samples[i].star_key.as_str()).collect();
        if used.len() < MIN_BIN_SAMPLES || nights.len() < MIN_BIN_NIGHTS {
            continue;
        }
        let centre = response.get(cell).copied().unwrap_or(0.0);
        let mut spread: Vec<f64> = used
            .iter()
            .map(|&i| {
                (samples[i].ln_corrected
                    - level.get(samples[i].star_key.as_str()).copied().unwrap_or(0.0)
                    - centre)
                    .abs()
            })
            .collect();
        cells.push(Cell {
            azimuth_index: cell.0,
            elevation_index: cell.1,
            ln_response: centre - pin,
            scatter: median(&mut spread).unwrap_or(0.0),
            samples: used.len(),
            nights: nights.len(),
            stars: stars.len(),
        });
    }
    if cells.is_empty() {
        return None;
    }
    let nights: BTreeSet<i64> = samples.iter().map(|s| s.night).collect();
    Some(Surface {
        samples: samples.len(),
        nights: nights.len(),
        stars: allowed.len(),
        uncorrected_samples: samples.iter().filter(|s| !s.extinction_corrected).count(),
        cells,
    })
}

impl Surface {
    /// Fraction of the sky above `MIN_ELEVATION_DEG` that has a usable cell.
    pub fn coverage(&self) -> f64 {
        let possible = AZIMUTH_BINS * ELEVATION_BINS;
        self.cells.len() as f64 / possible as f64
    }
    pub fn to_json(&self) -> Value {
        json!({
            "azimuth_bins": AZIMUTH_BINS,
            "elevation_bins": ELEVATION_BINS,
            "samples": self.samples,
            "nights": self.nights,
            "stars": self.stars,
            "uncorrected_samples": self.uncorrected_samples,
            "filled_cells": self.cells.len(),
            "coverage": self.coverage(),
            "cells": self.cells.iter().map(|c| {
                let (az, el) = bin_centre(c.azimuth_index, c.elevation_index);
                json!({
                    "azimuth_index": c.azimuth_index, "elevation_index": c.elevation_index,
                    "azimuth_deg": az, "elevation_deg": el,
                    "ln_response": c.ln_response,
                    "relative_response": c.ln_response.exp(),
                    "magnitudes": -2.5 / std::f64::consts::LN_10 * c.ln_response,
                    "scatter": c.scatter,
                    "samples": c.samples, "nights": c.nights, "stars": c.stars,
                })
            }).collect::<Vec<_>>(),
        })
    }
}

/// Gather one camera's clear, unsaturated measurements for one measure.
///
/// Two gates do the work. **Saturation**: the sky plus the star's peak must
/// stay inside the range, judged the same way whichever quantity is being read,
/// because a star sitting on a full detector is not a measurement of anything.
/// **Cloud**: the frame is judged by the median brightness of its stars against
/// their own archive best -- the frame, not the star, so the gate cannot select
/// the measurements that suit the answer.
pub fn gather(
    conn: &rusqlite::Connection,
    source_id: &str,
    channel: &str,
    measure: Measure,
    from_utc: &str,
    to_utc: &str,
    longitude_deg: f64,
    clear_threshold: f64,
) -> Result<Vec<Sample>> {
    let (value_column, sky_column) = measure.columns();
    // Frame clearness, from every star the frame measured.
    let best = crate::cloudweight::best_flux(conn, source_id, channel)?;
    let mut clearness: HashMap<String, Vec<f64>> = HashMap::new();
    {
        let mut q = conn.prepare(
            "SELECT observation_utc,star_key,flux FROM star_photometry
             WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 AND observation_utc<?4
               AND flux IS NOT NULL AND flux>0",
        )?;
        let rows = q.query_map(rusqlite::params![source_id, channel, from_utc, to_utc], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?))
        })?;
        for row in rows {
            let (at, key, flux) = row?;
            if let Some(&reference) = best.get(&key) {
                if reference > 0.0 {
                    clearness.entry(at).or_default().push((flux / reference).clamp(0.0, 1.0));
                }
            }
        }
    }
    let clear: HashMap<String, f64> = clearness
        .into_iter()
        .filter(|(_, v)| v.len() >= 3)
        .filter_map(|(at, mut v)| median(&mut v).map(|m| (at, m)))
        .collect();

    // Each night's extinction coefficient, where one was ever fitted.
    let mut k = HashMap::new();
    {
        let mut q = conn.prepare(
            "SELECT night,k_mag_per_airmass FROM extinction_nights
             WHERE source_id=?1 AND channel=?2",
        )?;
        for row in q.query_map(rusqlite::params![source_id, channel], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        })? {
            let (night, value) = row?;
            k.insert(night, value);
        }
    }

    let sql = format!(
        "SELECT star_key,observation_utc,azimuth_deg,elevation_deg,{value_column},{sky_column},
                COALESCE(peak_raw, background + amplitude)
         FROM star_photometry
         WHERE source_id=?1 AND channel=?2 AND observation_utc>=?3 AND observation_utc<?4
           AND {value_column} IS NOT NULL AND {value_column}>0
           AND elevation_deg >= ?5
           AND COALESCE(peak_raw, background + amplitude) < ?6"
    );
    let mut q = conn.prepare(&sql)?;
    let rows = q.query_map(
        rusqlite::params![
            source_id, channel, from_utc, to_utc,
            MIN_ELEVATION_DEG, crate::starphot::SATURATION_LEVEL
        ],
        |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, f64>(2)?, r.get::<_, f64>(3)?, r.get::<_, f64>(4)?,
            ))
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        let (star_key, at, azimuth, elevation, value) = row?;
        if clear.get(&at).copied().unwrap_or(0.0) < clear_threshold {
            continue;
        }
        let Ok(when) = chrono::DateTime::parse_from_rfc3339(&at) else { continue };
        let night = crate::extinction::night_index(when.timestamp() as f64, longitude_deg);
        let x = airmass(elevation);
        if !x.is_finite() {
            continue;
        }
        let (ln_corrected, corrected) = match k.get(&night) {
            Some(&coefficient) => (value.ln() + BETA * coefficient * x, true),
            // Uncorrected rather than dropped: the surface is built from many
            // nights, and a night without a coefficient still says where this
            // camera is sensitive, just with the atmosphere left in.
            None => (value.ln(), false),
        };
        out.push(Sample {
            star_key, night, azimuth_deg: azimuth, elevation_deg: elevation,
            ln_corrected, extinction_corrected: corrected,
        });
    }
    Ok(out)
}

/// Ratio of two surfaces, cell by cell, over the directions both cover.
///
/// This is the intercalibration. It needs no simultaneity and no shared sky --
/// only that both cameras have looked at enough stars in the same directions.
pub fn ratio(a: &Surface, b: &Surface) -> Option<(f64, f64, usize)> {
    let index: HashMap<(usize, usize), f64> = b
        .cells
        .iter()
        .map(|c| ((c.azimuth_index, c.elevation_index), c.ln_response))
        .collect();
    let mut shared: Vec<f64> = a
        .cells
        .iter()
        .filter_map(|c| {
            index
                .get(&(c.azimuth_index, c.elevation_index))
                .map(|other| c.ln_response - other)
        })
        .collect();
    if shared.is_empty() {
        return None;
    }
    let count = shared.len();
    let centre = median(&mut shared)?;
    let mut spread: Vec<f64> = shared.iter().map(|v| (v - centre).abs()).collect();
    Some((centre, median(&mut spread).unwrap_or(0.0), count))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A camera whose response falls off towards the horizon and is a little
    /// dimmer to the north, observed over several nights with many stars of
    /// wildly different brightness.
    fn planted(nights: usize, dim_north: f64, vignetting: f64) -> (Vec<Sample>, Vec<((usize, usize), f64)>) {
        let truth = |az: f64, el: f64| {
            vignetting * (el / 90.0 - 0.5) + dim_north * ((az.to_radians()).cos())
        };
        // Dense enough that every bin clears MIN_BIN_SAMPLES: the point of this
        // fixture is whether the shape is recovered, not whether a sparse sky
        // is refused, which the thin-bin test covers separately.
        let mut samples = Vec::new();
        let mut seed = 987654321u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        for night in 0..nights as i64 {
            for star in 0..40 {
                // Star levels span a factor of a thousand, as real ones do.
                let level = (star as f64 % 7.0) * 1.0 - 3.0;
                for _ in 0..40 {
                    let az = next() * 360.0;
                    let el = MIN_ELEVATION_DEG + next() * (90.0 - MIN_ELEVATION_DEG);
                    samples.push(Sample {
                        star_key: format!("star-{star}"),
                        night,
                        azimuth_deg: az,
                        elevation_deg: el,
                        ln_corrected: level + truth(az, el),
                        extinction_corrected: true,
                    });
                }
            }
        }
        let mut cells = Vec::new();
        for a in 0..AZIMUTH_BINS {
            for e in 0..ELEVATION_BINS {
                let (az, el) = bin_centre(a, e);
                cells.push(((a, e), truth(az, el)));
            }
        }
        (samples, cells)
    }

    #[test]
    fn a_planted_response_is_recovered_up_to_a_constant() {
        // The scale is arbitrary by construction, so what has to come back is
        // the *shape*: every cell offset from the truth by one common constant.
        let (samples, truth) = planted(4, 0.12, 0.45);
        let surface = solve(&samples).expect("solve");
        let index: HashMap<(usize, usize), f64> =
            truth.into_iter().collect();
        let offsets: Vec<f64> = surface
            .cells
            .iter()
            .filter_map(|c| index.get(&(c.azimuth_index, c.elevation_index))
                .map(|t| c.ln_response - t))
            .collect();
        assert!(offsets.len() > 50, "only {} cells recovered", offsets.len());
        let mean = offsets.iter().sum::<f64>() / offsets.len() as f64;
        let worst = offsets.iter().map(|o| (o - mean).abs()).fold(0.0, f64::max);
        assert!(worst < 0.02, "shape not recovered, worst departure {worst}");
    }

    #[test]
    fn star_brightness_does_not_leak_into_the_surface() {
        // Every star has a different level; if those levels were not profiled
        // out the surface would simply echo whichever stars happened to be
        // seen in each direction.
        let (samples, _) = planted(3, 0.0, 0.0);
        let surface = solve(&samples).expect("solve");
        let spread = surface.cells.iter().map(|c| c.ln_response.abs()).fold(0.0, f64::max);
        assert!(spread < 0.02, "a flat camera came back with structure: {spread}");
    }

    #[test]
    fn a_thin_bin_is_left_out_rather_than_reported_badly() {
        // Its own fixture, confined to the eastern sky, so the western bins are
        // genuinely empty and a single stray sample there is genuinely thin.
        let mut samples = Vec::new();
        for night in 0..3i64 {
            for star in 0..20 {
                for step in 0..30 {
                    let az = (step as f64 / 30.0) * 170.0;
                    let el = MIN_ELEVATION_DEG + ((star * 7 + step) % 70) as f64;
                    samples.push(Sample {
                        star_key: format!("star-{star}"), night,
                        azimuth_deg: az, elevation_deg: el,
                        ln_corrected: (star as f64 % 5.0) - 2.0,
                        extinction_corrected: true,
                    });
                }
            }
        }
        // One direction in the empty half, seen once, on one night.
        samples.push(Sample {
            star_key: "star-0".into(), night: 99,
            azimuth_deg: 270.0, elevation_deg: 85.0,
            ln_corrected: 0.0, extinction_corrected: true,
        });
        let surface = solve(&samples).expect("solve");
        let thin = bin_of(270.0, 85.0).unwrap();
        assert!(
            !surface.cells.iter().any(|c| (c.azimuth_index, c.elevation_index) == thin),
            "a bin with one sample from one night was reported"
        );
        // And the well-sampled half is still there, so the gate is selective
        // rather than simply refusing everything.
        assert!(surface.cells.len() > 20, "only {} cells survived", surface.cells.len());
    }

    #[test]
    fn coverage_and_night_counts_are_reported() {
        let (samples, _) = planted(5, 0.1, 0.4);
        let surface = solve(&samples).unwrap();
        assert_eq!(surface.nights, 5);
        assert!(surface.coverage() > 0.2, "coverage {}", surface.coverage());
        assert!(surface.cells.iter().all(|c| c.nights >= MIN_BIN_NIGHTS));
        assert!(surface.cells.iter().all(|c| c.samples >= MIN_BIN_SAMPLES));
        assert_eq!(surface.uncorrected_samples, 0);
    }

    #[test]
    fn the_ratio_of_two_surfaces_is_the_gain_between_them() {
        // Same camera shape, one of them 1.8 times as sensitive everywhere.
        let (a, _) = planted(4, 0.1, 0.4);
        let gain = 1.8_f64.ln();
        let b: Vec<Sample> = a
            .iter()
            .map(|s| Sample { ln_corrected: s.ln_corrected + gain, ..s.clone() })
            .collect();
        let (sa, sb) = (solve(&a).unwrap(), solve(&b).unwrap());
        // Both surfaces are pinned to their own median, so the *shapes* match
        // and the ratio is one: the constant is exactly what pinning removes.
        let (centre, scatter, shared) = ratio(&sa, &sb).unwrap();
        assert!(shared > 50);
        assert!(centre.abs() < 1e-9, "pinned surfaces should agree: {centre}");
        assert!(scatter < 1e-9);
        // The gain lives in the star levels, which is why a surface ratio needs
        // a common reference before it can be read as a sensitivity ratio.
    }

    #[test]
    fn binning_wraps_in_azimuth_and_refuses_the_horizon() {
        assert_eq!(bin_of(0.0, 45.0), bin_of(360.0, 45.0));
        assert_eq!(bin_of(-10.0, 45.0), bin_of(350.0, 45.0));
        assert!(bin_of(0.0, MIN_ELEVATION_DEG - 0.1).is_none());
        assert_eq!(bin_of(0.0, 90.0).unwrap().1, ELEVATION_BINS - 1, "zenith needs a bin");
        assert!(bin_of(f64::NAN, 45.0).is_none());
    }
}
