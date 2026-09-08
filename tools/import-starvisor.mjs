import fs from 'node:fs';

const input=process.argv[2];
if(!input)throw new Error('usage: node tools/import-starvisor.mjs starvisor.html');
const html=fs.readFileSync(input,'utf8').replaceAll('&nbsp;',' ');
const blocks=[...html.matchAll(/<div class="city-container" data-cam-name="([^"]+)"[\s\S]*?<\/a><\/div>/g)].filter((m,i,a)=>a.findIndex(x=>x[1]===m[1])===i);
const slug=s=>s.toLowerCase().normalize('NFKD').replace(/[^a-z0-9]+/g,'-').replace(/^-|-$/g,'');
const coord=(text,which)=>{const m=text.match(new RegExp(`([0-9]+(?:\\.[0-9]+)?)°${which==='lat'?'([NS])':'([EW])'}`));if(!m)return null;return Number(m[1])*(m[2]==='S'||m[2]==='W'?-1:1)};
const sources=blocks.map(match=>{const name=match[1].trim(),block=match[0];const coords=(block.match(/class="city-name-item coordinates">([^<]+)/)||[])[1]||'';const detail=(block.match(/<a href="([^"]+)"/)||[])[1];const image=(block.match(/<img[^>]+src="([^"]+)/)||[])[1];if(!detail||!image)return null;const current=image.split('?')[0].replace('/small/','/orig/');return{
  id:`starvisor-${slug(name)}`,name:`STARVISOR ${name}`,kind:'snapshot_url',url:current,timestamp_mode:'download_time',interval_seconds:300,
  producer:{name:`${name} camera operator`,institution:'STARVISOR Night Sky Patrol',website:detail,acknowledgement:`Image from the ${name} camera, made available by the STARVISOR Night Sky Patrol network.`,copyright:`Copyright remains with the ${name} camera operator and STARVISOR as applicable.`,license:null},
  latitude_deg:coord(coords,'lat'),longitude_deg:coord(coords,'lon'),altitude_m:null,request_headers:{Referer:detail},enabled:true
}}).filter(Boolean);
sources.sort((a,b)=>a.name.localeCompare(b.name));
const ids=new Set(sources.map(s=>s.id));
if(ids.size!==sources.length)throw new Error('duplicate STARVISOR id');
if(!sources.some(s=>s.id==='starvisor-popovo'))throw new Error('Popovo was not found');
fs.writeFileSync('sources/starvisor.json',`${JSON.stringify(sources,null,2)}\n`);
console.log(`wrote ${sources.length} STARVISOR cameras; Popovo=${sources.find(s=>s.id==='starvisor-popovo').url}`);
