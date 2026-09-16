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
  frames_available: number;
  pixel_source: string;
};

/// A closed time interval in epoch milliseconds.
export type Interval = { from: number; to: number };

/// A selected window, carrying the rows it was made from.
///
/// The rows travel with the window rather than being looked up later, which is
/// what lets a selection outlive the keogram it was drawn on: the operator can
/// pick an hour tonight, move the panel to last Tuesday, pick another, and have
/// both feed one scatter. `pair` records which camera pair the rows belong to,
/// because a window means nothing against a different pair.
export type Window = Interval & { pair: string; rows: KeogramRow[] };

/// Normalised: ordered, and with the ends the right way round.
function tidy<T extends Interval>(interval: T): T {
  return interval.from <= interval.to
    ? interval
    : { ...interval, from: interval.to, to: interval.from };
}

/// The rows of `rows` that fall inside the interval.
export function rowsWithin(rows: KeogramRow[], from: number, to: number): KeogramRow[] {
  const [low, high] = from <= to ? [from, to] : [to, from];
  return rows.filter(r => {
    const at = Date.parse(r.at);
    return at >= low && at <= high;
  });
}

/// Build a window over the loaded keogram, keeping the rows it covers.
export function makeWindow(pair: string, from: number, to: number, rows: KeogramRow[]): Window {
  return tidy({ pair, from, to, rows: rowsWithin(rows, from, to) });
}

/// Add a window, merging any it overlaps from the same pair. The set stays
/// disjoint and in time order, so no row is counted twice in the scatter.
export function addWindow(list: Window[], next: Window): Window[] {
  let pending = tidy(next);
  const out: Window[] = [];
  for (const current of [...list].sort((p, q) => p.from - q.from)) {
    if (current.pair !== pending.pair || current.to < pending.from || current.from > pending.to) {
      out.push(current);
    } else {
      const seen = new Set(pending.rows.map(r => r.at));
      pending = {
        pair: pending.pair,
        from: Math.min(pending.from, current.from),
        to: Math.max(pending.to, current.to),
        rows: [...pending.rows, ...current.rows.filter(r => !seen.has(r.at))]
          .sort((a, b) => Date.parse(a.at) - Date.parse(b.at)),
      };
    }
  }
  out.push(pending);
  return out.sort((p, q) => p.from - q.from);
}

/// Drop whichever window contains this instant.
export function removeWindowAt(list: Window[], at: number): Window[] {
  return list.filter(w => !(at >= w.from && at <= w.to));
}

/// Which end of which window is within `tolerance` of this instant, nearest
/// first. Returned rather than acted on so the caller can show a grab handle
/// before anything moves.
export function edgeNear(
  list: Window[],
  at: number,
  tolerance: number,
): { index: number; edge: 'from' | 'to' } | null {
  type Hit = { index: number; edge: 'from' | 'to'; distance: number };
  let best: Hit | null = null;
  for (let index = 0; index < list.length; index++) {
    for (const edge of ['from', 'to'] as const) {
      const distance = Math.abs(list[index][edge] - at);
      if (distance <= tolerance && (best === null || distance < best.distance)) {
        best = { index, edge, distance };
      }
    }
  }
  return best === null ? null : { index: best.index, edge: best.edge };
}

/// Move one end of one window to a new instant, in either direction.
///
/// Dragging an end past the other flips them rather than collapsing the window
/// to nothing, which is what a hand actually does when it overshoots. `rows`
/// is the currently loaded keogram: a window can only gain rows that are on
/// screen, so growing an edge beyond what is loaded moves the boundary and
/// keeps the rows it already had.
export function resizeWindow(
  list: Window[],
  index: number,
  edge: 'from' | 'to',
  at: number,
  rows: KeogramRow[],
): Window[] {
  if (index < 0 || index >= list.length) return list;
  const target = list[index];
  const moved = tidy({ ...target, [edge]: at } as Window);
  const seen = new Set<string>();
  const kept = [...rowsWithin(rows, moved.from, moved.to), ...rowsWithin(target.rows, moved.from, moved.to)]
    .filter(r => (seen.has(r.at) ? false : (seen.add(r.at), true)))
    .sort((a, b) => Date.parse(a.at) - Date.parse(b.at));
  const rest = list.filter((_, n) => n !== index);
  return addWindow(rest, { ...moved, rows: kept });
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

/// Paired intensities from every selected window, one set per colour channel.
///
/// Only samples both cameras saw contribute. A sample missing from either mesh
/// is dropped rather than read as zero, which would stack a false cloud of
/// points on the axes and drag any line fitted through them towards the origin.
export function scatterPoints(windows: { rows: KeogramRow[] }[]): ScatterSet[] {
  const sets: ScatterSet[] = CHANNELS.map(channel => ({ channel, points: [] }));
  const seen = new Set<string>();
  for (const window of windows) {
    for (const row of window.rows) {
      // Overlapping windows are merged, but a row could still arrive twice if
      // a caller passes the live keogram alongside a stored window.
      if (seen.has(row.at)) continue;
      seen.add(row.at);
      const width = Math.min(row.a.length, row.b.length);
      for (let i = 0; i < width; i++) {
        const a = row.a[i], b = row.b[i];
        if (!a || !b) continue;
        for (let c = 0; c < 3; c++) sets[c].points.push([a[c], b[c]]);
      }
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
