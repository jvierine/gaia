import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/station-solo.ts',import.meta.url),'utf8');
// ES2020 to match the shipped bundle. At the default target, for...of over a
// Set compiles to an index loop and silently iterates nothing, which would let
// a broken set comparison pass.
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {bestAtStation,sameSelection,SAME_STATION_DEG}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

// Two cameras on one roof at Skibotn, one downgraded; one camera far away.
const skibotn={lat:69.35,lon:20.36};
const sites=[
  {source_id:'bacc5',      lat:69.35,  lon:20.36,  weight:0},
  {source_id:'ai-skibotn', lat:69.35,  lon:20.36,  weight:-3},
  {source_id:'kiruna',     lat:67.84,  lon:20.41,  weight:0},
];

test('the best weight at the station wins, and neighbours are excluded',()=>{
  const best=bestAtStation(sites,skibotn);
  assert.deepEqual([...best],['bacc5']);
  // The neighbour has an equally good weight but is not at this station.
  assert.ok(!best.has('kiruna'));
});

test('a downgraded camera never wins its own station',()=>{
  const only=[{source_id:'weak',lat:69.35,lon:20.36,weight:-8},
              {source_id:'strong',lat:69.35,lon:20.36,weight:-1}];
  assert.deepEqual([...bestAtStation(only,skibotn)],['strong']);
});

test('cameras tied at the best weight are kept together',()=>{
  const tied=[{source_id:'a',lat:69.35,lon:20.36,weight:-2},
              {source_id:'b',lat:69.35,lon:20.36,weight:-2},
              {source_id:'c',lat:69.35,lon:20.36,weight:-4}];
  assert.deepEqual([...bestAtStation(tied,skibotn)].sort(),['a','b']);
});

test('coordinates only have to match to within the station tolerance',()=>{
  const jittered=[{source_id:'a',lat:69.35+SAME_STATION_DEG/2,lon:20.36,weight:0}];
  assert.deepEqual([...bestAtStation(jittered,skibotn)],['a']);
  const apart=[{source_id:'a',lat:69.35+SAME_STATION_DEG*2,lon:20.36,weight:0}];
  assert.equal(bestAtStation(apart,skibotn),null);
});

test('an empty site yields nothing rather than an empty selection',()=>{
  assert.equal(bestAtStation([],skibotn),null);
  assert.equal(bestAtStation([{lat:69.35,lon:20.36,weight:0}],skibotn),null,'a camera with no id is not selectable');
});

test('pressing the same station again is recognised as a request to restore',()=>{
  const best=bestAtStation(sites,skibotn);
  assert.ok(sameSelection(best,bestAtStation(sites,skibotn)));
  assert.ok(!sameSelection(best,bestAtStation(sites,{lat:67.84,lon:20.41})));
  assert.ok(!sameSelection(null,best));
  assert.ok(!sameSelection(best,null));
});
