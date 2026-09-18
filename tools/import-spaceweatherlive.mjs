// Re-audit the SpaceWeatherLive directory without creating duplicate crawlers.
// Run on the GAIA server: node tools/import-spaceweatherlive.mjs --write
import fs from 'node:fs';
import sharp from 'sharp';
const directory='https://www.spaceweatherlive.com/en/auroral-activity/webcams.html';
const rows=[
['davis','Davis Antarctic research station','https://www.antarctica.gov.au/antarctic-operations/webcams/davis/','Australian Antarctic Division / Australian Antarctic Program','https://www.antarctica.gov.au/antarctic-operations/webcams/davis/',null,null],
['kho-sony','KHO Sony all-sky','https://kho.unis.no/Quicklooks/kho_sony.jpg','Kjell Henriksen Observatory / UNIS','https://kho.unis.no/',78.15,16.04],
['uit-asc01','Skibotn ASC01','https://fox.phys.uit.no/ASC/Latest_ASC01.png','Tromsø Geophysical Observatory / UiT','https://fox.phys.uit.no/ASC/',69.35,20.36],
['panomax-nordkapp','Nordkapp panorama','https://live-image.panomax.com/cams/5067/recent_reduced.jpg','Nordkapp camera operator; PANOMAX image service','https://www.panomax.com/',null,null,'Panoramic camera: operator and projection model need verification'],
['yr-setermoen','Setermoen northeast','https://www.yr.no/webcams/1/2000/setermoen/3.jpg','Original Setermoen camera operator; image supplied through Yr','https://www.yr.no/',null,null,'Operator identity and exact camera coordinates need verification'],
['panomax-loen','Loen panorama','https://live-image.panomax.com/cams/1941/recent_reduced.jpg','Loen camera operator; PANOMAX image service','https://www.panomax.com/',null,null,'Panoramic camera: operator and projection model need verification'],
['yr-brekke','Brekke / Ortneset','https://www.yr.no/webcams/1/2000/ortneset/1.jpg','Original Brekke camera operator; image supplied through Yr','https://www.yr.no/',null,null,'SWL lists the identical URL twice with conflicting directions; operator and location need verification'],
['fabian-abisko','Abisko all-sky — Fabian Wimmer','https://media.fabianwimmer.com/image.jpg','Fabian Wimmer Photography','https://fabianwimmer.com/abisko-allsky-webcam-live-northern-lights-weather/',null,null],
['porjus-north','Porjus north','https://uk.jokkmokk.jp/photo/nr4/latest.jpg','Arctic Colors, Porjus / Nature of Jokkmokk camera project','https://uk.jokkmokk.jp/',null,null],
['porjus-west','Porjus west','https://uk.jokkmokk.jp/photo/nr3/latest.jpg','Arctic Colors, Porjus / Nature of Jokkmokk camera project','https://uk.jokkmokk.jp/',null,null],
['sgo-ucl','Sodankylä UCL camera','https://www.sgo.fi/Data/RealTime/Kuvat/UCL.jpg','Sodankylä Geophysical Observatory / University of Oulu','https://www.sgo.fi/Data/RealTime/AllSky.php',null,null],
['sirius-hankasalmi','Hankasalmi all-sky','https://aurorasnow.fmi.fi/public_service/images/latest_SIR_AllSky.jpg','Jyväskylän Sirius ry — Hankasalmi Observatory','https://murtoinen.jklsirius.fi/',62+15/60+16/3600,26+35/60+59/3600],
['sirius-nyrola','Nyrölä aurora camera','https://aurorasnow.fmi.fi/public_service/images/latest_SIR.jpg','Jyväskylän Sirius ry — Nyrölä Observatory','https://rwc-finland.fmi.fi/index.php/all-sky-camera-images/',62+20/60+33.2/3600,25+30/60+35.6/3600],
['aalto-metsahovi','Metsähovi all-sky, Kirkkonummi','https://aurorasnow.fmi.fi/public_service/images/latest_HOV.jpg','Metsähovi Radio Observatory / Aalto University','https://rwc-finland.fmi.fi/index.php/all-sky-camera-images/',60+13/60+2.85/3600,24+23/60+34.9/3600],
['fmi-helsinki','Helsinki aurora camera','https://space.fmi.fi/MIRACLE/RWC/latest_HEL.jpg','Finnish Meteorological Institute','https://space.fmi.fi/MIRACLE/',null,null],
['syrjavaara','Syrjävaara Dark Sky Park','https://www.syrjavaara.fi/img/syrjavaara_latest_img.jpg','Syrjävaara Dark Sky Park','https://www.syrjavaara.fi/',null,null],
['allsky-soro','Sorø AMS89 camera 010893','https://archive.allsky.tv/AMS89/LATEST/010893.jpg','AMS89 station operator / AllSky network','https://archive.allsky.tv/AMS89/',null,null],
['ettelsberg','Ettelsberg Hochheideturm north','https://www.foto-webcam.eu/webcam/ettelsberg/current/1920.jpg','Ettelsberg-Seilbahn / Foto-Webcam.eu','https://www.foto-webcam.eu/webcam/ettelsberg/',null,null],
['auroramax-yellowknife','AuroraMAX Yellowknife','https://auroramax.phys.ucalgary.ca/recent/recent_480p.jpg','AuroraMAX — University of Calgary, Canadian Space Agency, Astronomy North and City of Yellowknife','https://auroramax.phys.ucalgary.ca/',null,null],
['nps-isle-royale','Isle Royale north shore','https://www.nps.gov/webcams-isro/northshore.jpg','Isle Royale National Park / US National Park Service','https://www.nps.gov/isro/learn/photosmultimedia/webcams.htm',null,null],
['uacnj-hope','UACNJ Hope all-sky','https://www.allskycam.com/u/627/latest_full.jpg','United Astronomy Clubs of New Jersey','https://www.uacnj.org/',null,null],
['linuxkidd-animas','Linuxkidd AstroShed — Animas, New Mexico','https://allsky.linuxkidd.com/image-fullsize.jpg','Michael J. Kidd','https://allsky.linuxkidd.com/',32.117087,-108.923644],
['cades-kingston','Cades Observatory, Kingston, Tasmania','https://www.allskycam.com/u/539/latest_full.jpg','Cades Observatory','https://www.allskycam.com/u.php?u=539',null,null],
['allsky-craigie','Craigie all-sky','https://www.allskycam.com/u/606/latest_full.jpg','Original Craigie camera operator / AllSkyCam station 606','https://www.allskycam.com/u.php?u=606',null,null],
['meteobridge-sydney','Sydney webcam','https://admin.meteobridge.com/cam/9c25cda08544cf0db24b350b27d1e5f5/camplus.jpg','Original Sydney camera operator; hosted by Meteobridge','https://www.spaceweatherlive.com/en/auroral-activity/webcams.html',null,null,'Operator identity, provider website and exact location need verification']
];
const existing=fs.readdirSync('sources').filter(x=>x.endsWith('.json')&&x!=='spaceweatherlive.json').flatMap(x=>JSON.parse(fs.readFileSync('sources/'+x)));
const canonical=u=>u.replace('://www.', '://').replace('starvisor.ru','starvisor.net');
const page=await(await fetch(directory,{signal:AbortSignal.timeout(20000)})).text();
const inventory=[...page.slice(0,page.indexOf('id="back-top"')).matchAll(/<a class="list-group-item list-group-item-action" href="([^"]+)"[^>]*>([^<]+)/g)].map(m=>({name:m[2].trim(),url:m[1]}));
if(inventory.length<40)throw Error('Unexpected directory layout: refusing partial import');
const configs=[],probes=[];let next=0;
await Promise.all(Array.from({length:4},async()=>{while(next<rows.length){
const [id,name,url,operator,website,lat,lon,hold]=rows[next++];
const duplicate=existing.find(s=>canonical(s.url)===canonical(url));if(duplicate){probes.push({url,source_id:duplicate.id,status:'already_present'});continue}
let result={url,source_id:'swl-'+id,status:'unreachable'};let enabled=false;
try {
let r=await fetch(url,{signal:AbortSignal.timeout(15000)});if(id==='davis'&&r.ok){const html=await r.text();const image=html.match(/https:\/\/images\.antarctica\.gov\.au\/webcams\/davis\/[^"'<> ]+\.jpg/)?.[0];if(!image)throw Error('No Davis image');r=await fetch(image,{signal:AbortSignal.timeout(15000)})}if(!r.ok)throw Error('HTTP '+r.status);
const size=Number(r.headers.get('content-length')||0);if(size>16*1024*1024)throw Error('Image too large');
const b=Buffer.from(await r.arrayBuffer());if(b.length>16*1024*1024)throw Error('Image too large');
const meta=await sharp(b).metadata();if(!meta.width||!meta.height)throw Error('Not an image');
const modified=r.headers.get('last-modified'), stale=modified&&Number.isFinite(Date.parse(modified))&&Date.now()-Date.parse(modified)>72*3600000;
enabled=!stale&&!hold;result={...result,status:stale?'stale_upstream':hold?'needs_review':'enabled_uncalibrated',modified,width:meta.width,height:meta.height,bytes:b.length,note:hold||null};
}catch(e){result.error=String(e)}
probes.push(result);
configs.push({id:'swl-'+id,name,kind:id==='davis'?'html_index':'snapshot_url',...(id==='davis'?{image_link_regex:'^https://images[.]antarctica[.]gov[.]au/webcams/davis/.*[.]jpg$'}:{}),url,timestamp_mode:'download_time',interval_seconds:60,producer:{name:operator,institution:null,website,acknowledgement:'Images provided by '+operator+'. Camera discovered through the SpaceWeatherLive webcam directory. Original camera operators retain copyright; scientific and commercial use requires their permission.',copyright:'Copyright retained by the original camera operator and data producers.',license:null},latitude_deg:lat,longitude_deg:lon,altitude_m:null,stream_updated_header:'last-modified',enabled});
console.log(result.source_id,result.status);
}}));
configs.sort((a,b)=>a.id.localeCompare(b.id));
for(const entry of inventory){
const probe=probes.find(p=>p.url===entry.url||rows.find(r=>r[2]===p.url&&r[4]===entry.url));
const match=existing.find(s=>canonical(s.url)===canonical(entry.url)||canonical(s.producer.website)===canonical(entry.url));
Object.assign(entry,probe|| (match?{source_id:match.id,status:'already_present'}:{status:entry.url.includes('youtube.com')?'video_stream_adapter_needed':'provider_endpoint_research_needed'}));
if(entry.url.includes('/krn/latest_medium'))Object.assign(entry,{source_id:'irf-kiruna-alis',status:'already_present'});
if(entry.url.includes('starvisor')){const suffix=entry.url.split('/').pop(),ids={'capture_str.jpg':'starvisor-strezhevoy','cap_spbd.jpg':'starvisor-st-petersburg','cap_klnsky.jpg':'starvisor-kaliningrad'};Object.assign(entry,{source_id:ids[suffix],status:'already_present_restricted'});}
}
const audit={directory,checked_at:new Date().toISOString(),entries:inventory,probes,notes:['No YouTube thumbnail substitutes for observations. Video streams need a bounded adapter.','Coordinates left null when exact station location is not verified; set before AIDA calibration.','Panoramas and unidentified operators are held for review. Stale upstream files are not enabled as realtime cameras.','No calibrations copied between instruments at the same site. Starvisor authorization unchanged.','Linuxkidd provider config.js says Animas, not the obsolete Mayhill label in the directory.','Brekke is listed twice with the same URL; only one crawler is configured.']};
if(process.argv.includes('--write')){fs.writeFileSync('sources/spaceweatherlive.json',JSON.stringify(configs,null,2)+'\n');fs.mkdirSync('docs',{recursive:true});fs.writeFileSync('docs/spaceweatherlive-audit.json',JSON.stringify(audit,null,2)+'\n');}
console.log(JSON.stringify({directory_entries:inventory.length,configured:configs.length,enabled:configs.filter(c=>c.enabled).length,pending:inventory.filter(e=>!e.source_id).length}));
