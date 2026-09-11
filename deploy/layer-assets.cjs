// Accept only prepared local assets from the selected audience namespace.
const fs=require('fs'),path=require('path');
const [file,root,audience]=process.argv.slice(2),m=JSON.parse(fs.readFileSync(file));
if(m.composition!=='browser-layers-v1')throw Error('Expected independent camera layers');
const restricted=c=>c.imagery_restricted===true||/starvis/i.test(c.source_id);
if(audience==='open'&&(m.audience!=='anonymous-no-starvisor'||m.cameras.some(c=>restricted(c)&&c.projection!=null)||(m.lens_models||[]).some(restricted)))throw Error('Restricted imagery in anonymous audience');
const names=new Set();function visit(v){if(typeof v==='string'&&v.startsWith('/gaia/')){const prefix=`/gaia/${audience}/assets/`;if(!v.startsWith(prefix))throw Error('Unexpected asset namespace');const n=v.slice(prefix.length);if(!/^[\w.-]+$/.test(n)||n.includes('..'))throw Error('Unsafe asset');names.add(n)}else if(Array.isArray(v))v.forEach(visit);else if(v&&typeof v==='object')Object.values(v).forEach(visit)}visit(m);
for(const n of names)if(!fs.statSync(path.join(root,n)).size)throw Error('Empty asset '+n);
process.stdout.write([...names].sort().join('\n')+'\n');
