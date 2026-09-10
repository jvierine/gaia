import {test} from 'node:test';import assert from 'node:assert/strict';
import ts from 'typescript';import {readFileSync} from 'node:fs';
const code=ts.transpileModule(readFileSync(new URL('../src/source-map-coordinates.ts',import.meta.url),'utf8'),{compilerOptions:{module:ts.ModuleKind.ESNext}}).outputText;
const {shellTextureCoordinates}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
test('picking inverts the rendered 100-km shell, including oblique views',()=>{
 for(const [lat,lon,yaw,pitch] of [[68,20,.35,-1.15],[54,40,.35,-1.15],[0,60,0,0],[60,-100,-1.5,-.8]]){
  const r=1+100/6371,a=lat*Math.PI/180,b=lon*Math.PI/180,x=r*Math.cos(a)*Math.sin(b),y=r*Math.sin(a),z=r*Math.cos(a)*Math.cos(b);
  const px=Math.cos(yaw)*x-Math.sin(yaw)*z,zz=Math.sin(yaw)*x+Math.cos(yaw)*z,py=Math.cos(pitch)*y+Math.sin(pitch)*zz;
  const uv=shellTextureCoordinates(px,py,yaw,pitch);assert.ok(uv);assert.ok(Math.abs(uv[0]-(lon/360+.5))<1e-10);assert.ok(Math.abs(uv[1]-(.5-lat/180))<1e-10);
 }
});
test('shell rim remains pickable outside the Earth disc',()=>{assert.ok(shellTextureCoordinates(1.005,0,0,0));assert.equal(shellTextureCoordinates(1.1,0,0,0),null);});
