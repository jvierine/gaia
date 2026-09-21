// Graticule, tick labels and country outlines for the equirectangular location
// picker.
//
// The map is drawn in degrees: x is longitude, y is *negative* latitude so that
// north is up. Working in the data's own units means the marker, the grid and
// the borders all share one coordinate system and no conversion can drift
// between them; the SVG viewBox does the zooming.

export type View = {
  /// West and east edges, in degrees.
  lon: number;
  lonSpan: number;
  /// South and north edges, in degrees.
  lat: number;
  latSpan: number;
};

export const WHOLE_WORLD: View = { lon: -180, lonSpan: 360, lat: -90, latSpan: 180 };

/// Tightest view worth allowing: a tenth of a degree is about 11 km, finer than
/// any station position is known.
const MIN_SPAN = 0.1;

/// Keep a view inside the world and the right way up. Panning off the edge is
/// the commonest way a map like this ends up showing nothing at all.
export function clampView(view: View): View {
  const lonSpan = Math.min(360, Math.max(MIN_SPAN, view.lonSpan));
  const latSpan = Math.min(180, Math.max(MIN_SPAN / 2, view.latSpan));
  return {
    lonSpan,
    latSpan,
    lon: Math.min(180 - lonSpan, Math.max(-180, view.lon)),
    lat: Math.min(90 - latSpan, Math.max(-90, view.lat)),
  };
}

/// Zoom by `factor` about a point given in degrees, so the place under the
/// pointer stays under the pointer.
export function zoomAt(view: View, factor: number, atLon: number, atLat: number): View {
  const lonSpan = view.lonSpan / factor;
  const latSpan = view.latSpan / factor;
  const fx = (atLon - view.lon) / view.lonSpan;
  const fy = (atLat - view.lat) / view.latSpan;
  return clampView({
    lon: atLon - fx * lonSpan,
    lat: atLat - fy * latSpan,
    lonSpan,
    latSpan,
  });
}

/// Spacing between graticule lines for a given span, from a fixed ladder of
/// values a reader expects to see on a map. Chosen so a view always carries
/// roughly four to twelve lines: fewer gives nothing to judge position by,
/// more turns the map into graph paper.
export function graticuleStep(spanDeg: number): number {
  const ladder = [30, 15, 10, 5, 2, 1, 0.5, 0.2, 0.1, 0.05, 0.02, 0.01];
  for (const step of ladder) {
    if (spanDeg / step >= 4) return step;
  }
  return ladder[ladder.length - 1];
}

/// Graticule values across a range, at multiples of `step`.
export function gridLines(low: number, span: number, step: number): number[] {
  const out: number[] = [];
  if (!(span > 0) || !(step > 0)) return out;
  const first = Math.ceil(low / step) * step;
  for (let v = first; v <= low + span + step * 1e-9; v += step) {
    // Guard the accumulated error so 17.999999999 is not a label.
    out.push(Number(v.toFixed(6)));
  }
  return out;
}

/// How many decimals a label needs at this spacing, so 0.5 steps do not all
/// read as the same whole degree.
function decimalsFor(step: number): number {
  if (step >= 1) return 0;
  if (step >= 0.1) return 1;
  if (step >= 0.01) return 2;
  return 3;
}

export function formatLatitude(value: number, step = 1): string {
  const decimals = decimalsFor(step);
  if (Math.abs(value) < Math.pow(10, -decimals) / 2) return '0°';
  return `${Math.abs(value).toFixed(decimals)}°${value > 0 ? 'N' : 'S'}`;
}

export function formatLongitude(value: number, step = 1): string {
  const decimals = decimalsFor(step);
  const wrapped = ((value + 180) % 360 + 360) % 360 - 180;
  if (Math.abs(wrapped) < Math.pow(10, -decimals) / 2) return '0°';
  if (Math.abs(Math.abs(wrapped) - 180) < Math.pow(10, -decimals) / 2) return '180°';
  return `${Math.abs(wrapped).toFixed(decimals)}°${wrapped > 0 ? 'E' : 'W'}`;
}

type Ring = number[][];
type Geometry = { type: string; coordinates: unknown };
type Feature = { geometry?: Geometry };
type Collection = { features?: Feature[] };

/// Country outlines as SVG path data, in the map's own degree coordinates.
///
/// Rings are emitted as closed subpaths of one path per feature, which keeps
/// the element count near the number of countries rather than the number of
/// islands. Coordinates are rounded to three decimals -- about a hundred metres
/// -- because a border drawn finer than that is invisible at any zoom this
/// picker allows and only costs bytes.
export function countryPaths(world: Collection | null | undefined): string[] {
  const out: string[] = [];
  for (const feature of world?.features ?? []) {
    const geometry = feature?.geometry;
    if (!geometry) continue;
    const polygons: Ring[][] =
      geometry.type === 'Polygon'
        ? [geometry.coordinates as Ring[]]
        : geometry.type === 'MultiPolygon'
          ? (geometry.coordinates as Ring[][])
          : [];
    let d = '';
    for (const polygon of polygons) {
      for (const ring of polygon) {
        if (!Array.isArray(ring) || ring.length < 3) continue;
        ring.forEach((position, index) => {
          const lon = Number(position?.[0]);
          const lat = Number(position?.[1]);
          if (!Number.isFinite(lon) || !Number.isFinite(lat)) return;
          // y is negative latitude: north up.
          d += `${index === 0 ? 'M' : 'L'}${lon.toFixed(3)} ${(-lat).toFixed(3)}`;
        });
        d += 'Z';
      }
    }
    if (d) out.push(d);
  }
  return out;
}
