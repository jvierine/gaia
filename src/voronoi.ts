// Voronoi tessellation over the stars identified in one frame.
//
// Each star owns the region of the image closer to it than to any other star,
// so the cell boundaries are where responsibility for the sky passes from one
// star to the next. That makes them the natural place to read a cloud estimate:
// an edge is only as trustworthy as the dimmer of the two stars that meet
// across it, which is why every edge carries both owners rather than one.
//
// The cells are built by clipping, not by a Delaunay dual: start each cell as
// the whole region the camera sees and cut it with the perpendicular bisector
// against every other star. That is O(n^2) in the number of stars, which for
// the couple of hundred a frame yields is nothing, it is exact at the boundary
// whatever shape that is, and it is robust against the collinear and coincident
// stars a real catalogue produces.

export type Site = { x: number; y: number };

/// One boundary segment. `a` and `b` index the two stars that meet across it;
/// `b` is -1 where the cell is closed by the edge of the frame rather than by
/// another star.
export type VoronoiEdge = {
  x1: number; y1: number;
  x2: number; y2: number;
  a: number; b: number;
};

type Vertex = { x: number; y: number; owner: number };

/// Sub-pixel tolerance. Below this a vertex is treated as lying on the cut,
/// which keeps coincident and collinear stars from producing slivers.
const EPSILON = 1e-9;

/// Cuts a polygon down to the frame rectangle. A fisheye whose horizon circle
/// runs off the sensor needs this; one whose circle sits inside the frame is
/// returned untouched.
export function clipToFrame(polygon: Site[], width: number, height: number): Site[] {
  if (!(width > 0) || !(height > 0)) return [];
  let poly = polygon;
  const sides: [(p: Site) => number, (a: Site, b: Site, t: number) => Site][] = [
    [p => p.x, (a, b, t) => ({ x: 0, y: a.y + t * (b.y - a.y) })],
    [p => width - p.x, (a, b, t) => ({ x: width, y: a.y + t * (b.y - a.y) })],
    [p => p.y, (a, b, t) => ({ x: a.x + t * (b.x - a.x), y: 0 })],
    [p => height - p.y, (a, b, t) => ({ x: a.x + t * (b.x - a.x), y: height })],
  ];
  for (const [inside, meet] of sides) {
    const next: Site[] = [];
    for (let k = 0; k < poly.length; k++) {
      const cur = poly[k], following = poly[(k + 1) % poly.length];
      const curAt = inside(cur), nextAt = inside(following);
      if (curAt >= 0) next.push(cur);
      if ((curAt >= 0) !== (nextAt >= 0)) next.push(meet(cur, following, curAt / (curAt - nextAt)));
    }
    poly = next;
    if (!poly.length) return [];
  }
  return poly;
}

/// The rectangle of a frame, as a boundary polygon.
export function frameBoundary(width: number, height: number): Site[] {
  return [{ x: 0, y: 0 }, { x: width, y: 0 }, { x: width, y: height }, { x: 0, y: height }];
}

/// The cell belonging to `sites[index]`, clipped to `boundary`, as a ring of
/// vertices where each carries the star that owns the edge leaving it.
function cell(sites: Site[], index: number, boundary: Site[]): Vertex[] {
  const here = sites[index];
  let poly: Vertex[] = boundary.map(p => ({ x: p.x, y: p.y, owner: -1 }));
  for (let other = 0; other < sites.length && poly.length; other++) {
    if (other === index) continue;
    const there = sites[other];
    const dx = there.x - here.x, dy = there.y - here.y;
    // Coincident stars cannot be separated; keeping the first is the only
    // answer that does not invent a boundary.
    if (Math.abs(dx) < EPSILON && Math.abs(dy) < EPSILON) continue;
    // Keep the half-plane closer to `here`: dot(q,d) <= dot(midpoint,d).
    const limit = ((here.x + there.x) * dx + (here.y + there.y) * dy) / 2;
    const at = (v: Vertex) => v.x * dx + v.y * dy - limit;
    const next: Vertex[] = [];
    for (let k = 0; k < poly.length; k++) {
      const cur = poly[k], following = poly[(k + 1) % poly.length];
      const curAt = at(cur), nextAt = at(following);
      const curIn = curAt <= EPSILON, nextIn = nextAt <= EPSILON;
      if (curIn) next.push(cur);
      if (curIn !== nextIn) {
        const t = curAt / (curAt - nextAt);
        const crossing = {
          x: cur.x + t * (following.x - cur.x),
          y: cur.y + t * (following.y - cur.y),
          // Leaving the kept side, the boundary runs along this cut and so
          // belongs to `other`; entering it, the original edge resumes.
          owner: curIn ? other : cur.owner,
        };
        next.push(crossing);
      }
    }
    poly = next;
  }
  return poly;
}

/// Every boundary segment of the tessellation, each appearing once.
///
/// `boundary` is the region the cells are cut to: the frame rectangle for a
/// rectilinear camera, the projected horizon for a fisheye. Cutting to the
/// horizon matters because a fisheye's corners are not sky at all, and a cell
/// running out there claims ground the star never saw.
///
/// `includeBoundary` adds the segments where a cell is closed by that region
/// rather than by another star. Those have no second owner and no cloud
/// meaning, so they are left out by default.
export function voronoiEdges(
  sites: Site[],
  boundary: Site[],
  includeBoundary = false,
): VoronoiEdge[] {
  const edges: VoronoiEdge[] = [];
  if (boundary.length < 3) return edges;
  for (let index = 0; index < sites.length; index++) {
    const poly = cell(sites, index, boundary);
    for (let k = 0; k < poly.length; k++) {
      const from = poly[k], to = poly[(k + 1) % poly.length];
      const other = from.owner;
      // A shared edge is found from both of its cells; take it from the lower
      // index so it is drawn once.
      if (other >= 0 && other < index) continue;
      if (other < 0 && !includeBoundary) continue;
      if (Math.abs(to.x - from.x) < EPSILON && Math.abs(to.y - from.y) < EPSILON) continue;
      edges.push({ x1: from.x, y1: from.y, x2: to.x, y2: to.y, a: index, b: other });
    }
  }
  return edges;
}
