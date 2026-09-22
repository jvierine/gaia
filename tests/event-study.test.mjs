import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/event-study.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {chooseBinMinutes,buildTimeBins,projectEventLocation,clusterVisibleMedia}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const item=(id,at,lat=60,lon=0,until=null)=>({id,source:'test',kind:until?'timelapse':'image',title:null,creator:null,location:'test',latitude:lat,longitude:lon,capturedAt:at,capturedUntil:until,timePrecision:'second',timeSource:'test',coordinateSource:'test',coordinatePrecision:'test',grade:'A',thumbnailUrl:'',previewUrl:'',mediaUrl:null,sourceUrl:'',originalUrl:null,license:null,rightsNote:null});

test('bin width adapts while explicit five-minute bins retain exact times',()=>{
  const rows=Array.from({length:40},(_,i)=>item(String(i),`2025-11-12T03:${String(i%10).padStart(2,'0')}:00Z`));
  assert.equal(chooseBinMinutes(rows),5);
  const bins=buildTimeBins(rows,5);
  assert.equal(bins.length,2);assert.equal(bins.reduce((n,b)=>n+b.items.length,0),40);
  assert.equal(bins[0].items[0].capturedAt,'2025-11-12T03:00:00Z');
});

test('timelapses remain visible in populated bins they overlap',()=>{
  const video=item('v','2025-11-12T02:00:00Z',60,0,'2025-11-12T04:00:00Z');
  const bins=buildTimeBins([video,item('a','2025-11-12T03:01:00Z'),item('b','2025-11-12T03:11:00Z')],5);
  assert.ok(bins.filter(b=>b.at>=Date.parse('2025-11-12T03:00:00Z')).every(b=>b.items.some(x=>x.id==='v')));
});

test('projection hides the far side and clustering retains every visible item',()=>{
  const view={yaw:0,pitch:0,zoom:1,width:1000,height:800};
  assert.equal(projectEventLocation(0,0,view).visible,true);
  assert.equal(projectEventLocation(0,180,view).visible,false);
  const rows=[item('a','2025-11-12T03:00:00Z',60,10),item('b','2025-11-12T03:01:00Z',60.01,10.01),item('far','2025-11-12T03:02:00Z',0,180)];
  const clusters=clusterVisibleMedia(rows,view);
  assert.equal(clusters.length,1);assert.deepEqual(clusters[0].items.map(x=>x.id),['a','b']);
});
