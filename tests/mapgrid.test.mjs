import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/mapgrid.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {WHOLE_WORLD,clampView,zoomAt,graticuleStep,gridLines,formatLatitude,formatLongitude,countryPaths}=
  await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

test('a view cannot wander off the world',()=>{
  const off=clampView({lon:-400,lat:200,lonSpan:60,latSpan:30});
  assert.ok(off.lon>=-180&&off.lon+off.lonSpan<=180,`lon ${off.lon}..${off.lon+off.lonSpan}`);
  assert.ok(off.lat>=-90&&off.lat+off.latSpan<=90,`lat ${off.lat}..${off.lat+off.latSpan}`);
  const huge=clampView({lon:-180,lat:-90,lonSpan:900,latSpan:900});
  assert.deepEqual([huge.lonSpan,huge.latSpan],[360,180],'cannot zoom out past the world');
  const tiny=clampView({lon:0,lat:0,lonSpan:0,latSpan:0});
  assert.ok(tiny.lonSpan>0&&tiny.latSpan>0,'cannot collapse to nothing');
});

test('zooming keeps the point under the pointer',()=>{
  const start=clampView({lon:0,lat:50,lonSpan:40,latSpan:20});
  const [atLon,atLat]=[25,62];
  const zoomed=zoomAt(start,2,atLon,atLat);
  const before=[(atLon-start.lon)/start.lonSpan,(atLat-start.lat)/start.latSpan];
  const after=[(atLon-zoomed.lon)/zoomed.lonSpan,(atLat-zoomed.lat)/zoomed.latSpan];
  assert.ok(Math.abs(before[0]-after[0])<1e-9,`x moved ${before[0]} -> ${after[0]}`);
  assert.ok(Math.abs(before[1]-after[1])<1e-9,`y moved ${before[1]} -> ${after[1]}`);
  assert.ok(zoomed.lonSpan<start.lonSpan,'zooming in narrows the view');
  // Zooming out from the whole world stays the whole world.
  assert.deepEqual(zoomAt(WHOLE_WORLD,0.5,0,0).lonSpan,360);
});

test('the graticule thins out as the view widens',()=>{
  assert.equal(graticuleStep(360),30);
  assert.equal(graticuleStep(40),10);
  assert.equal(graticuleStep(8),2);
  assert.equal(graticuleStep(2),0.5);
  // Every span gives between four and about twelve lines, which is the point.
  for(const span of [360,180,90,40,20,8,3,1,0.4,0.2]){
    const n=gridLines(0,span,graticuleStep(span)).length;
    assert.ok(n>=4&&n<=13,`span ${span} gave ${n} lines`);
  }
});

test('grid lines land on multiples and do not drift',()=>{
  // -7 to 13, so the multiples of five inside it are these four; 15 is past
  // the eastern edge and must not appear.
  assert.deepEqual(gridLines(-7,20,5),[-5,0,5,10]);
  assert.deepEqual(gridLines(0,20,5),[0,5,10,15,20],'both edges are inclusive');
  // Floating-point accumulation must not produce 17.999999999 as a label.
  for(const v of gridLines(0,3,0.1)) assert.equal(v,Number(v.toFixed(6)));
});

test('labels name the hemisphere and carry enough decimals',()=>{
  assert.equal(formatLatitude(69.6,1),'70°N','whole degrees still name the hemisphere');
  assert.equal(formatLatitude(69.6,0.1),'69.6°N');
  assert.equal(formatLatitude(-33.9,0.1),'33.9°S');
  assert.equal(formatLatitude(0,1),'0°','the equator has no hemisphere');
  assert.equal(formatLongitude(18.9,0.1),'18.9°E');
  assert.equal(formatLongitude(-18.9,0.1),'18.9°W');
  assert.equal(formatLongitude(0,1),'0°');
  assert.equal(formatLongitude(180,1),'180°','the antimeridian is neither east nor west');
  assert.equal(formatLongitude(-180,1),'180°');
  assert.equal(formatLongitude(190,1),'170°W','longitude wraps');
});

test('country outlines become one closed path per feature',()=>{
  const world={features:[
    {geometry:{type:'Polygon',coordinates:[[[0,0],[10,0],[10,10],[0,0]]]}},
    {geometry:{type:'MultiPolygon',coordinates:[
      [[[20,20],[30,20],[30,30],[20,20]]],
      [[[40,40],[50,40],[50,50],[40,40]]]]}},
    {geometry:{type:'Point',coordinates:[1,2]}},
    {geometry:{type:'Polygon',coordinates:[[[0,0],[1,1]]]}},
  ]};
  const paths=countryPaths(world);
  assert.equal(paths.length,2,'a point and a two-vertex ring are not outlines');
  // North is up, so latitude is negated.
  assert.ok(paths[0].startsWith('M0.000 0.000L10.000 0.000L10.000 -10.000'),paths[0]);
  assert.equal((paths[1].match(/Z/g)||[]).length,2,'each ring closes');
  assert.deepEqual(countryPaths(null),[]);
  assert.deepEqual(countryPaths({}),[]);
});
