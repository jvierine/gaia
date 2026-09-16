import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/keogram.ts',import.meta.url),'utf8');
// ES2020 to match the shipped bundle: the default target compiles for..of over
// a Set to an index loop that never runs, which has bitten this suite before.
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {makeWindow,addWindow,removeWindowAt,edgeNear,resizeWindow,inAny,selectedSeconds,
       scatterPoints,rowsWithin,keogramImage,timeToFraction,fractionToTime}=
  await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

const T=t=>Date.parse('2026-09-12T20:00:00Z')+t*1000;
const row=(seconds,a,b)=>({at:new Date(T(seconds)).toISOString(),a_at:'',b_at:'',a,b});
// Two nights of rows, so a selection can be made on each.
const tonight=[row(0,[[10,20,30]],[[11,21,31]]),row(100,[[40,50,60]],[[41,51,61]]),
               row(200,[[70,80,90]],[[71,81,91]])];
const lastWeek=[{at:'2026-09-05T21:00:00.000Z',a_at:'',b_at:'',a:[[12,22,32]],b:[[13,23,33]]}];

test('overlapping selections merge instead of double counting',()=>{
  let list=addWindow([],makeWindow('p',T(0),T(100),tonight));
  list=addWindow(list,makeWindow('p',T(50),T(200),tonight));
  assert.equal(list.length,1,'two overlapping brushes are one window');
  assert.deepEqual([list[0].from,list[0].to],[T(0),T(200)]);
  assert.equal(list[0].rows.length,3,'each row appears once');
  list=addWindow(list,makeWindow('p',T(400),T(500),tonight));
  assert.equal(list.length,2,'a disjoint brush stays separate');
});

test('a brush dragged backwards still selects the interval',()=>{
  const list=addWindow([],makeWindow('p',T(300),T(100),tonight));
  assert.deepEqual([list[0].from,list[0].to],[T(100),T(300)]);
  assert.ok(inAny(list,T(200)));
});

test('clicking inside a selection takes it back',()=>{
  const list=addWindow(addWindow([],makeWindow('p',T(0),T(100),tonight)),
                       makeWindow('p',T(400),T(500),tonight));
  const left=removeWindowAt(list,T(50));
  assert.equal(left.length,1);
  assert.equal(left[0].from,T(400));
  assert.equal(removeWindowAt(list,T(250)).length,2,'a click in the gap removes nothing');
  assert.equal(selectedSeconds(list),200);
});

test('either end of a window can be moved in either direction',()=>{
  let list=addWindow([],makeWindow('p',T(50),T(150),tonight));
  // The late end forward, then back past where it began.
  list=resizeWindow(list,0,'to',T(250),tonight);
  assert.deepEqual([list[0].from,list[0].to],[T(50),T(250)]);
  assert.equal(list[0].rows.length,2,'growing picks up the row it now covers');
  list=resizeWindow(list,0,'to',T(120),tonight);
  assert.deepEqual([list[0].from,list[0].to],[T(50),T(120)]);
  assert.equal(list[0].rows.length,1,'shrinking drops the row it no longer covers');
  // The early end backwards, picking up the first row.
  list=resizeWindow(list,0,'from',T(-50),tonight);
  assert.deepEqual([list[0].from,list[0].to],[T(-50),T(120)]);
  assert.equal(list[0].rows.length,2);
  // Dragging an end past the other flips rather than collapsing to nothing.
  list=resizeWindow(list,0,'from',T(300),tonight);
  assert.deepEqual([list[0].from,list[0].to],[T(120),T(300)]);
  assert.ok(list[0].to>list[0].from);
});

test('an edge is found only when the pointer is near it',()=>{
  const list=addWindow([],makeWindow('p',T(50),T(150),tonight));
  assert.deepEqual(edgeNear(list,T(52),5000),{index:0,edge:'from'});
  assert.deepEqual(edgeNear(list,T(148),5000),{index:0,edge:'to'});
  assert.equal(edgeNear(list,T(100),5000),null,'the middle of a window is not an edge');
  // Five seconds away with a tenth-second tolerance: out of reach.
  assert.equal(edgeNear(list,T(55),100),null,'outside the tolerance');
  assert.deepEqual(edgeNear(list,T(50),100),{index:0,edge:'from'},'exactly on the edge is on it');
});

test('windows from different nights both feed the scatter',()=>{
  // One window on tonight's keogram, one made earlier on another night. The
  // rows travel with the window, so the older one survives the panel moving on.
  let list=addWindow([],makeWindow('p',T(-100),T(50),tonight));
  list=addWindow(list,makeWindow('p',Date.parse('2026-09-05T20:00:00Z'),
                                     Date.parse('2026-09-05T22:00:00Z'),lastWeek));
  assert.equal(list.length,2);
  const sets=scatterPoints(list);
  assert.deepEqual(sets.map(s=>s.channel),['r','g','b']);
  assert.deepEqual(sets[0].points.sort((p,q)=>p[0]-q[0]),[[10,11],[12,13]],
    'one point from each night');
  assert.deepEqual(sets[1].points.sort((p,q)=>p[0]-q[0]),[[20,21],[22,23]]);
});

test('only samples both cameras saw reach the scatter',()=>{
  const mixed=[{at:'2026-09-12T20:00:00.000Z',a_at:'',b_at:'',
                a:[[10,20,30],[40,50,60]],b:[[11,21,31],null]}];
  const sets=scatterPoints([{rows:mixed}]);
  assert.equal(sets[0].points.length,1,'the unmatched sample is dropped');
  assert.deepEqual(sets[0].points,[[10,11]]);
});

test('rows are selected by the interval they fall in',()=>{
  assert.equal(rowsWithin(tonight,T(-10),T(110)).length,2);
  assert.equal(rowsWithin(tonight,T(110),T(-10)).length,2,'ends either way round');
  assert.equal(rowsWithin(tonight,T(1000),T(2000)).length,0);
});

test('a keogram is packed time across and cut position down',()=>{
  const {width,height,data}=keogramImage(tonight.slice(0,2).map((r,i)=>(
    {...r,a:i?[[70,80,90],null]:[[10,20,30],[40,50,60]],b:r.b}
  )),'a',2);
  assert.equal(width,2);assert.equal(height,2);
  const at=(x,y)=>[...data.slice((y*width+x)*4,(y*width+x)*4+4)];
  assert.deepEqual(at(0,0),[10,20,30,255],'first row, first sample');
  assert.deepEqual(at(1,0),[70,80,90,255],'second row is the next column, not the next row');
  assert.deepEqual(at(0,1),[40,50,60,255]);
  // The missing sample stays transparent rather than becoming black sky.
  assert.deepEqual(at(1,1),[0,0,0,0]);
});

test('time maps to the panel and back again',()=>{
  const rows=[tonight[0],{...tonight[0],at:'2026-09-12T20:05:00.000Z'}];
  assert.equal(timeToFraction(rows,Date.parse('2026-09-12T20:00:00Z')),0);
  assert.equal(timeToFraction(rows,Date.parse('2026-09-12T20:05:00Z')),1);
  assert.equal(timeToFraction(rows,Date.parse('2026-09-12T20:02:30Z')),0.5);
  assert.equal(fractionToTime(rows,0.5),Date.parse('2026-09-12T20:02:30Z'));
  // A single row has no extent; this must not divide by zero.
  assert.equal(timeToFraction([rows[0]],Date.now()),0);
  assert.equal(fractionToTime([],0.5),0);
});
