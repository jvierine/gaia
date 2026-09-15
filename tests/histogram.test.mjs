import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/histogram.ts',import.meta.url),'utf8');
// ES2020 to match the shipped bundle.
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {histogram1d,histogram2d,extent,densityColour}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

test('every sample lands in exactly one bin',()=>{
  const values=[0,1,2,3,4,5,6,7,8,9,10];
  const h=histogram1d(values,5);
  assert.equal(h.total,values.length);
  assert.equal(h.counts.reduce((a,b)=>a+b,0),values.length);
  // The largest value belongs in the last bin, not outside the plot.
  assert.ok(h.counts[h.counts.length-1]>0);
  assert.equal(h.low,0); assert.equal(h.high,10);
});

test('counts land where the values are',()=>{
  // Everything in the bottom fifth.
  const h=histogram1d([0,0.1,0.2,10],5,[0,10]);
  assert.equal(h.counts[0],3);
  assert.equal(h.counts[4],1);
  assert.equal(h.peak,3);
});

test('values outside a given range are dropped, not clamped into the edge bins',()=>{
  const h=histogram1d([-5,0,5,10,15],4,[0,10]);
  assert.equal(h.total,3,'the two outside values must not be counted');
  assert.equal(h.counts.reduce((a,b)=>a+b,0),3);
});

test('degenerate input yields an empty histogram rather than a crash',()=>{
  const empty=histogram1d([],6);
  assert.equal(empty.total,0);
  assert.equal(empty.peak,0);
  assert.equal(empty.counts.length,6);
  // A single repeated value still needs a bin of non-zero width.
  const flat=histogram1d([4,4,4],3);
  assert.equal(flat.total,3);
  assert.ok(flat.high>flat.low);
  // Nulls and NaNs are not data.
  assert.equal(histogram1d([NaN,Infinity,1],4).total,1);
});

test('the two-dimensional histogram bins both axes independently',()=>{
  // x low / y high, and x high / y low: opposite corners, one count each.
  const h=histogram2d([0,10],[10,0],2,2,[0,10],[0,10]);
  assert.equal(h.total,2);
  assert.equal(h.columns,2); assert.equal(h.rows,2);
  // Row 0 is the low end of y, so the pairs land in opposite corners.
  assert.equal(h.counts[0*2+0],0);
  assert.equal(h.counts[0*2+1],1,'x high, y low');
  assert.equal(h.counts[1*2+0],1,'x low, y high');
  assert.equal(h.counts[1*2+1],0);
});

test('a pair with either value missing is dropped, not counted at zero',()=>{
  const h=histogram2d([1,NaN,3],[1,2,NaN],4,4);
  assert.equal(h.total,1,'only the complete pair counts');
});

test('the density ramp leaves empty bins unpainted',()=>{
  assert.equal(densityColour(0),null);
  assert.equal(densityColour(-1),null);
  assert.ok(densityColour(0.5).startsWith('hsl('));
  // Denser bins read lighter, which is what makes the peak visible.
  const light=s=>Number(s.match(/([0-9]+)%\)$/)[1]);
  assert.ok(light(densityColour(1))>light(densityColour(0.1)));
});

test('extent widens a degenerate span and honours a given one',()=>{
  assert.deepEqual(extent([5,5,5]),[4.5,5.5]);
  assert.deepEqual(extent([1,2,3],[0,10]),[0,10]);
  assert.deepEqual(extent([]),[0,1]);
});
