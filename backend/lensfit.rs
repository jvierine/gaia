//! Refitting a camera's lens parameters from the stars it actually saw.
//!
//! The forward model already exists: `starphot::az_el_to_pixel` takes a star's
//! azimuth and elevation and the AIDA/WISC optical parameters and says where in
//! the image that star should fall. A calibration is nothing more than the
//! parameter vector that makes those predictions land on the measured
//! centroids. So a refit is the same model run backwards: hold the model number
//! fixed, vary the eight parameters, and minimise the distance between where
//! the stars were predicted and where they were found.
//!
//! This exists because lenses drift. A camera knocked, refocused or simply left
//! out for a winter no longer matches the model fitted a year ago, and the
//! symptom -- a standing offset between prediction and centroid -- is exactly
//! what the star photometry measures on every frame. The archive can therefore
//! notice its own calibrations going stale.
//!
//! Nothing here replaces a live calibration. A refit is added to the list and
//! left unselected; which model a camera uses stays an administrator's decision.

use crate::starphot::az_el_to_pixel;

/// One star in one frame: where the sky says it is, and where it was found.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub x: f64,
    pub y: f64,
}

/// Fewest stars a refit is allowed to run on. Eight free parameters need a good
/// deal more than eight points before the fit describes the lens rather than
/// the noise, and a frame this rich is not rare.
pub const MIN_OBSERVATIONS: usize = 20;

/// Predicted position minus measured, per star, as (dx, dy, dr). Stars the
/// model cannot place at all are skipped, which is why the caller gets the
/// count back rather than assuming it.
pub fn residuals(optpar: &[f64], observations: &[Observation], width: f64, height: f64)
    -> Vec<(f64, f64, f64)>
{
    observations
        .iter()
        .filter_map(|o| {
            let (x, y) = az_el_to_pixel(o.azimuth_deg, o.elevation_deg, optpar, width, height)?;
            let (dx, dy) = (x - o.x, y - o.y);
            Some((dx, dy, dx.hypot(dy)))
        })
        .collect()
}

/// Root-mean-square distance between prediction and centroid, in pixels.
///
/// Infinite when too few stars can be placed at all: a parameter set that
/// cannot project the sky is not a better fit for having fewer residuals to
/// average, and without this the search happily wanders somewhere the model
/// breaks down and calls it an improvement.
pub fn rms(optpar: &[f64], observations: &[Observation], width: f64, height: f64) -> f64 {
    let r = residuals(optpar, observations, width, height);
    if r.len() * 2 < observations.len() {
        return f64::INFINITY;
    }
    (r.iter().map(|(_, _, d)| d * d).sum::<f64>() / r.len() as f64).sqrt()
}

