// Histograms for the star photometry panel.
//
// The time series answers "what happened when"; a histogram answers "what is
// this camera's sky usually like". Both matter for cloud: a night that spends
// most of its samples at a low total intensity is a clouded night whatever the
// order the samples arrived in, and the joint distribution of peak against
// background is where auroral saturation shows itself as a diagonal wall.

export type Bins = {
  /// Count in each bin, low edge first.
  counts: number[];
  /// Value at the low edge of the first bin.
  low: number;
  /// Value at the high edge of the last bin.
  high: number;
  /// Largest count, so a caller can scale without rescanning.
  peak: number;
  /// How many finite values went in.
  total: number;
};

/// Finite values only, and only those inside `range` when one is given.
function usable(values: number[], range?: [number, number]): number[] {
  return values.filter(
    v => Number.isFinite(v) && (!range || (v >= range[0] && v <= range[1])),
  );
}

/// The span to bin over: the given range, or the data's own, widened to a
/// non-zero width so a single repeated value still produces one bin rather than
/// a division by zero.
export function extent(values: number[], range?: [number, number]): [number, number] {
  if (range && range[1] > range[0]) return range;
  const finite = usable(values);
  if (!finite.length) return [0, 1];
  let low = Math.min(...finite), high = Math.max(...finite);
  if (!(high > low)) { low -= 0.5; high += 0.5; }
  return [low, high];
}

/// One-dimensional histogram. The top edge is inclusive, so the largest value
/// lands in the last bin instead of falling outside the plot.
export function histogram1d(values: number[], bins: number, range?: [number, number]): Bins {
  const count = Math.max(1, Math.floor(bins));
  const [low, high] = extent(values, range);
  const counts = new Array(count).fill(0);
  let total = 0;
  for (const v of usable(values, range)) {
    const at = Math.min(count - 1, Math.floor(((v - low) / (high - low)) * count));
    if (at >= 0) { counts[at]++; total++; }
  }
  return { counts, low, high, peak: counts.reduce((m, c) => Math.max(m, c), 0), total };
}

export type Bins2d = {
  /// Row-major, `rows` rows of `columns`. Row 0 is the low end of y.
  counts: number[];
  columns: number;
  rows: number;
  xLow: number; xHigh: number;
  yLow: number; yHigh: number;
  peak: number;
  total: number;
};

/// Two-dimensional histogram over paired samples. Pairs where either value is
/// missing are dropped rather than counted at zero, which would pile a false
/// spike into the corner.
export function histogram2d(
  xs: number[],
  ys: number[],
  columns: number,
  rows: number,
  xRange?: [number, number],
  yRange?: [number, number],
): Bins2d {
  const nx = Math.max(1, Math.floor(columns)), ny = Math.max(1, Math.floor(rows));
  const pairs: [number, number][] = [];
  for (let i = 0; i < Math.min(xs.length, ys.length); i++) {
    if (Number.isFinite(xs[i]) && Number.isFinite(ys[i])) pairs.push([xs[i], ys[i]]);
  }
  const [xLow, xHigh] = extent(pairs.map(p => p[0]), xRange);
  const [yLow, yHigh] = extent(pairs.map(p => p[1]), yRange);
  const counts = new Array(nx * ny).fill(0);
  let total = 0;
  for (const [x, y] of pairs) {
    if (x < xLow || x > xHigh || y < yLow || y > yHigh) continue;
    const cx = Math.min(nx - 1, Math.floor(((x - xLow) / (xHigh - xLow)) * nx));
    const cy = Math.min(ny - 1, Math.floor(((y - yLow) / (yHigh - yLow)) * ny));
    counts[cy * nx + cx]++;
    total++;
  }
  return {
    counts, columns: nx, rows: ny, xLow, xHigh, yLow, yHigh,
    peak: counts.reduce((m, c) => Math.max(m, c), 0), total,
  };
}

/// Pseudo-colour for a normalised count, dark blue through green to yellow.
/// Zero is returned as null so an empty bin can be left unpainted rather than
/// drawn as the darkest colour, which would hide where there is no data at all.
export function densityColour(fraction: number): string | null {
  if (!(fraction > 0)) return null;
  // Compressed, because a joint distribution is usually dominated by one peak
  // and a linear scale would show that peak and nothing else.
  const t = Math.min(1, Math.max(0, Math.sqrt(fraction)));
  const hue = 260 - 200 * t;
  const light = 18 + 52 * t;
  return `hsl(${Math.round(hue)} ${Math.round(55 + 35 * t)}% ${Math.round(light)}%)`;
}
