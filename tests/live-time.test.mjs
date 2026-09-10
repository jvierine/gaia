import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/live-time.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {liveCutoff,readyFrames}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
test('delayed live is ten minutes behind wall clock, rounded down',()=>{
 const now=Date.parse('2026-09-10T16:00:45Z');
 assert.equal(liveCutoff(now),Date.parse('2026-09-10T15:50:00Z'));
});
test('latest excludes incomplete new frames but retains older published history',()=>{
 const frames=[{at:'2026-09-10T15:40:00Z'},{at:'2026-09-10T15:50:00Z'},{at:'2026-09-10T15:51:00Z'},{at:'invalid'}];
 assert.deepEqual(readyFrames(frames,Date.parse('2026-09-10T16:00:00Z')),frames.slice(0,2));
 assert.equal(readyFrames(frames,Date.parse('2026-09-10T15:00:00Z')).length,0);
});
