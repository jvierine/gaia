// Which cameras to show when one station is isolated.
//
// Several cameras often stand on the same coordinates, and they are not equally
// good: the operator grades each one with a quality weight, a power of two from
// 1 down to 1/256, which the mosaic multiplies into that camera's contribution.
// Isolating a station means showing what that site sees at its best, so the
// answer is the cameras carrying the highest weight there, and nothing else.

export type StationCamera = {
  source_id?: string;
  lat: number;
  lon: number;
  /// Quality weight as its exponent, 0 for full weight down to -8 for 1/256.
  weight: number;
};

/// Degrees within which two cameras count as standing on the same spot. Shared
/// stations are registered at identical coordinates, so this only has to absorb
/// rounding, not separate nearby sites.
export const SAME_STATION_DEG = 0.005;

/// The cameras at `site` whose quality weight is the highest there.
///
/// Ties are kept together rather than broken arbitrarily: two cameras the
/// operator graded alike are equally the best view of that sky, and dropping
/// one at random would be a worse answer than showing both. Returns null when
/// nothing identifiable stands there.
export function bestAtStation(
  sites: StationCamera[],
  site: { lat: number; lon: number },
  tolerance: number = SAME_STATION_DEG,
): Set<string> | null {
  const here = sites.filter(
    c => c.source_id &&
      Math.abs(c.lat - site.lat) < tolerance &&
      Math.abs(c.lon - site.lon) < tolerance,
  );
  if (!here.length) return null;
  const best = here.reduce((top, c) => Math.max(top, c.weight), -Infinity);
  return new Set(here.filter(c => c.weight === best).map(c => c.source_id!));
}

/// Whether `solo` is exactly `next`, which is how a second press on the same
/// station is recognised as a request to restore the blended mosaic.
export function sameSelection(solo: Set<string> | null, next: Set<string> | null): boolean {
  if (!solo || !next) return false;
  if (solo.size !== next.size) return false;
  for (const id of next) if (!solo.has(id)) return false;
  return true;
}
