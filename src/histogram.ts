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

/// Tick positions across a range, at values a reader would choose: 1, 2 or 5
/// times a power of ten. Returns the values themselves, so a caller can place
/// and label them.
export function ticks(low: number, high: number, about = 5): number[] {
  if (!Number.isFinite(low) || !Number.isFinite(high) || !(high > low)) return [];
  const raw = (high - low) / Math.max(1, about);
  const magnitude = Math.pow(10, Math.floor(Math.log10(raw)));
  const step = [1, 2, 5, 10].map(m => m * magnitude).find(s => s >= raw) ?? 10 * magnitude;
  const out: number[] = [];
  for (let v = Math.ceil(low / step) * step; v <= high + step * 1e-9; v += step) {
    // Guard the accumulated error, so 0.30000000000000004 is not a tick label.
    out.push(Math.abs(v) < step * 1e-9 ? 0 : Number(v.toFixed(12)));
  }
  return out;
}

/// A short label for a tick, keeping large and small numbers readable without
/// a units column.
export function tickLabel(value: number): string {
  const size = Math.abs(value);
  if (size === 0) return '0';
  if (size >= 1e5 || size < 1e-3) return value.toExponential(0).replace('e+', 'e');
  if (size >= 100) return value.toFixed(0);
  if (size >= 10) return value.toFixed(value % 1 === 0 ? 0 : 1);
  return value.toFixed(size >= 1 ? 1 : 2);
}
