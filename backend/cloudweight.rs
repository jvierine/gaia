//! A per-pixel cloud weight for one frame, from how far its stars have faded.
//!
//! The star photometry already measures, star by star, how much dimmer a known
//! star is than it has been at its best. That is a sample of the transparency
//! of the sky along one line of sight. This turns a frame's worth of such
//! samples into a field over the whole image, so that a cloudy quarter of one
//! camera's view yields to a clear view of the same sky from a neighbour
//! instead of being averaged into it.
//!
//! **This scheme is preliminary.** See `PRELIMINARY` below: it cannot yet tell
//! a saturated star under bright aurora, where the sky is clear, from a
//! saturated star under thin moonlit cloud, where it is not. Both are excluded
//! here rather than guessed at, which biases the field towards clear.
//!
//! The field is built as follows. Each usable star owns the region of the image
//! closer to it than to any other -- its Voronoi cell -- and carries its own
//! fading factor there. Within a cell the star's own measurement is trusted
//! fully at the star and less further away, following a Hann window that
//! reaches zero at the cell boundary; at the boundary the value is the blend of
//! the two cells that meet there. That blend is spread over `BLEND_PX` image
//! pixels with the same smooth transition used everywhere else in the composite
//! weight, but only where the cell is large enough to hold such a band: inside
//! `BLEND_PX` of its own star a cell keeps its own value, since a transition
//! wider than the cell would erase the measurement it is meant to carry.
//!
//! The result is continuous everywhere, equals a star's own fading at that
//! star, and equals the mean of two neighbours on the boundary between them.

use crate::geometry::smooth_step;
use std::sync::LazyLock;

/// Why the numbers this module produces should not yet be trusted as physics.
///
/// A star vanishes from the photometry for two reasons that look identical in
/// the data and mean opposite things about the sky:
///
///   * bright aurora lifts the background until the star's peak is clipped --
///     and the sky above is **clear**;
///   * thin cloud lit by the Moon lifts the background until the star's peak is
///     clipped -- and the sky above is **not**.
///
/// Both produce a similar peak intensity and a similar background, so no rule
/// written today can separate them. Saturated fits are therefore excluded from
/// the field rather than counted either way, which is the conservative choice
/// in one direction only: it biases the archive towards believing the sky is
/// clear. Before this can be settled, the archive has to collect measured
/// responses under both conditions -- aurora over a clear sky, and moonlit thin
/// cloud -- and they have to be compared. Until then this weight is a first
/// approximation, useful for seeing the effect and wrong to quote.
pub const PRELIMINARY: &str = "Excludes saturated stars, which cannot yet be \
separated into aurora over clear sky and moonlit thin cloud; both clip a star's \
peak in the same way. Biases towards clear. Collect responses under both \
conditions before trusting the values.";

/// Width of the transition across a Voronoi boundary, in image pixels.
pub const BLEND_PX: f64 = 10.0;

/// Whether each star's value is Hann-tapered across its own cell.
///
/// **Off.** The field is then flat within a cell at that star's own fading,
/// with the smooth transition across the cell boundaries retained. Turning the
/// taper off makes the field say exactly what was measured -- this star, this
/// fading, over the region it speaks for -- and leaves the only smoothing where
/// it was asked for, at the boundaries.
///
/// The taper is kept rather than deleted because the question it answers is
/// real: a star two hundred pixels away is weaker evidence than one overhead,
/// and the taper is one way to say so. It has simply not been looked at on real
/// frames yet, and it interacts with the boundary blend in a way nobody has
/// examined. `GAIA_CLOUD_HANN=1` puts it back without a rebuild, and both
/// manifests record which was used, so a published mosaic always says which
/// field it was made with.
pub static HANN_RESPONSE: LazyLock<bool> =
    LazyLock::new(|| std::env::var("GAIA_CLOUD_HANN").as_deref() == Ok("1"));

/// How the field is built, for the manifests to publish alongside the mosaic.
pub fn description() -> String {
    let response = if *HANN_RESPONSE {
        "each star's value Hann-tapered from full at the star to none at its cell boundary"
    } else {
        "each star's value flat across its own cell (Hann response disabled)"
    };
    format!(
        "PRELIMINARY. Per pixel, from how far this frame's unsaturated stars have faded \
against their own best flux over the whole archive, and only where at least {MIN_STARS} \
such stars were measured. Each star owns its Voronoi cell, with {response}; across a \
boundary the two cells are blended over {BLEND_PX:.0} image pixels with the same smooth \
step used elsewhere in the weight chain, applied only where the cell is larger than that \
band. Floored at {FLOOR}; multiplies the weight and never the pixel value, so it enters \
the same denominator."
    )
}

