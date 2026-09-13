// Git-tracked, narrowly scoped patch for the existing standalone AIDA install.
// Usage: node deploy/aida-calibration-mask.cjs /absolute/aida/root
const fs=require('fs'),path=require('path'),root=process.argv[2];
if(!root||!path.isAbsolute(root))throw Error('Absolute AIDA root required');
const p=path.join(root,'js/app.js');let s=fs.readFileSync(p,'utf8');
const replacements=[
 ['? `/gaia/api/images/${encodeURIComponent(imageId)}/original`','? `/gaia/api/images/${encodeURIComponent(imageId)}/original?calibration=true`'],
 [': `/gaia/api/sources/${encodeURIComponent(sourceId)}/latest`',': `/gaia/api/sources/${encodeURIComponent(sourceId)}/latest?calibration=true`'],
 ['fetch(endpoint, {cache: selected ? "force-cache" : "no-store"})','fetch(endpoint, {cache: "no-store"})']
];
for(const [a,b] of replacements){if(s.includes(a))s=s.replace(a,b);else if(!s.includes(b))throw Error('AIDA handoff changed; review before patching')}
fs.writeFileSync(p,s);
const html=path.join(root,'index.html'),before=fs.readFileSync(html,'utf8');
if(!/js\/app\.js\?v=[^"]+/.test(before))throw Error('AIDA script tag missing');
fs.writeFileSync(html,before.replace(/js\/app\.js\?v=[^"]+/,'js/app.js?v=20260913-gaia-mask'));
console.log('AIDA handoff requests current masked calibration copies');
