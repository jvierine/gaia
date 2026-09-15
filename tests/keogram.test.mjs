import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/keogram.ts',import.meta.url),'utf8');
// ES2020 to match the shipped bundle: the default target compiles for..of over
// a Set to an index loop that never runs, which has bitten this suite before.
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {addInterval,removeAt,inAny,selectedSeconds,scatterPoints,keogramImage,
       timeToFraction,fractionToTime}=
  await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

const T=t=>Date.parse('2026-09-12T20:00:00Z')+t*1000;

test('overlapping selections merge instead of double counting',()=>{
  let list=addInterval([],{from:T(0),to:T(100)});
  list=addInterval(list,{from:T(50),to:T(200)});
  assert.equal(list.length,1,'two overlapping brushes are one interval');
  assert.deepEqual(list[0],{from:T(0),to:T(200)});
  list=addInterval(list,{from:T(400),to:T(500)});
  assert.equal(list.length,2,'a disjoint brush stays separate');
  // Touching intervals join; a gap of one millisecond does not.
  assert.equal(addInterval(list,{from:T(200),to:T(400)}).length,1);
});

test('a brush dragged backwards still selects the interval',()=>{
  const list=addInterval([],{from:T(300),to:T(100)});
  assert.deepEqual(list[0],{from:T(100),to:T(300)});
  assert.ok(inAny(list,T(200)));
});

test('clicking inside a selection takes it back',()=>{
  const list=addInterval(addInterval([],{from:T(0),to:T(100)}),{from:T(400),to:T(500)});
  const left=removeAt(list,T(50));
  assert.equal(left.length,1);
  assert.deepEqual(left[0],{from:T(400),to:T(500)});
  assert.equal(removeAt(list,T(250)).length,2,'a click in the gap removes nothing');
  assert.equal(selectedSeconds(list),200);
});

const rows=[
  {at:'2026-09-12T20:00:00Z',a_at:'',b_at:'',a:[[10,20,30],[40,50,60]],b:[[11,21,31],null]},
  {at:'2026-09-12T20:05:00Z',a_at:'',b_at:'',a:[[70,80,90],null],b:[[71,81,91],[1,2,3]]},
];

test('only samples both cameras saw reach the scatter',()=>{
  const sets=scatterPoints({rows},[]);
  assert.deepEqual(sets.map(s=>s.channel),['r','g','b']);
  // Four sample slots, but only two where neither side is missing.
  assert.equal(sets[0].points.length,2);
  assert.deepEqual(sets[0].points,[[10,11],[70,71]],'red pairs A against B');
  assert.deepEqual(sets[1].points,[[20,21],[80,81]],'green');
  assert.deepEqual(sets[2].points,[[30,31],[90,91]],'blue');
});

test('the selection restricts which rows contribute',()=>{
  const only=scatterPoints({rows},[{from:Date.parse('2026-09-12T20:04:00Z'),
                                    to:Date.parse('2026-09-12T20:06:00Z')}]);
  assert.deepEqual(only[0].points,[[70,71]],'the first row is outside the selection');
  const none=scatterPoints({rows},[{from:T(-9999),to:T(-9000)}]);
  assert.equal(none[0].points.length,0);
});

test('a keogram is packed time across and cut position down',()=>{
  const {width,height,data}=keogramImage(rows,'a',2);
  assert.equal(width,2);assert.equal(height,2);
  const at=(x,y)=>[...data.slice((y*width+x)*4,(y*width+x)*4+4)];
  assert.deepEqual(at(0,0),[10,20,30,255],'first row, first sample');
  assert.deepEqual(at(1,0),[70,80,90,255],'second row is the next column, not the next row');
  assert.deepEqual(at(0,1),[40,50,60,255]);
  // The missing sample stays transparent rather than becoming black sky.
  assert.deepEqual(at(1,1),[0,0,0,0]);
});

test('time maps to the panel and back again',()=>{
  assert.equal(timeToFraction(rows,Date.parse('2026-09-12T20:00:00Z')),0);
  assert.equal(timeToFraction(rows,Date.parse('2026-09-12T20:05:00Z')),1);
  assert.equal(timeToFraction(rows,Date.parse('2026-09-12T20:02:30Z')),0.5);
  assert.equal(fractionToTime(rows,0.5),Date.parse('2026-09-12T20:02:30Z'));
  // A single row has no extent; this must not divide by zero.
  assert.equal(timeToFraction([rows[0]],Date.now()),0);
  assert.equal(fractionToTime([],0.5),0);
});