/// A weight is never taken all the way to zero. With a single contributing
/// camera the per-pixel normalization divides the weight out again, so a floor
/// keeps a fully clouded lone view showing something rather than punching a
/// hole in the mosaic; the effect is confined to overlaps, which is where it
/// belongs.
pub const FLOOR: f64 = 0.02;

/// What this module needs to know about one star in one frame.
#[derive(Clone, Copy, Debug)]
pub struct StarFading {
    /// Position in the original image, in pixels.
    pub x: f64,
    pub y: f64,
    /// How bright the star is now against its own best: 1 is unfaded, 0 is gone.
    pub fading: f64,
}

/// The Hann window, on a coordinate that is 0 at the centre and 1 at the edge.
/// Chosen over a Gaussian because it reaches exactly zero at the edge, which is
/// what lets one cell hand over to the next without a discontinuity and without
/// a tail reaching across the whole image.
pub fn hann(x: f64) -> f64 {
    if !(x > 0.0) {
        return 1.0;
    }
    if x >= 1.0 {
        return 0.0;
    }
    0.5 * (1.0 + (std::f64::consts::PI * x).cos())
}

/// The two nearest stars to a point, as (index, distance).
fn two_nearest(stars: &[StarFading], x: f64, y: f64) -> (Option<(usize, f64)>, Option<(usize, f64)>) {
    let (mut first, mut second): (Option<(usize, f64)>, Option<(usize, f64)>) = (None, None);
    for (index, star) in stars.iter().enumerate() {
        let r = ((star.x - x).powi(2) + (star.y - y).powi(2)).sqrt();
        if first.is_none_or(|(_, best)| r < best) {
            second = first;
            first = Some((index, r));
        } else if second.is_none_or(|(_, next)| r < next) {
            second = Some((index, r));
        }
    }
    (first, second)
}

/// The cloud weight at one point of the image.
///
/// With no stars at all there is no measurement, and the honest answer is to
/// leave the composite exactly as it was: 1, not 0. A frame whose stars were
/// all saturated must not be silently erased from the mosaic.
pub fn weight_at(stars: &[StarFading], x: f64, y: f64) -> f64 {
    let (Some((a, r_a)), neighbour) = two_nearest(stars, x, y) else {
        return 1.0;
    };
    let f_a = stars[a].fading;
    let Some((b, r_b)) = neighbour else {
        // A single star speaks for the whole frame. Nothing to blend against.
        return f_a.clamp(FLOOR, 1.0);
    };
    let baseline = ((stars[b].x - stars[a].x).powi(2) + (stars[b].y - stars[a].y).powi(2)).sqrt();
    if !(baseline > 0.0) {
        return f_a.clamp(FLOOR, 1.0);
    }
    // Distance from this point to the perpendicular bisector of the two stars,
    // which is the Voronoi boundary between them. Non-negative inside a's cell.
    let d = (r_b * r_b - r_a * r_a) / (2.0 * baseline);
    // What the boundary itself should read: the two cells blended over
    // BLEND_PX, but only where the cell is big enough to hold that band.
    let edge = if r_a > BLEND_PX {
        let s = smooth_step((d / BLEND_PX).clamp(0.0, 1.0));
        0.5 * (1.0 + s) * f_a + 0.5 * (1.0 - s) * stars[b].fading
    } else {
        f_a
    };
    // Without the Hann response the cell is flat at its own star's fading, and
    // `edge` already carries the boundary transition: at d >= BLEND_PX the
    // smooth step is 1 and the expression above reduces to f_a exactly.
    if !*HANN_RESPONSE {
        return edge.clamp(FLOOR, 1.0);
    }
    // Hann response within the cell: the star's own value at the star, falling
    // to zero influence at the boundary, where `edge` takes over. `reach` is
    // the distance from the star to the boundary through this point.
    let reach = r_a + d.max(0.0);
    let shape = if reach > 0.0 { hann(r_a / reach) } else { 1.0 };
    (shape * f_a + (1.0 - shape) * edge).clamp(FLOOR, 1.0)
}

