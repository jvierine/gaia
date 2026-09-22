#!/usr/bin/env node
import {createHash} from 'node:crypto';
import {mkdir,readFile,stat,writeFile} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import path from 'node:path';

const [input='/data1/gaiasm/2025-11-11-g4/manifest/geotimed-media.jsonl',output='/data1/gaiasm/events/20251111',eventId='20251111']=process.argv.slice(2);
const baseUrl=`/gaia/public/events/${eventId}`;
const lines=(await readFile(input,'utf8')).split(/\n+/).filter(Boolean);
const rows=lines.map(line=>JSON.parse(line));
await Promise.all(['thumbnails','previews','videos'].map(name=>mkdir(path.join(output,name),{recursive:true})));

const explicitUtc=value=>{const match=String(value||'').match(/^(2025-\d\d-\d\d) (\d\d:\d\d:\d\d) UTC$/);if(!match)return null;const date=new Date(`${match[1]}T${match[2]}Z`);return Number.isFinite(date.valueOf())?date.toISOString():null};
const fromLocalAndOffset=(local,offset)=>{const match=String(local||'').match(/^(2025):(\d\d):(\d\d) (\d\d:\d\d:\d\d)$/);if(!match||!/^[+-]\d\d:\d\d$/.test(offset||''))return null;const value=new Date(`${match[1]}-${match[2]}-${match[3]}T${match[4]}${offset}`);return Number.isFinite(value.valueOf())?value.toISOString():null};
const exists=async file=>{try{const value=await stat(file);return value.isFile()&&value.size>0}catch{return false}};
const key=value=>createHash('sha256').update(value).digest('hex').slice(0,20);
const run=(command,args)=>new Promise((resolve,reject)=>{const child=spawn(command,args,{stdio:['ignore','ignore','pipe']});let error='';child.stderr.on('data',chunk=>{if(error.length<4000)error+=chunk});child.on('error',reject);child.on('close',code=>code===0?resolve():reject(Error(`${command} exited ${code}: ${error.slice(-1200)}`)))});

const candidates=[];let unavailable=0,untimed=0,unlocated=0;
for(const row of rows){
  if(!row.downloaded||!await exists(row.local_path)){unavailable++;continue}
  if(!Number.isFinite(row.latitude)||!Number.isFinite(row.longitude)){unlocated++;continue}
  const capturedAt=explicitUtc(row.capture_time_utc)||fromLocalAndOffset(row.capture_time_local,row.utc_offset);
  if(!capturedAt){untimed++;continue}
  candidates.push({row,capturedAt,capturedUntil:explicitUtc(row.capture_time_end_utc)});
}

let cursor=0,finished=0;
const items=Array.from({length:candidates.length});
async function worker(){while(cursor<candidates.length){const index=cursor++,{row,capturedAt,capturedUntil}=candidates[index],name=key(row.record_id),preview=path.join(output,'previews',`${name}.jpg`),thumbnail=path.join(output,'thumbnails',`${name}.jpg`),video=row.media_type==='timelapse video';
    try{
      if(!await exists(preview)){
        if(video)await run('ffmpeg',['-loglevel','error','-ss','2','-i',row.local_path,'-frames:v','1','-vf','scale=1280:1280:force_original_aspect_ratio=decrease','-q:v','3','-y',preview]);
        else try{await run('convert',[row.local_path,'-auto-orient','-thumbnail','1280x1280>','-strip','-quality','82',preview])}catch{await run('ffmpeg',['-loglevel','error','-i',row.local_path,'-frames:v','1','-vf','scale=1280:1280:force_original_aspect_ratio=decrease','-q:v','3','-y',preview])}
      }
      if(!await exists(thumbnail))await run('convert',[preview,'-thumbnail','240x160^','-gravity','center','-extent','240x160','-strip','-quality','78',thumbnail]);
      let mediaUrl=null;
      if(video){const media=path.join(output,'videos',`${name}.mp4`);if(!await exists(media))await run('ffmpeg',['-loglevel','error','-i',row.local_path,'-vf','scale=1280:720:force_original_aspect_ratio=decrease,pad=ceil(iw/2)*2:ceil(ih/2)*2','-c:v','libx264','-preset','veryfast','-crf','28','-an','-movflags','+faststart','-y',media]);mediaUrl=`${baseUrl}/videos/${name}.mp4`}
      items[index]={id:row.record_id,source:row.source,kind:video?'timelapse':'image',title:row.title||null,creator:row.creator||null,location:row.location_text||row.geocode_display_name||'Location unavailable',latitude:row.latitude,longitude:row.longitude,capturedAt,capturedUntil,timePrecision:row.time_precision||'unknown',timeSource:row.time_source||'unknown',coordinateSource:row.coordinate_source||'unknown',coordinatePrecision:row.coordinate_precision||'unknown',grade:row.spatiotemporal_grade||'ungraded',thumbnailUrl:`${baseUrl}/thumbnails/${name}.jpg`,previewUrl:`${baseUrl}/previews/${name}.jpg`,mediaUrl,sourceUrl:row.source_url||row.original_url,originalUrl:row.original_url||null,license:row.license||null,rightsNote:row.rights_note||null};
    }catch(error){console.error(`SKIP ${row.record_id}: ${error.message}`)}
    finished++;if(finished%50===0||finished===candidates.length)console.error(`${finished}/${candidates.length} prepared`);
  }}
await Promise.all(Array.from({length:Math.min(8,candidates.length)},()=>worker()));
const ready=items.filter(Boolean).sort((a,b)=>Date.parse(a.capturedAt)-Date.parse(b.capturedAt)||a.id.localeCompare(b.id));
const candidateMinutes=[1,2,5,10,15,30,60];let binMinutes=60;
for(const minutes of candidateMinutes){const width=minutes*60_000,counts=new Map;for(const item of ready){const bin=Math.floor(Date.parse(item.capturedAt)/width);counts.set(bin,(counts.get(bin)||0)+1)}const ordered=[...counts.values()].sort((a,b)=>a-b),p90=ordered[Math.min(ordered.length-1,Math.floor(ordered.length*.9))]||0;if(p90>=16&&p90<=24){binMinutes=minutes;break}}
const manifest={schema:'gaia-event-study-v1',eventId,title:'The 11–12 November 2025 auroral storm',description:'Ground photographs and timelapses collected for the November 2025 G4 geomagnetic storm. Every preview retains its source, location, timestamp basis, coordinate basis and rights note.',generatedAt:new Date().toISOString(),binMinutes,items:ready,excluded:{untimed,unlocated,unavailable}};
await writeFile(path.join(output,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
console.log(JSON.stringify({output,items:ready.length,binMinutes,excluded:manifest.excluded},null,2));
