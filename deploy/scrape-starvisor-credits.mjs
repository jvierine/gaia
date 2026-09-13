// Refresh provider-published camera credits; original links and roles are retained.
import fs from 'node:fs/promises';
const sources=JSON.parse(await fs.readFile('sources/starvisor.json','utf8'));
const decode=s=>s.replace(/&nbsp;/g,' ').replace(/&amp;/g,'&').replace(/&quot;/g,'"').replace(/&#39;/g,"'");
const text=s=>decode(s.replace(/<[^>]*>/g,' ').replace(/\s+/g,' ').trim());
// Discover station pages not yet registered in GAIA as well.
const home=await (await fetch('https://starvisor.net/')).text();
const known=new Set(sources.map(s=>new URL(s.producer.website).pathname.replace(/\/$/,'')));
for(const m of home.matchAll(/<div class="city-container" data-cam-name="([^"]+)"[\s\S]*?<a href="([^"]+)"/g)) {
 const url=new URL(m[2]);if(known.has(url.pathname.replace(/\/$/,'')))continue;
 known.add(url.pathname.replace(/\/$/,''));sources.push({id:'starvisor-page-'+url.pathname.replace(/\//g,''),name:'STARVISOR '+decode(m[1]),producer:{website:url.href}});
}
const records=[];
for(const source of sources){
 const url=source.producer.website;
 try {
 const response=await fetch(url,{signal:AbortSignal.timeout(20000)});if(!response.ok)throw Error('HTTP '+response.status);
 const html=await response.text();const info=html.match(/<div[^>]*id=["']cam-info["'][^>]*>([\s\S]*?)<\/div>/i)?.[1];
 const credit=info?text(info):null;
 const links=info?[...info.matchAll(/<a[^>]*href=["']([^"']+)["'][^>]*>([\s\S]*?)<\/a>/gi)].map(m=>({name:text(m[2]),url:new URL(decode(m[1]),response.url).href})).filter(l=>/^https?:/.test(l.url)):[];
 records.push({source_id:source.id,camera:source.name.replace(/^STARVISOR /,''),page_url:response.url,credit,links,checked_utc:new Date().toISOString()});
 console.log(source.id+': '+(credit||'NO CAMERA CREDIT FOUND'));
 }catch(error){throw Error(url+': '+error.message);}
 await new Promise(r=>setTimeout(r,300));
}
await fs.writeFile('src/starvisor-credits.json',JSON.stringify(records,null,2)+'\n');

const registered=JSON.parse(await fs.readFile('sources/starvisor.json','utf8'));
for(const source of registered){const row=records.find(r=>r.source_id===source.id);if(!row?.credit)throw Error('Missing station credit for '+source.id);source.producer.acknowledgement=row.credit+'. Camera imagery provided through STARVISOR Night Sky Patrol. Source: '+row.page_url;}
await fs.writeFile('sources/starvisor.json',JSON.stringify(registered,null,2)+'\n');