/// Fewest usable stars a frame must have before its field is published at all.
///
/// A single star scales an entire hemispheric image by one measurement, which
/// live turned out to be the common case rather than the rare one: on the first
/// publish, ten of sixteen fields were one value everywhere and six of those
/// dimmed the whole camera, one to seven percent. That is the module's own
/// principle violated -- one star is very nearly absence of measurement, and
/// absence must not become measurement of cloud. Three matches the floor the
/// clear-sky fit uses for the same reason, and below it the Voronoi has no
/// spatial information to carry anyway.
pub const MIN_STARS: usize = 3;

/// The channel the cloud estimate is read from. Cloud extinction is close to
/// achromatic, so the mean carries the signal with the least noise; the three
/// colour channels are kept in the archive for the checks that need them.
pub const CHANNEL: &str = "mean";

/// How many grid cells across the image the published field uses. The field is
/// smooth on the scale of the spacing between stars, which is hundreds of
/// pixels, so this is already finer than the information in it.
pub const GRID: usize = 48;

/// Each star's best flux ever recorded by this camera in this channel, which is
/// what a present-day flux is faded against.
///
/// Taken over the whole archive rather than over a window, for the same reason
/// the distributions in the panel are: a star this far north barely changes
/// elevation, so its clear-sky level is far better determined over months than
/// over hours.
pub fn best_flux(
    conn: &rusqlite::Connection,
    source_id: &str,
    channel: &str,
) -> anyhow::Result<std::collections::HashMap<String, f64>> {
    let mut q = conn.prepare(
        "SELECT star_key,max(flux) FROM star_photometry
         WHERE source_id=?1 AND channel=?2 AND flux IS NOT NULL AND flux>0
         GROUP BY star_key",
    )?;
    let rows = q.query_map(rusqlite::params![source_id, channel], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?))
    })?;
    Ok(rows.collect::<Result<std::collections::HashMap<_, _>, _>>()?)
}

/// The usable stars of one frame, with how far each has faded.
///
/// Saturated fits are excluded -- see `PRELIMINARY`, which is the whole reason
/// this module is marked provisional. So are fits indistinguishable from noise:
/// a star that cannot be measured is not evidence of cloud, it is absence of
/// evidence, and counting it as total fading would let a noisy frame erase
/// itself from the mosaic.
pub fn stars_for_frame(
    conn: &rusqlite::Connection,
    source_id: &str,
    observation_utc: &str,
    channel: &str,
    best: &std::collections::HashMap<String, f64>,
) -> anyhow::Result<Vec<StarFading>> {
    let mut q = conn.prepare(
        "SELECT star_key,
                COALESCE(centroid_x,predicted_x),COALESCE(centroid_y,predicted_y),
                flux
         FROM star_photometry
         WHERE source_id=?1 AND channel=?2 AND observation_utc=?3
           AND flux IS NOT NULL AND flux>0
           AND amplitude IS NOT NULL AND background IS NOT NULL
           AND background + amplitude < ?4
           AND amplitude_snr >= ?5 AND flux_snr >= ?5",
    )?;
    let rows = q.query_map(
        rusqlite::params![
            source_id,
            channel,
            observation_utc,
            crate::starphot::SATURATION_LEVEL,
            crate::extinction::MIN_DEPTH_SNR
        ],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<f64>>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, f64>(3)?,
            ))
        },
    )?;
    let mut out = Vec::new();
    for row in rows {
        let (key, x, y, flux) = row?;
        let (Some(x), Some(y)) = (x, y) else { continue };
        let Some(&reference) = best.get(&key) else { continue };
        if !(reference > 0.0) {
            continue;
        }
        out.push(StarFading { x, y, fading: (flux / reference).clamp(0.0, 1.0) });
    }
    Ok(out)
}

/// The field sampled on a regular grid over the image, row-major, `columns` by
/// `rows`, with cell centres at the usual half-pixel offsets.
///
/// A grid rather than a full-resolution raster because the field is smooth on
/// the scale of the distance between stars -- hundreds of pixels -- so sampling
/// it per image pixel would cost a great deal and say nothing more. Callers
/// interpolate with `sample`.
pub fn grid(
    stars: &[StarFading],
    width: f64,
    height: f64,
    columns: usize,
    rows: usize,
) -> Vec<f32> {
    let (columns, rows) = (columns.max(1), rows.max(1));
    let mut out = vec![1.0f32; columns * rows];
    if stars.is_empty() {
        return out;
    }
    for row in 0..rows {
        let y = (row as f64 + 0.5) / rows as f64 * height;
        for column in 0..columns {
            let x = (column as f64 + 0.5) / columns as f64 * width;
            out[row * columns + column] = weight_at(stars, x, y) as f32;
        }
    }
    out
}

