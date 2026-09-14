import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/playback-clock.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {clockAt,loopPosition,resumePosition}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

const minute=60000;
const day=Array.from({length:144},(_,i)=>Date.parse('2026-09-13T00:00:00Z')+i*10*minute);

test('the clock runs continuously between image times',()=>{
  // Halfway between the first two images is halfway in time, not on either.
  const half=clockAt(day,0.5/day.length);
  assert.equal(half,(day[0]+day[1])/2);
  // Strictly increasing across a whole loop, which is what "smooth" means here:
  // no two successive samples share an instant, so the globe never stalls.
  let previous=-Infinity;
  for(let i=0;i<1000;i++){
    const t=clockAt(day,i/1000);
    assert.ok(t>previous,`clock went backwards or stalled at ${i}`);
    previous=t;
  }
});

test('the clock stays inside the published day',()=>{
  assert.equal(clockAt(day,0),day[0]);
  // It may run past the last image by at most one image spacing, which is the
  // gap it is crossing before the loop wraps, and no further.
  assert.ok(clockAt(day,0.9999)<=day[day.length-1]+10*minute);
  assert.equal(clockAt([],0.5),null);
  assert.equal(clockAt([day[0]],0.7),day[0]);
});

test('a loop position wraps and never runs backwards within a pass',()=>{
  assert.equal(loopPosition(0,1000),0);
  assert.equal(loopPosition(250,1000),0.25);
  // Second time round is the same place, which is what lets a loop repeat.
  assert.equal(loopPosition(1250,1000),0.25);
  assert.equal(loopPosition(500,0),0);
});

test('playback resumes where it was, not at the beginning',()=>{
  const middle=day[72];
  const position=resumePosition(day,middle);
  assert.equal(position,72/day.length);
  // And the clock built from that position lands back on the same image.
  assert.ok(Math.abs(clockAt(day,position)-middle)<1);
});

test('a speed change keeps the place it was at',()=>{
  // The player restarts on every speed change, so the resume point has to
  // survive a change of loop duration. Same position, different duration.
  const at=day[100];
  const position=resumePosition(day,at);
  for(const duration of [15000,60000,1875]){
    const start=1000-position*duration;
    const elapsed=1000-start;
    assert.ok(Math.abs(clockAt(day,loopPosition(elapsed,duration))-at)<1,
      `duration ${duration} lost the position`);
  }
});

test('a time outside the published day starts at the beginning',()=>{
  // The live edge is past the last overview image; there is nothing to carry on
  // from, so a run started there begins the day rather than wrapping instantly.
  assert.equal(resumePosition(day,day[day.length-1]+minute),0);
  assert.equal(resumePosition(day,day[0]-minute),0);
  assert.equal(resumePosition(day,null),0);
  assert.equal(resumePosition(day,Number.NaN),0);
  assert.equal(resumePosition([],day[0]),0);
});
