// Compare browser time-dependent weights directly with the old Rust equations.
const fs=require('fs'),ts=require('typescript'),vm=require('vm'),cp=require('child_process'),assert=require('assert');
const code=ts.transpileModule(fs.readFileSync('src/composition-rules.ts','utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS}}).outputText;
const sandbox={exports:{},Math};vm.runInNewContext(code,sandbox);const rules=sandbox.exports;
const out=cp.execFileSync('/home/j/.cargo/bin/cargo',['test','--release','--bin','gaia-server','--target-dir','/mnt/data/juha/gaia-build','publish_layers::tests::browser_solar_reference','--','--nocapture'],{encoding:'utf8',maxBuffer:2e6});
const cases=JSON.parse(out.split('\n').find(l=>l.startsWith('GAIA_RULES_REFERENCE=')).slice('GAIA_RULES_REFERENCE='.length));let maxElevation=0,maxScale=0;
for(const c of cases){const elevation=rules.solarElevation(c.latitude_deg,c.longitude_deg,c.epoch),scale=rules.cameraWeightScale(c,c.epoch,{sun_dark_deg:-12,sun_light_deg:0,sun_floor:.05});maxElevation=Math.max(maxElevation,Math.abs(elevation-c.elevation));maxScale=Math.max(maxScale,Math.abs(scale-c.scale));assert(Math.abs(elevation-c.elevation)<1e-9);assert(Math.abs(scale-c.scale)<1e-10)}
assert.strictEqual(rules.smoothStep(-1),0);assert.strictEqual(rules.smoothStep(.5),.5);assert.strictEqual(rules.smoothStep(1),1);
console.log(JSON.stringify({cases:cases.length,maxElevationErrorDegrees:maxElevation,maxWeightScaleError:maxScale}));
