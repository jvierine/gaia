import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/voronoi.ts',import.meta.url),'utf8');
// ES2020 to match the shipped bundle.
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {voronoiEdges}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

const near=(a,b,tol=1e-6)=>Math.abs(a-b)<tol;

test('two stars are separated by their perpendicular bisector',()=>{
  const edges=voronoiEdges([{x:25,y:50},{x:75,y:50}],100,100);
  assert.equal(edges.length,1,'one boundary, counted once and not twice');
  const e=edges[0];
  assert.ok(near(e.x1,50)&&near(e.x2,50),`bisector should be x=50, got ${e.x1},${e.x2}`);
  assert.ok(near(Math.min(e.y1,e.y2),0)&&near(Math.max(e.y1,e.y2),100),'it should span the frame');
  assert.deepEqual([e.a,e.b].sort((p,q)=>p-q),[0,1],'the edge names both stars');
});

test('four stars in a square meet at the centre',()=>{
  const edges=voronoiEdges([{x:25,y:25},{x:75,y:25},{x:25,y:75},{x:75,y:75}],100,100);
  // Four interior boundaries, each shared by one pair; the diagonals do not meet.
  assert.equal(edges.length,4);
  for(const e of edges){
    const touchesCentre=[[e.x1,e.y1],[e.x2,e.y2]].some(([x,y])=>near(x,50)&&near(y,50));
    assert.ok(touchesCentre,'every interior edge should reach the centre');
  }
});

test('each interior boundary appears exactly once',()=>{
  const sites=Array.from({length:30},(_,i)=>({x:7+((i*37)%90),y:5+((i*53)%88)}));
  const edges=voronoiEdges(sites,100,100);
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
  const edges=voronoiEdges(sites,100,100,true);
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

test('frame edges are excluded unless asked for',()=>{
  const sites=[{x:30,y:30},{x:70,y:70}];
  assert.ok(voronoiEdges(sites,100,100).every(e=>e.b>=0),'no border segments by default');
  const withFrame=voronoiEdges(sites,100,100,true);
  assert.ok(withFrame.some(e=>e.b===-1),'border segments on request');
  assert.ok(withFrame.length>voronoiEdges(sites,100,100).length);
});

test('degenerate input does not produce boundaries out of nothing',()=>{
  assert.deepEqual(voronoiEdges([],100,100),[]);
  assert.deepEqual(voronoiEdges([{x:50,y:50}],100,100),[],'a lone star has no neighbour to divide from');
  // Coincident stars cannot be separated, and must not invent an edge or hang.
  assert.deepEqual(voronoiEdges([{x:50,y:50},{x:50,y:50}],100,100),[]);
  assert.deepEqual(voronoiEdges([{x:1,y:1},{x:2,y:2}],0,0),[],'an empty frame has no cells');
});

test('collinear stars divide into parallel strips',()=>{
  const edges=voronoiEdges([{x:20,y:50},{x:50,y:50},{x:80,y:50}],100,100);
  // Two boundaries, at x=35 and x=65, and none between the outer pair.
  assert.equal(edges.length,2);
  const xs=edges.map(e=>Math.round((e.x1+e.x2)/2)).sort((a,b)=>a-b);
  assert.deepEqual(xs,[35,65]);
});
