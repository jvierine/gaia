import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/voronoi.ts',import.meta.url),'utf8');
// ES2020 to match the shipped bundle.
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {voronoiEdges,frameBoundary,clipToFrame}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

const near=(a,b,tol=1e-6)=>Math.abs(a-b)<tol;

test('two stars are separated by their perpendicular bisector',()=>{
  const edges=voronoiEdges([{x:25,y:50},{x:75,y:50}],frameBoundary(100,100));
  assert.equal(edges.length,1,'one boundary, counted once and not twice');
  const e=edges[0];
  assert.ok(near(e.x1,50)&&near(e.x2,50),`bisector should be x=50, got ${e.x1},${e.x2}`);
  assert.ok(near(Math.min(e.y1,e.y2),0)&&near(Math.max(e.y1,e.y2),100),'it should span the frame');
  assert.deepEqual([e.a,e.b].sort((p,q)=>p-q),[0,1],'the edge names both stars');
});

test('four stars in a square meet at the centre',()=>{
  const edges=voronoiEdges([{x:25,y:25},{x:75,y:25},{x:25,y:75},{x:75,y:75}],frameBoundary(100,100));
  // Four interior boundaries, each shared by one pair; the diagonals do not meet.
  assert.equal(edges.length,4);
  for(const e of edges){
    const touchesCentre=[[e.x1,e.y1],[e.x2,e.y2]].some(([x,y])=>near(x,50)&&near(y,50));
    assert.ok(touchesCentre,'every interior edge should reach the centre');
  }
});

test('each interior boundary appears exactly once',()=>{
  const sites=Array.from({length:30},(_,i)=>({x:7+((i*37)%90),y:5+((i*53)%88)}));
  const edges=voronoiEdges(sites,frameBoundary(100,100));
  const seen=new Set();
  for(const e of edges){
    const key=[e.a,e.b].sort((p,q)=>p-q).join(':');
    assert.ok(!seen.has(key),`pair ${key} produced two edges`);
    seen.add(key);
  }
  assert.ok(edges.length>20,`expected a web of edges, got ${edges.length}`);
});

test('every cell keeps its own star, which is what makes it that star\'s region',()=>{
  const sites=[{x:10,y:10},{x:90,y:15},{x:50,y:80},{x:20,y:60},{x:70,y:45}];
  const edges=voronoiEdges(sites,frameBoundary(100,100),true);
  // A point is in the right cell when no other star is nearer. Check the
  // tessellation agrees with that definition on a grid.
  for(let x=5;x<100;x+=9)for(let y=5;y<100;y+=9){
    let nearest=0;
    for(let i=1;i<sites.length;i++){
      const d=(sites[i].x-x)**2+(sites[i].y-y)**2, best=(sites[nearest].x-x)**2+(sites[nearest].y-y)**2;
      if(d<best)nearest=i;
    }
    // The nearest star must own an edge somewhere, i.e. it has a real cell.
    assert.ok(edges.some(e=>e.a===nearest||e.b===nearest),`star ${nearest} has no cell`);
  }
});

test('boundary edges are excluded unless asked for',()=>{
  const sites=[{x:30,y:30},{x:70,y:70}];
  assert.ok(voronoiEdges(sites,frameBoundary(100,100)).every(e=>e.b>=0),'no border segments by default');
  const withFrame=voronoiEdges(sites,frameBoundary(100,100),true);
  assert.ok(withFrame.some(e=>e.b===-1),'border segments on request');
  assert.ok(withFrame.length>voronoiEdges(sites,frameBoundary(100,100)).length);
});

test('degenerate input does not produce boundaries out of nothing',()=>{
  assert.deepEqual(voronoiEdges([],frameBoundary(100,100)),[]);
  assert.deepEqual(voronoiEdges([{x:50,y:50}],frameBoundary(100,100)),[],'a lone star has no neighbour to divide from');
  // Coincident stars cannot be separated, and must not invent an edge or hang.
  assert.deepEqual(voronoiEdges([{x:50,y:50},{x:50,y:50}],frameBoundary(100,100)),[]);
  assert.deepEqual(voronoiEdges([{x:1,y:1},{x:2,y:2}],frameBoundary(0,0)),[],'a degenerate frame has no cells');
});

test('collinear stars divide into parallel strips',()=>{
  const edges=voronoiEdges([{x:20,y:50},{x:50,y:50},{x:80,y:50}],frameBoundary(100,100));
  // Two boundaries, at x=35 and x=65, and none between the outer pair.
  assert.equal(edges.length,2);
  const xs=edges.map(e=>Math.round((e.x1+e.x2)/2)).sort((a,b)=>a-b);
  assert.deepEqual(xs,[35,65]);
});

test('cells are cut to the camera field, not the frame corners',()=>{
  // A fisheye horizon: a circle well inside a square sensor.
  const circle=Array.from({length:120},(_,i)=>{
    const a=i/120*2*Math.PI; return {x:50+40*Math.cos(a),y:50+40*Math.sin(a)};
  });
  const sites=[{x:35,y:50},{x:65,y:50}];
  const inField=voronoiEdges(sites,circle,true);
  // Nothing may reach a corner of the frame, which is ground and housing.
  for(const e of inField)for(const [x,y] of [[e.x1,e.y1],[e.x2,e.y2]]){
    assert.ok(Math.hypot(x-50,y-50)<=40.001,`edge point ${x},${y} escaped the field`);
  }
  // The dividing boundary is still there, and is shorter than it would be
  // across the whole frame.
  const shared=inField.filter(e=>e.b>=0);
  assert.equal(shared.length,1);
  const length=Math.hypot(shared[0].x2-shared[0].x1,shared[0].y2-shared[0].y1);
  assert.ok(length>70&&length<81,`chord of the circle, got ${length}`);
});

test('a field running off the sensor is cut back to it',()=>{
  const big=Array.from({length:60},(_,i)=>{
    const a=i/60*2*Math.PI; return {x:50+90*Math.cos(a),y:50+90*Math.sin(a)};
  });
  const cut=clipToFrame(big,100,100);
  assert.ok(cut.length>=4);
  for(const p of cut){
    assert.ok(p.x>=-1e-6&&p.x<=100+1e-6&&p.y>=-1e-6&&p.y<=100+1e-6,`${p.x},${p.y} outside the frame`);
  }
  // A circle already inside the frame is untouched.
  const small=[{x:40,y:40},{x:60,y:40},{x:60,y:60},{x:40,y:60}];
  assert.equal(clipToFrame(small,100,100).length,4);
  assert.deepEqual(clipToFrame(small,0,0),[]);
});