/// Nelder-Mead over the eight optical parameters, with the model number held.
///
/// Downhill simplex rather than a gradient method because the model is a match
/// on an integer model number with several branches, so the objective is not
/// smooth everywhere and a numerical Jacobian across a branch boundary is
/// meaningless. The search starts from the calibration in force, so it is a
/// local refinement of a fit that was already good -- it is not asked to find a
/// lens model from nothing.
pub fn fit(seed: &[f64], observations: &[Observation], width: f64, height: f64)
    -> Option<(Vec<f64>, f64)>
{
    if seed.len() < 9 || observations.len() < MIN_OBSERVATIONS {
        return None;
    }
    let n = seed.len() - 1;
    let cost = |free: &[f64]| -> f64 {
        let mut p = Vec::with_capacity(seed.len());
        p.push(seed[0]);
        p.extend_from_slice(free);
        rms(&p, observations, width, height)
    };
    let start: Vec<f64> = seed[1..].to_vec();
    // Steps scaled to each parameter's own size: the focal lengths are order
    // one, the rotations order a hundred degrees, the offsets order a
    // thousandth. One absolute step cannot serve all three.
    let step: Vec<f64> = start.iter().map(|v| (v.abs() * 0.02).max(1e-4)).collect();

    let mut simplex: Vec<(Vec<f64>, f64)> = Vec::with_capacity(n + 1);
    simplex.push((start.clone(), cost(&start)));
    for i in 0..n {
        let mut p = start.clone();
        p[i] += step[i];
        let c = cost(&p);
        simplex.push((p, c));
    }
    if !simplex[0].1.is_finite() {
        return None;
    }

    let centroid = |s: &[(Vec<f64>, f64)]| -> Vec<f64> {
        let mut c = vec![0.0; n];
        for (p, _) in &s[..n] {
            for i in 0..n {
                c[i] += p[i] / n as f64;
            }
        }
        c
    };
    let combine = |a: &[f64], b: &[f64], t: f64| -> Vec<f64> {
        (0..n).map(|i| a[i] + t * (b[i] - a[i])).collect()
    };

    for _ in 0..4000 {
        simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        // Converged once the simplex is smaller than a thousandth of a pixel in
        // objective terms; further iterations move the lens model by less than
        // the centroids are known to.
        if (simplex[n].1 - simplex[0].1).abs() < 1e-6 {
            break;
        }
        let c = centroid(&simplex);
        let worst = simplex[n].0.clone();
        let reflected = combine(&c, &worst, -1.0);
        let fr = cost(&reflected);
        if fr < simplex[0].1 {
            let expanded = combine(&c, &worst, -2.0);
            let fe = cost(&expanded);
            simplex[n] = if fe < fr { (expanded, fe) } else { (reflected, fr) };
        } else if fr < simplex[n - 1].1 {
            simplex[n] = (reflected, fr);
        } else {
            let contracted = combine(&c, &worst, 0.5);
            let fc = cost(&contracted);
            if fc < simplex[n].1 {
                simplex[n] = (contracted, fc);
            } else {
                // Shrink towards the best vertex.
                let best = simplex[0].0.clone();
                for entry in simplex.iter_mut().skip(1) {
                    let p = combine(&best, &entry.0, 0.5);
                    let f = cost(&p);
                    *entry = (p, f);
                }
            }
        }
    }
    simplex.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let (best, value) = simplex.remove(0);
    if !value.is_finite() {
        return None;
    }
    let mut out = Vec::with_capacity(seed.len());
    out.push(seed[0]);
    out.extend_from_slice(&best);
    Some((out, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real Kiruna calibration: model 2, the equisolid-like branch.
    const TRUTH: [f64; 9] = [
        2.0, 0.733181, 0.733112, -1.454884, -1.366835, -170.19926, 0.007369, -0.003686, 0.460838,
    ];
    const W: f64 = 3072.0;
    const H: f64 = 2048.0;

    /// Stars spread over the sky, projected through the true model. Only those
    /// the model actually places land in the set, so the fixture is exactly
    /// what a frame would offer.
    fn planted(optpar: &[f64], count: usize) -> Vec<Observation> {
        let mut out = Vec::new();
        for i in 0..count {
            // A coarse spiral, so azimuth and elevation are not correlated with
            // each other or with the index.
            let azimuth = (i as f64 * 137.508) % 360.0;
            let elevation = 12.0 + 72.0 * ((i * 7 % 23) as f64 / 23.0);
            if let Some((x, y)) = az_el_to_pixel(azimuth, elevation, optpar, W, H) {
                if x > 0.0 && y > 0.0 && x < W && y < H {
                    out.push(Observation { azimuth_deg: azimuth, elevation_deg: elevation, x, y });
                }
            }
        }
        out
    }

    #[test]
    fn the_truth_has_no_residual_against_itself() {
        let obs = planted(&TRUTH, 80);
        assert!(obs.len() >= MIN_OBSERVATIONS, "fixture too small: {}", obs.len());
        assert!(rms(&TRUTH, &obs, W, H) < 1e-9);
    }

    #[test]
    fn a_drifted_lens_is_recovered() {
        let obs = planted(&TRUTH, 120);
        // A plausible drift: the camera has rotated a little and shifted on its
        // mount, and the focal scale has crept.
        let mut drifted = TRUTH;
        drifted[1] *= 1.004;
        drifted[2] *= 0.997;
        drifted[3] += 0.25;
        drifted[5] += 0.40;
        drifted[6] += 0.0015;
        drifted[7] -= 0.0011;
        let before = rms(&drifted, &obs, W, H);
        assert!(before > 2.0, "drift too small to be a test: {before} px");

        let (fitted, after) = fit(&drifted, &obs, W, H).expect("fit failed");
        assert_eq!(fitted[0], TRUTH[0], "the model number must not move");
        assert!(after < 0.05, "residual still {after} px after refit");
        assert!(after < before / 20.0, "{before} -> {after}");
    }

    #[test]
    fn noisy_centroids_still_improve_the_model() {
        let mut obs = planted(&TRUTH, 120);
        // Half a pixel of centroid scatter, which is about what the archive
        // shows on a well-pointed camera.
        let mut seed = 12345u64;
        let mut noise = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as f64 / (1u64 << 31) as f64 - 0.5) * 1.0
        };
        for o in &mut obs {
            o.x += noise();
            o.y += noise();
        }
        let mut drifted = TRUTH;
        drifted[3] += 0.3;
        drifted[6] += 0.002;
        let before = rms(&drifted, &obs, W, H);
        let (_, after) = fit(&drifted, &obs, W, H).expect("fit failed");
        assert!(after < before, "{before} -> {after}");
        // It cannot beat the noise it was given, and must not claim to.
        assert!(after > 0.1, "suspiciously perfect against noisy data: {after}");
        assert!(after < 0.6, "did not converge through the noise: {after}");
    }

    #[test]
    fn a_fit_is_refused_where_there_is_nothing_to_fit_to() {
        let obs = planted(&TRUTH, 120);
        assert!(fit(&TRUTH, &obs[..5], W, H).is_none(), "fitted 5 stars to 8 parameters");
        assert!(fit(&TRUTH[..4], &obs, W, H).is_none(), "accepted a truncated seed");
    }

    #[test]
    fn parameters_that_cannot_project_the_sky_are_not_an_improvement() {
        // Fewer than half the stars placed is refused outright, so the search
        // cannot escape into a region where the model simply stops working.
        let obs = planted(&TRUTH, 120);
        let mut broken = TRUTH;
        broken[1] = 0.0;
        broken[2] = 0.0;
        let r = rms(&broken, &obs, W, H);
        assert!(r.is_infinite() || r > 100.0, "degenerate parameters scored {r}");
    }
}
