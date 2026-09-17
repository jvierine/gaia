// Display levels for an 8-bit image: a black point, a white point, and the
// linear transfer between them.
//
// A star frame is mostly dark with a few bright points, so the interesting
// structure lives in the bottom few percent of the range and is invisible at
// the default stretch. Pulling the white point down is what makes the faint
// stars, the horizon and the dome edge visible at all -- and it is exactly what
// an operator judging a lens fit needs, because a star they cannot see is a
// star they cannot check.
//
// The transfer is deliberately linear and clipping rather than a gamma curve:
// the question being asked of this image is geometric, and a curve that moves
// apparent centroids would be answering a different one.

export type Levels = {
  /// Input value mapped to black, 0-255.
  min: number;
  /// Input value mapped to white, 0-255.
  max: number;
};

export const FULL_RANGE: Levels = { min: 0, max: 255 };

/// Ordered, inside the range, and never zero-width. A zero-width window would
/// divide by nothing and turn the frame into a black-and-white mask.
export function clampLevels(levels: Levels): Levels {
  const lo = Math.min(levels.min, levels.max);
  const hi = Math.max(levels.min, levels.max);
  const min = Math.max(0, Math.min(254, Math.round(lo)));
  const max = Math.min(255, Math.max(min + 1, Math.round(hi)));
  return { min, max };
}

/// The SVG `feFunc*` linear coefficients for these levels.
///
/// `feComponentTransfer` works on values already normalised to 0-1 and clamps
/// its own output, so clipping outside the window costs nothing extra:
/// `out = slope * in + intercept`, with slope the reciprocal of the window
/// width and intercept placing the black point at zero.
export function transfer(levels: Levels): { slope: number; intercept: number } {
  const { min, max } = clampLevels(levels);
  const slope = 255 / (max - min);
  const intercept = -(min / 255) * slope;
  // A black point of zero yields -0, which is harmless in an SVG attribute but
  // is not equal to 0 under Object.is and so trips any comparison downstream.
  return { slope, intercept: intercept === 0 ? 0 : intercept };
}

/// What an input value displays as, 0-255, for previewing the mapping.
export function apply(levels: Levels, value: number): number {
  const { min, max } = clampLevels(levels);
  return Math.max(0, Math.min(255, Math.round(((value - min) / (max - min)) * 255)));
}

/// Where a level sits on a bar drawn from `max` at the top to `min` at the
/// bottom, as a fraction down from the top.
export function levelToFraction(value: number): number {
  return 1 - Math.max(0, Math.min(255, value)) / 255;
}

/// The inverse, for dragging a handle.
export function fractionToLevel(fraction: number): number {
  return Math.round((1 - Math.max(0, Math.min(1, fraction))) * 255);
}

/// A stretch that suits a star frame, from the frame's own brightest star and
/// its sky. Chosen so the faintest measured star still shows: the white point
/// goes a little above the brightest peak rather than to full scale, and the
/// black point sits just under the sky rather than at zero.
export function suggest(background: number | null, peak: number | null): Levels {
  const sky = Number.isFinite(background as number) ? (background as number) : 0;
  const bright = Number.isFinite(peak as number) ? (peak as number) : 255;
  return clampLevels({
    min: Math.max(0, sky - 8),
    max: Math.max(sky + 12, Math.min(255, bright + 10)),
  });
}