/// Whether a frame's stars are enough to say anything about its sky. Below this
/// no field is published and the layer composites exactly as it did before.
pub fn enough(stars: &[StarFading]) -> bool {
    stars.len() >= MIN_STARS
}

/// Bilinear read of a grid at normalised image coordinates, the convention the
/// meshes carry: u across, v down, both in [0,1].
pub fn sample(grid: &[f32], columns: usize, rows: usize, u: f64, v: f64) -> f32 {
    if grid.is_empty() || columns == 0 || rows == 0 {
        return 1.0;
    }
    let fx = (u * columns as f64 - 0.5).clamp(0.0, columns as f64 - 1.0);
    let fy = (v * rows as f64 - 0.5).clamp(0.0, rows as f64 - 1.0);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(columns - 1), (y0 + 1).min(rows - 1));
    let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
    let at = |x: usize, y: usize| grid[y * columns + x] as f64;
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    (top * (1.0 - ty) + bottom * ty) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn star(x: f64, y: f64, fading: f64) -> StarFading {
        StarFading { x, y, fading }
    }

    #[test]
    fn too_few_stars_is_not_a_measurement_of_cloud() {
        // One star scaling a whole all-sky image by its own fading is what the
        // first live publish actually did, to the tune of ten frames in sixteen.
        assert!(!enough(&[]));
        assert!(!enough(&[star(100.0, 100.0, 0.07)]));
        assert!(!enough(&[star(100.0, 100.0, 0.5), star(300.0, 100.0, 0.5)]));
        assert!(enough(&[
            star(100.0, 100.0, 0.5),
            star(300.0, 100.0, 0.5),
            star(200.0, 300.0, 0.5)
        ]));
    }

    #[test]
    fn a_frame_with_no_usable_stars_changes_nothing() {
        // The composite must be left exactly as it was, not erased. A frame
        // whose stars were all saturated says nothing about cloud.
        assert_eq!(weight_at(&[], 100.0, 100.0), 1.0);
        assert!(grid(&[], 512.0, 512.0, 8, 8).iter().all(|w| *w == 1.0));
    }

    #[test]
    fn each_star_carries_its_own_fading_at_its_own_position() {
        let stars = [star(100.0, 100.0, 0.9), star(400.0, 100.0, 0.2)];
        assert!((weight_at(&stars, 100.0, 100.0) - 0.9).abs() < 1e-9);
        assert!((weight_at(&stars, 400.0, 100.0) - 0.2).abs() < 1e-9);
    }

    #[test]
    fn the_boundary_between_two_cells_reads_their_mean() {
        let stars = [star(100.0, 100.0, 1.0), star(400.0, 100.0, 0.0)];
        // Midway between them is the Voronoi boundary.
        let on_edge = weight_at(&stars, 250.0, 100.0);
        assert!((on_edge - 0.5).abs() < 1e-6, "boundary read {on_edge}");
    }

    #[test]
    fn the_field_is_continuous_across_a_boundary() {
        // The whole point of the Hann response. The field is *steep* at the
        // boundary by design -- nearly the whole range crosses in BLEND_PX --
        // so a fixed step tolerance tests nothing. Continuity is that the
        // largest step shrinks in proportion to the sampling interval; a jump
        // would stay the same size however finely it is sampled.
        let stars = [star(100.0, 100.0, 1.0), star(400.0, 100.0, 0.05)];
        let largest_step = |h: f64| {
            let mut worst: f64 = 0.0;
            let mut previous = weight_at(&stars, 200.0, 100.0);
            let steps = (100.0 / h) as i64;
            for i in 1..=steps {
                let here = weight_at(&stars, 200.0 + i as f64 * h, 100.0);
                worst = worst.max((here - previous).abs());
                previous = here;
            }
            worst
        };
        let coarse = largest_step(0.5);
        let fine = largest_step(0.05);
        assert!(fine < coarse * 0.2, "step did not shrink: {coarse} -> {fine}");
        // And an explicit look either side of the boundary itself.
        let left = weight_at(&stars, 249.999, 100.0);
        let right = weight_at(&stars, 250.001, 100.0);
        assert!((left - right).abs() < 1e-3, "jump at the boundary: {left} vs {right}");
    }

    #[test]
    fn the_field_falls_monotonically_towards_the_fainter_star() {
        let stars = [star(100.0, 100.0, 1.0), star(500.0, 100.0, 0.1)];
        let mut previous = f64::INFINITY;
        for step in 0..=80 {
            let x = 100.0 + step as f64 * 5.0;
            let here = weight_at(&stars, x, 100.0);
            assert!(here <= previous + 1e-9, "rose at x={x}: {previous} -> {here}");
            previous = here;
        }
    }

    #[test]
    fn a_cell_smaller_than_the_blend_band_keeps_its_own_value() {
        // Two stars 12 px apart: there is no room for a 10 px transition either
        // side, so neither cell is allowed to be blended away. Close to each
        // star the value has to be that star's own.
        let stars = [star(100.0, 100.0, 1.0), star(112.0, 100.0, 0.0)];
        assert!((weight_at(&stars, 100.0, 100.0) - 1.0).abs() < 1e-9);
        assert!(weight_at(&stars, 112.0, 100.0) < FLOOR + 1e-9);
        // And with room to spare, the band is used: 10 px inside the boundary
        // of a large cell the value is already most of the way to its own star.
        let wide = [star(100.0, 100.0, 1.0), star(1100.0, 100.0, 0.0)];
        assert!(weight_at(&wide, 590.0, 100.0) > 0.5);
        assert!(weight_at(&wide, 610.0, 100.0) < 0.5);
    }

    #[test]
    fn the_weight_never_reaches_zero() {
        // A hole in the mosaic is worse than a dim contribution: with one
        // contributing camera the normalization divides the weight out again.
        let stars = [star(100.0, 100.0, 0.0), star(400.0, 100.0, 0.0)];
        for x in [0.0, 100.0, 250.0, 400.0, 900.0] {
            assert!(weight_at(&stars, x, 100.0) >= FLOOR, "zero weight at {x}");
        }
    }

    #[test]
    fn with_the_taper_off_a_cell_is_flat_until_its_boundary() {
        // Two stars 1000 px apart, boundary at 600. Everything further than
        // BLEND_PX inside a cell reads that cell's own star, unchanged.
        assert!(!*HANN_RESPONSE, "the taper is expected off by default");
        let stars = [star(100.0, 100.0, 0.9), star(1100.0, 100.0, 0.2)];
        for x in [120.0, 300.0, 500.0, 585.0] {
            let w = weight_at(&stars, x, 100.0);
            assert!((w - 0.9).abs() < 1e-9, "not flat at x={x}: {w}");
        }
        // The transition is confined to the band, and centred on the boundary.
        assert!((weight_at(&stars, 600.0, 100.0) - 0.55).abs() < 1e-6);
        for x in [615.0, 800.0, 1080.0] {
            let w = weight_at(&stars, x, 100.0);
            assert!((w - 0.2).abs() < 1e-9, "not flat at x={x}: {w}");
        }
    }

    #[test]
    fn the_published_description_says_which_field_was_built() {
        let text = description();
        assert!(text.contains("Hann response disabled"), "{text}");
        assert!(text.contains("PRELIMINARY"));
        assert!(text.contains("10 image pixels"));
    }

    #[test]
    fn the_hann_window_is_one_at_the_centre_and_zero_at_the_edge() {
        assert_eq!(hann(0.0), 1.0);
        assert_eq!(hann(1.0), 0.0);
        assert!((hann(0.5) - 0.5).abs() < 1e-12);
        // Outside the support it stays zero rather than coming back up.
        assert_eq!(hann(1.5), 0.0);
        assert_eq!(hann(-0.5), 1.0);
    }

    #[test]
    fn the_grid_is_read_back_where_it_was_written() {
        let stars = [star(128.0, 128.0, 0.8), star(384.0, 384.0, 0.3)];
        let (columns, rows) = (32, 32);
        let g = grid(&stars, 512.0, 512.0, columns, rows);
        // Sampling at a star's own normalised position recovers its fading,
        // give or take the grid spacing.
        assert!((sample(&g, columns, rows, 128.0 / 512.0, 128.0 / 512.0) as f64 - 0.8).abs() < 0.06);
        assert!((sample(&g, columns, rows, 384.0 / 512.0, 384.0 / 512.0) as f64 - 0.3).abs() < 0.06);
        // Off the end of the grid it clamps rather than wrapping or panicking.
        assert!(sample(&g, columns, rows, -1.0, 2.0).is_finite());
        assert_eq!(sample(&[], 0, 0, 0.5, 0.5), 1.0);
    }
}
