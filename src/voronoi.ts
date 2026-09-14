// Voronoi tessellation over the stars identified in one frame.
//
// Each star owns the region of the image closer to it than to any other star,
// so the cell boundaries are where responsibility for the sky passes from one
// star to the next. That makes them the natural place to read a cloud estimate:
// an edge is only as trustworthy as the dimmer of the two stars that meet
// across it, which is why every edge carries both owners rather than one.
//
// The cells are built by clipping, not by a Delaunay dual: start each cell as
// the whole frame and cut it with the perpendicular bisector against every
// other star. That is O(n^2) in the number of stars, which for the couple of
// hundred a frame yields is nothing, and it is exact at the frame border and
// robust against the collinear and coincident stars a real catalogue produces.

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

/// The cell belonging to `sites[index]`, clipped to the frame, as a ring of
/// vertices where each carries the star that owns the edge leaving it.
function cell(sites: Site[], index: number, width: number, height: number): Vertex[] {
  const here = sites[index];
  let poly: Vertex[] = [
    { x: 0, y: 0, owner: -1 },
    { x: width, y: 0, owner: -1 },
    { x: width, y: height, owner: -1 },
    { x: 0, y: height, owner: -1 },
  ];
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
/// `includeFrame` adds the segments where a cell is closed by the edge of the
/// image. Those have no second owner and no cloud meaning, so they are left out
/// by default.
export function voronoiEdges(
  sites: Site[],
  width: number,
  height: number,
  includeFrame = false,
): VoronoiEdge[] {
  const edges: VoronoiEdge[] = [];
  if (!(width > 0) || !(height > 0)) return edges;
  for (let index = 0; index < sites.length; index++) {
    const poly = cell(sites, index, width, height);
    for (let k = 0; k < poly.length; k++) {
      const from = poly[k], to = poly[(k + 1) % poly.length];
      const other = from.owner;
      // A shared edge is found from both of its cells; take it from the lower
      // index so it is drawn once.
      if (other >= 0 && other < index) continue;
      if (other < 0 && !includeFrame) continue;
      if (Math.abs(to.x - from.x) < EPSILON && Math.abs(to.y - from.y) < EPSILON) continue;
      edges.push({ x1: from.x, y1: from.y, x2: to.x, y2: to.y, a: index, b: other });
    }
  }
  return edges;
}
