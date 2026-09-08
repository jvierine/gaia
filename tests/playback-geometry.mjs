// Read-only integration check: adaptive tessellation must not change mask coverage.
// Usage: node tests/playback-geometry.mjs OLD_GEOMETRY_URL NEW_GEOMETRY_URL
const [oldUrl, newUrl] = process.argv.slice(2);
if (!oldUrl || !newUrl) throw new Error('Supply old and new XYZUV geometry URLs');
async function read(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${response.status}: ${url}`);
  return new Float32Array(await response.arrayBuffer());
}
const [oldMesh, newMesh] = await Promise.all([read(oldUrl), read(newUrl)]);
const width = Math.round(1 / (oldMesh[8] - oldMesh[3]));
const height = Math.round(1 / (oldMesh[14] - oldMesh[4]));
function coverage(mesh) {
  const covered = new Set();
  for (let i = 0; i < mesh.length; i += 15) {
    const p = [0, 5, 10].map(j => [mesh[i + j + 3] * width, mesh[i + j + 4] * height]);
    const cross = (a, b, x, y) => (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
    for (let y = Math.floor(Math.min(...p.map(v => v[1]))); y < Math.ceil(Math.max(...p.map(v => v[1]))); y++) {
      for (let x = Math.floor(Math.min(...p.map(v => v[0]))); x < Math.ceil(Math.max(...p.map(v => v[0]))); x++) {
        const sides = p.map((a, k) => cross(a, p[(k + 1) % 3], x + .5, y + .5));
        if (sides.every(v => v >= -1e-6) || sides.every(v => v <= 1e-6)) covered.add(y * width + x);
      }
    }
  }
  return covered;
}
const a = coverage(oldMesh), b = coverage(newMesh);
if (a.size !== b.size || [...a].some(pixel => !b.has(pixel))) throw new Error('Mask coverage changed');
console.log(JSON.stringify({pixels: a.size, oldVertices: oldMesh.length / 5, newVertices: newMesh.length / 5, reduction: oldMesh.length / newMesh.length, maskCoverage: 'identical'}));
