// Paired keograms and the time intervals selected from them.
//
// Two cameras sampled along their equidistant cut give two images with the same
// axes: time across, position along the cut down. What the operator does with
// them is choose *when* the pair is worth believing -- a stretch with structure,
// no cloud, no twilight -- and that choice is the whole input to the
// equalization fit. So the selection is the interesting state here, and it is
// kept as a set of disjoint intervals rather than one brush, because a night
// rarely offers a single usable stretch.

export type Sample = [number, number, number] | null;

export type KeogramRow = {
  /// Grid instant the row was built for, and the frame each camera actually
  /// contributed. They differ by less than the simultaneity tolerance.
  at: string;
  a_at: string;
  b_at: string;
  a: Sample[];
  b: Sample[];
};

export type KeogramPair = {
  a: { id: string; name: string; coverage: number };
  b: { id: string; name: string; coverage: number };
  arc_km: number[];
  rows: KeogramRow[];
  samples: number;
  cadence_seconds: number;
  separation_km: number;
  colocated: boolean;
  half_width_km: number;
  from: string;
  to: string;
  unpaired_steps: number;
  pixel_source: string;
};

/// A closed time interval in epoch milliseconds.
export type Interval = { from: number; to: number };

/// Normalised: ordered, and with the ends the right way round.
function tidy(interval: Interval): Interval {
  return interval.from <= interval.to
    ? { from: interval.from, to: interval.to }
    : { from: interval.to, to: interval.from };
}

/// Add an interval, merging anything it touches. The set stays disjoint and in
/// time order, so the scatter never counts a row twice for overlapping brushes.
export function addInterval(list: Interval[], next: Interval): Interval[] {
  const merged = tidy(next);
  const out: Interval[] = [];
  let pending = merged;
  for (const current of [...list].map(tidy).sort((p, q) => p.from - q.from)) {
    if (current.to < pending.from || current.from > pending.to) {
      out.push(current);
    } else {
      pending = {
        from: Math.min(pending.from, current.from),
        to: Math.max(pending.to, current.to),
      };
    }
  }
  out.push(pending);
  return out.sort((p, q) => p.from - q.from);
}

/// Drop whichever interval contains this instant. Clicking inside a selection
/// is how it is taken back, so nothing else needs a modifier key.
export function removeAt(list: Interval[], at: number): Interval[] {
  return list.filter(i => !(at >= i.from && at <= i.to));
}

export function inAny(list: Interval[], at: number): boolean {
  return list.some(i => at >= i.from && at <= i.to);
}

/// Total selected time, in seconds. Shown so the operator knows whether the
/// selection is long enough to fit anything with.
export function selectedSeconds(list: Interval[]): number {
  return list.reduce((sum, i) => sum + (i.to - i.from), 0) / 1000;
}

export type ScatterSet = {
  channel: 'r' | 'g' | 'b';
  /// One entry per usable sample pair: the value in A and the value in B.
  points: [number, number][];
};

const CHANNELS: ScatterSet['channel'][] = ['r', 'g', 'b'];

/// Paired intensities inside the selection, one set per colour channel.
///
/// Only samples both cameras saw contribute. A sample missing from either mesh
/// is dropped rather than read as zero, which would stack a false cloud of
/// points on the axes and drag any line fitted through them towards the origin.
export function scatterPoints(
  pair: Pick<KeogramPair, 'rows'>,
  intervals: Interval[],
): ScatterSet[] {
  const sets: ScatterSet[] = CHANNELS.map(channel => ({ channel, points: [] }));
  const everything = intervals.length === 0;
  for (const row of pair.rows) {
    const at = Date.parse(row.at);
    if (!everything && !inAny(intervals, at)) continue;
    const width = Math.min(row.a.length, row.b.length);
    for (let i = 0; i < width; i++) {
      const a = row.a[i], b = row.b[i];
      if (!a || !b) continue;
      for (let c = 0; c < 3; c++) sets[c].points.push([a[c], b[c]]);
    }
  }
  return sets;
}

/// Pixels for one side of the pair, time across and cut position down.
///
/// Returned as raw RGBA rather than drawn, so the packing can be tested without
/// a canvas. A sample neither camera covers is left fully transparent, which is
/// what distinguishes "outside the field" from "dark sky" on screen; painting
/// it black would invent a measurement.
export function keogramImage(
  rows: KeogramRow[],
  side: 'a' | 'b',
  samples: number,
): { width: number; height: number; data: Uint8ClampedArray } {
  const width = Math.max(1, rows.length);
  const height = Math.max(1, samples);
  const data = new Uint8ClampedArray(width * height * 4);
  for (let x = 0; x < rows.length; x++) {
    const column = rows[x][side];
    for (let y = 0; y < height; y++) {
      const value = column[y];
      const at = (y * width + x) * 4;
      if (!value) continue;
      data[at] = value[0];
      data[at + 1] = value[1];
      data[at + 2] = value[2];
      data[at + 3] = 255;
    }
  }
  return { width, height, data };
}

/// Where a row sits horizontally, as a fraction of the panel. Rows are on a
/// regular grid, so this is linear in time and a pixel maps back to an instant
/// the same way.
export function timeToFraction(rows: KeogramRow[], at: number): number {
  if (rows.length < 2) return 0;
  const first = Date.parse(rows[0].at), last = Date.parse(rows[rows.length - 1].at);
  if (!(last > first)) return 0;
  return (at - first) / (last - first);
}

export function fractionToTime(rows: KeogramRow[], fraction: number): number {
  if (!rows.length) return 0;
  const first = Date.parse(rows[0].at);
  if (rows.length < 2) return first;
  const last = Date.parse(rows[rows.length - 1].at);
  return first + fraction * (last - first);
}
