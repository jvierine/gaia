// Metadata-only rollout for an already verified anonymous manifest. No assets
// are copied, and restricted projections/lens models are never imported.
const fs=require('fs');
const [fullPath,openPath]=process.argv.slice(2);
const full=JSON.parse(fs.readFileSync(fullPath)),open=JSON.parse(fs.readFileSync(openPath));
if(open.audience!=='anonymous-no-starvisor')throw Error('Not an anonymous manifest');
const restricted=c=>c.imagery_restricted===true||/starvis/i.test([c.source_id,c.producer,c.website_url].join(' '));
const entries=new Map(open.cameras.filter(c=>!restricted(c)).map(c=>[c.source_id,c]));
let index=Math.max(0,...open.cameras.map(c=>c.map_index||0));
for(const c of full.cameras.filter(restricted)){
  const metadata={};
  for(const key of ['source_id','name','producer','institution','website_url','latitude_deg','longitude_deg','altitude_m','acknowledgement','copyright','calibrated'])metadata[key]=c[key];
  entries.set(c.source_id,{...metadata,imagery_restricted:true,map_index:++index});
}
open.cameras=[...entries.values()].sort((a,b)=>a.source_id.localeCompare(b.source_id));
open.lens_models=(open.lens_models||[]).filter(c=>!restricted(c));
const pending=openPath+'.stations-'+process.pid;
fs.writeFileSync(pending,JSON.stringify(open));fs.renameSync(pending,openPath);
console.log('Public Starvisor station metadata:',open.cameras.filter(restricted).length);
