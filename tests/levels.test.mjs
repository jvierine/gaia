import {test} from 'node:test';
import assert from 'node:assert/strict';
import ts from 'typescript';
import {readFileSync} from 'node:fs';
const source=readFileSync(new URL('../src/levels.ts',import.meta.url),'utf8');
const code=ts.transpileModule(source,{compilerOptions:{module:ts.ModuleKind.ESNext,target:ts.ScriptTarget.ES2020}}).outputText;
const {clampLevels,transfer,apply,levelToFraction,fractionToLevel,suggest,FULL_RANGE}=
  await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));

test('the full range is the identity',()=>{
  const t=transfer(FULL_RANGE);
  assert.equal(t.slope,1);
  assert.equal(t.intercept,0);
  assert.equal(apply(FULL_RANGE,0),0);
  assert.equal(apply(FULL_RANGE,255),255);
  assert.equal(apply(FULL_RANGE,128),128);
});

test('a narrow window stretches and clips',()=>{
  const l={min:20,max:60};
  assert.equal(apply(l,20),0,'the black point goes to black');
  assert.equal(apply(l,60),255,'the white point goes to white');
  assert.equal(apply(l,40),128,'the middle lands in the middle');
  assert.equal(apply(l,5),0,'below the window clips to black');
  assert.equal(apply(l,200),255,'above it clips to white');
  // The SVG transfer has to agree with the preview, or the bar lies about the
  // image beside it.
  const t=transfer(l);
  for(const v of [20,40,60]){
    const svg=Math.max(0,Math.min(1,t.slope*(v/255)+t.intercept));
    assert.ok(Math.abs(svg*255-apply(l,v))<1.5,`${v}: svg ${svg*255} vs preview ${apply(l,v)}`);
  }
});

test('levels are ordered and never zero width',()=>{
  assert.deepEqual(clampLevels({min:200,max:50}),{min:50,max:200},'reversed ends swap');
  const flat=clampLevels({min:100,max:100});
  assert.ok(flat.max>flat.min,'a zero-width window would divide by nothing');
  assert.deepEqual(clampLevels({min:-40,max:900}),{min:0,max:255},'clamped to the range');
  assert.ok(Number.isFinite(transfer({min:100,max:100}).slope));
});

test('a bar position maps to a level and back',()=>{
  assert.equal(levelToFraction(255),0,'white is at the top');
  assert.equal(levelToFraction(0),1,'black is at the bottom');
  assert.equal(fractionToLevel(0),255);
  assert.equal(fractionToLevel(1),0);
  assert.equal(fractionToLevel(levelToFraction(96)),96);
  assert.equal(fractionToLevel(-0.5),255,'off the top clamps');
  assert.equal(fractionToLevel(9),0,'off the bottom clamps');
});

test('the suggested stretch shows the stars',()=>{
  // A typical dark frame: sky at 40, brightest star peaking at 150.
  const l=suggest(40,150);
  assert.ok(l.min<40&&l.min>=0,`black point ${l.min} should sit under the sky`);
  assert.ok(l.max>150&&l.max<=255,`white point ${l.max} should clear the brightest star`);
  assert.ok(apply(l,150)>200,'the brightest star is near white');
  assert.ok(apply(l,45)<60,'the sky stays dark');
  // Missing numbers must not produce a broken window.
  const fallback=suggest(null,null);
  assert.ok(fallback.max>fallback.min);
  assert.ok(Number.isFinite(transfer(fallback).slope));
});
