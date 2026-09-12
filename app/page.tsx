'use client';

import {liveCutoff,LIVE_DELAY_MINUTES} from '../src/live-time';
import { Activity, Aperture, CircleHelp, Database, Lightbulb, Satellite, Send, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import GaiaGlobeView from '../src/GaiaGlobeView';

type ViewName = 'globe' | 'cameras' | 'status' | 'calibrate' | 'about';
type Camera = {id:string;name:string;producer:string;state:string;timestamp_mode:string;latitude_deg:number|null;longitude_deg:number|null;calibrated:boolean;enabled:boolean;quality_exponent:number;images_24h:number;message?:string|null};
type CameraFrame = {id:string;observation_utc:string;width:number|null;height:number|null};
type Calibration = {id:string;created_utc:string;valid_from_utc:string|null;valid_to_utc:string|null;method:string;residual_px:number|null;star_count:number|null;selected:boolean};
// Playback step interval is PLAYBACK_STEP_MS divided by the chosen speed.
const PLAYBACK_STEP_MS = 150;
const SPEEDS = [0.25, 0.5, 1, 2, 4, 8, 16, 32] as const;
type StarSummary = {star_key:string;vt_mag:number;ra_hours_j2000:number;dec_deg_j2000:number;image_x:number|null;image_y:number|null;elevation_deg:number|null;frames:number;found:number;median_flux:number|null;clear_flux:number|null;median_background:number|null;variation:number|null};
type StarSample = {at:string;flux:number|null;background:number|null;sigma_major:number|null;sigma_minor:number|null;angle_deg:number|null;centroid_offset_px:number|null;elevation_deg:number};
const STAR_CHANNELS = ['mean','r','g','b'] as const;
/// Steady stars read cool, strongly varying ones warm.
const variationColour = (v:number|null) => v==null?'#5d7080':`hsl(${Math.round(190-190*Math.min(1,Math.max(0,v)))} 78% 55%)`;
type SortKey = 'name' | 'latitude_deg' | 'longitude_deg' | 'calibrated';
// Manual per-camera quality weight, offered as powers of two from 1 to 1/256.
const qualitySteps = [0,-1,-2,-3,-4,-5,-6,-7,-8];
const sortOptions:[SortKey,string][] = [['name','Name'],['latitude_deg','Latitude'],['longitude_deg','Longitude'],['calibrated','Calibration']];

const fallbackCameras:Camera[]=[
  {id:'starvisor-popovo',name:'STARVISOR Popovo',producer:'Popovo camera operator / STARVISOR',state:'waiting',timestamp_mode:'download_time',latitude_deg:57,longitude_deg:38,calibrated:false,enabled:true,quality_exponent:0,images_24h:0},
  {id:'irf-kiruna-alis',name:'IRF Kiruna all-sky',producer:'Swedish Institute of Space Physics',state:'waiting',timestamp_mode:'archive',latitude_deg:67.84,longitude_deg:20.41,calibrated:false,enabled:true,quality_exponent:0,images_24h:0},
];

function LocationEditor({camera,onClose,onSaved}:{camera:Camera,onClose:()=>void,onSaved:(camera:Camera)=>void}){
  const [lat,setLat]=useState(camera.latitude_deg??0);const[lon,setLon]=useState(camera.longitude_deg??0);const[saving,setSaving]=useState(false);
  const point=(event:React.PointerEvent<HTMLDivElement>)=>{const r=event.currentTarget.getBoundingClientRect();setLon(Math.max(-180,Math.min(180,(event.clientX-r.left)/r.width*360-180)));setLat(Math.max(-90,Math.min(90,90-(event.clientY-r.top)/r.height*180)))};
  async function save(){setSaving(true);try{const response=await fetch(`/gaia/api/sources/${camera.id}/location`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({latitude_deg:lat,longitude_deg:lon})});if(!response.ok)throw new Error();onSaved({...camera,latitude_deg:lat,longitude_deg:lon});onClose()}catch{alert('The camera service is not available yet. Please try again shortly.')}finally{setSaving(false)}}
  return <div className="modal-backdrop"><div className="location-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CAMERA POSITION</span><h2>{camera.name}</h2><p>Drag the marker or enter exact WGS84 coordinates.</p><div className="location-map" onPointerDown={point} onPointerMove={e=>{if(e.buttons)point(e)}}><span className="map-equator"/><span className="map-meridian"/><i style={{left:`${(lon+180)/360*100}%`,top:`${(90-lat)/180*100}%`}}/></div><div className="form-row"><label>Latitude<input type="number" min="-90" max="90" step="0.0001" value={lat.toFixed(4)} onChange={e=>setLat(Number(e.target.value))}/></label><label>Longitude<input type="number" min="-180" max="180" step="0.0001" value={lon.toFixed(4)} onChange={e=>setLon(Number(e.target.value))}/></label></div><button className="send-button" onClick={save} disabled={saving}>{saving?'Saving…':'Save camera position'}</button></div></div>
}

function MaskEditor({camera,onClose}:{camera:Camera,onClose:()=>void}){
  const[mode,setMode]=useState<'crop'|'mask'|'nodes'>('mask');
  const[loading,setLoading]=useState(true);const[loadError,setLoadError]=useState('');const[maskEnabled,setMaskEnabled]=useState(true);
  useEffect(()=>{
    const controller=new AbortController();
    setLoading(true);setLoadError('');
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/settings`,{cache:'no-store',signal:controller.signal}).then(async response=>{
      if(!response.ok)throw new Error('Could not load saved crop and mask. Close and reopen to retry.');
      const settings=await response.json();
      if(controller.signal.aborted)return;
      setMaskEnabled(settings.mask_enabled!==false);
      setCrop(settings.crop?[settings.crop.left,settings.crop.top,settings.crop.right,settings.crop.bottom].map(v=>v*100):[0,0,100,100]);
      setPolygons(settings.mask?.polygons?.length?settings.mask.polygons:[[]]);setActiveMask(0);
      setLoading(false);
    }).catch(error=>{if(!controller.signal.aborted){setLoadError(String(error.message));setLoading(false)}});
    return()=>controller.abort();
  },[camera.id]);
  const[polygons,setPolygons]=useState<[number,number][][]>([[]]);const[activeMask,setActiveMask]=useState(0);
  const points=polygons[activeMask]??[];
  const setPoints=(value:[number,number][]|((p:[number,number][])=>[number,number][]))=>setPolygons(all=>all.map((p,i)=>i===activeMask?(typeof value==='function'?value(p):value):p));
  const[crop,setCrop]=useState([0,0,100,100]);const[saving,setSaving]=useState(false);const imageRef=useRef<HTMLImageElement>(null);const drag=useRef<{kind:'node'|'crop';edge?:string;index?:number;x:number;y:number;points:[number,number][];crop:number[]}|null>(null);
  const normalized=(event:React.PointerEvent)=>{const r=imageRef.current!.getBoundingClientRect();return[(event.clientX-r.left)/r.width,(event.clientY-r.top)/r.height] as [number,number]};
  const pointerDown=(event:React.PointerEvent<HTMLDivElement>)=>{
    if(!imageRef.current||event.button!==0)return;
    event.preventDefault();drag.current=null;
    const target=event.target as HTMLElement,kind=target.dataset.kind,[x,y]=normalized(event);
    if(mode==='mask'){
      // Drawing never grabs a node or captures the pointer: every press adds one.
      setPoints(p=>[...p,[Math.max(-.25,Math.min(1.25,x)),Math.max(-.25,Math.min(1.25,y))]]);
      return;
    }
    if(kind==='node'&&mode==='nodes')drag.current={kind:'node',index:Number(target.dataset.index),x,y,points,crop};
    else if(kind==='crop'&&mode==='crop')drag.current={kind:'crop',edge:target.dataset.edge,x,y,points,crop};
    if(drag.current)event.currentTarget.setPointerCapture(event.pointerId);
  };
  const pointerMove=(event:React.PointerEvent<HTMLDivElement>)=>{
    const d=drag.current;if(!d||!imageRef.current)return;
    const[x,y]=normalized(event),dx=x-d.x,dy=y-d.y;
    if(d.kind==='node')setPoints(d.points.map((p,i)=>i===d.index?[Math.max(-.25,Math.min(1.25,p[0]+dx)),Math.max(-.25,Math.min(1.25,p[1]+dy))]:p));
    else if(d.edge){
      const next=[...d.crop];
      if(d.edge.includes('w'))next[0]=Math.max(0,Math.min(d.crop[2]-1,d.crop[0]+dx*100));
      if(d.edge.includes('e'))next[2]=Math.min(100,Math.max(d.crop[0]+1,d.crop[2]+dx*100));
      if(d.edge.includes('n'))next[1]=Math.max(0,Math.min(d.crop[3]-1,d.crop[1]+dy*100));
      if(d.edge.includes('s'))next[3]=Math.min(100,Math.max(d.crop[1]+1,d.crop[3]+dy*100));
      setCrop(next);
    }else{const w=d.crop[2]-d.crop[0],h=d.crop[3]-d.crop[1],left=Math.max(0,Math.min(100-w,d.crop[0]+dx*100)),top=Math.max(0,Math.min(100-h,d.crop[1]+dy*100));setCrop([left,top,left+w,top+h])}
  };
  async function save(){if(loading||loadError)return;if(polygons.some(p=>p.length>0&&p.length<3)){alert('Each mask needs at least three points. Finish or delete the incomplete mask before saving.');return;}setSaving(true);try{const response=await fetch(`/gaia/api/sources/${camera.id}/settings`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({crop:{left:crop[0]/100,top:crop[1]/100,right:crop[2]/100,bottom:crop[3]/100},mask:{coordinate_system:'normalized_image',polygons:polygons.filter(p=>p.length>=3)},mask_enabled:maskEnabled})});if(!response.ok)throw new Error();onClose()}catch{alert('Could not save crop and mask.')}finally{setSaving(false)}}
  if(loading||loadError)return <div className="modal-backdrop"><div className="mask-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><h2>{camera.name}</h2><p role="status">{loadError||'Loading saved crop and mask…'}</p></div></div>;
  return <div className="modal-backdrop"><div className="mask-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CROP &amp; OBSTRUCTION MASK</span><h2>{camera.name}</h2><p>Drag inside the cyan crop rectangle to move it. Drag any edge or corner to resize. Draw mask adds a node on every click, including outside the image. Choose Move nodes to drag existing nodes. Each mask is a separate excluded obstruction area; use New mask for another tree, building, or other obstruction.</p><label className="mask-switch" title="Ignore this camera's obstruction outlines without deleting them"><input type="checkbox" checked={maskEnabled} onChange={event=>setMaskEnabled(event.target.checked)}/><span><strong>Use this mask</strong><small>{maskEnabled?'obstruction areas are excluded from the mosaic':'outlines are kept on record but ignored'}</small></span></label><div className={`mask-actions${maskEnabled?'':' mask-off'}`}><button aria-pressed={mode==='crop'} onClick={()=>setMode('crop')}>Crop — move / resize</button><button aria-pressed={mode==='mask'} onClick={()=>setMode('mask')}>Draw mask — add nodes</button><button aria-pressed={mode==='nodes'} onClick={()=>setMode('nodes')}>Move nodes</button><span role="status">{points.length} mask nodes</span></div><div className="mask-actions mask-selector"><button onClick={()=>{drag.current=null;setPolygons(all=>[...all,[]]);setActiveMask(polygons.length);setMode('mask')}}>+ New mask</button>{polygons.map((p,i)=><button key={i} aria-pressed={activeMask===i} onClick={()=>{drag.current=null;setActiveMask(i);setMode('nodes')}}>Mask {i+1} ({p.length} nodes)</button>)}<button onClick={()=>{drag.current=null;setPolygons(all=>all.length===1?[[]]:all.filter((_,i)=>i!==activeMask));setActiveMask(0)}}>Delete selected mask</button></div><div className={`mask-image editing-${mode}`} onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={()=>drag.current=null} onPointerCancel={()=>drag.current=null} onLostPointerCapture={()=>drag.current=null}><div className="mask-frame"><img draggable={false} ref={imageRef} src={`/gaia/api/sources/${camera.id}/latest`} alt={`Latest frame from ${camera.name}`}/><svg viewBox="0 0 1 1" preserveAspectRatio="none">{polygons.map((polygon,i)=><polygon key={i} style={{opacity:i===activeMask?1:.45}} points={polygon.map(p=>p.join(',')).join(' ')}/>)}<rect className="crop-box" data-kind="crop" x={crop[0]/100} y={crop[1]/100} width={(crop[2]-crop[0])/100} height={(crop[3]-crop[1])/100}/><rect className="crop-grab" data-kind="crop" x={crop[0]/100} y={crop[1]/100} width={(crop[2]-crop[0])/100} height=".035"/></svg>{points.map((p,i)=><span className="mask-node-dot" data-kind="node" data-index={i} key={i} style={{left:`${p[0]*100}%`,top:`${p[1]*100}%`}}/>)}{mode==='crop'&&<div className="crop-interaction" data-kind="crop" style={{left:`${crop[0]}%`,top:`${crop[1]}%`,width:`${crop[2]-crop[0]}%`,height:`${crop[3]-crop[1]}%`}}>{['n','s','e','w','nw','ne','sw','se'].map(edge=><span key={edge} data-kind="crop" data-edge={edge} className={`crop-handle handle-${edge}`} title={`Resize crop ${edge}`}/>)}</div>}</div></div><details><summary>Exact crop coordinates (optional)</summary><div className="crop-grid">{['Left','Top','Right','Bottom'].map((label,i)=><label key={label}>{label}<input type="number" min="0" max="100" step="0.1" value={Number(crop[i].toFixed(1))} onChange={e=>setCrop(v=>v.map((x,j)=>j===i?Number(e.target.value):x))}/></label>)}</div></details><div className="mask-actions"><button onClick={()=>setPoints(p=>p.slice(0,-1))}>Undo point</button><button onClick={()=>setPoints([])}>Clear selected mask</button><button className="send-button" onClick={save} disabled={saving}>{saving?'Saving…':'Save crop and mask'}</button></div></div></div>
}

function FrameBrowser({camera,onClose}:{camera:Camera,onClose:()=>void}){
  const[frames,setFrames]=useState<CameraFrame[]>([]);const[index,setIndex]=useState(0);const[error,setError]=useState('');
  useEffect(()=>{const controller=new AbortController();void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/frames`,{cache:'no-store',signal:controller.signal}).then(async response=>{if(!response.ok)throw new Error('Could not load frame history.');return await response.json() as CameraFrame[]}).then(rows=>{setFrames(rows);setIndex(Math.max(0,rows.length-1))}).catch(reason=>{if(!controller.signal.aborted)setError(String(reason.message||reason))});return()=>controller.abort()},[camera.id]);
  const frame=frames[index];
  return <div className="modal-backdrop"><div className="frame-browser"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CAMERA FRAME HISTORY</span><h2>{camera.name}</h2><p>Browse the last 24 hours and open the exact selected archived frame in AIDA for calibration.</p>{error?<p role="alert">{error}</p>:!frame?<div className="history-empty">No archived frames in the last 24 hours.</div>:<><div className="history-image"><img src={`/gaia/api/images/${encodeURIComponent(frame.id)}/original`} alt={`${camera.name} at ${frame.observation_utc}`}/></div><div className="history-meta"><time>{new Date(frame.observation_utc).toISOString().replace('T',' ').replace('.000Z',' UTC')}</time><span>{index+1} / {frames.length}{frame.width&&frame.height?` · ${frame.width} × ${frame.height}`:''}</span></div><input className="history-slider" aria-label="Historical camera frame" type="range" min="0" max={Math.max(0,frames.length-1)} step="1" value={index} onChange={event=>setIndex(Number(event.target.value))}/><div className="history-actions"><button onClick={()=>setIndex(value=>Math.max(0,value-1))} disabled={index===0}>← Previous</button><button onClick={()=>setIndex(value=>Math.min(frames.length-1,value+1))} disabled={index===frames.length-1}>Next →</button><a className="send-button" href={`/aida/?gaia=1&source_id=${encodeURIComponent(camera.id)}&image_id=${encodeURIComponent(frame.id)}`} target="_blank" rel="noopener noreferrer">Calibrate selected frame in AIDA ↗</a></div></>}</div></div>;
}

function Credits(){
  const[providers,setProviders]=useState<{name:string;website:string|null;acknowledgement:string;copyright:string}[]>([]);
  const[error,setError]=useState(false);
  useEffect(()=>{void fetch('/gaia/api/credits').then(r=>{if(!r.ok)throw new Error();return r.json()}).then(setProviders).catch(()=>setError(true))},[]);
  return <section aria-label="Data provider credits"><h3>Data provider credits</h3>{error?<p>Could not load provider credits. Please reopen this page to retry.</p>:providers.length?providers.map((p,i)=><article key={i}><h4>{p.website&&/^https?:\/\//.test(p.website)?<a href={p.website} target="_blank" rel="noopener noreferrer">{p.name} ↗</a>:p.name}</h4><p>{p.acknowledgement}</p><p>{p.copyright}</p></article>):<p>Loading credits…</p>}</section>;
}

/// Per-star brightness and background over time, and where those stars sit in
/// the frame. Cloud shows up as a star dimming, so these are the raw inputs to
/// the cloud-thickness estimate rather than a derived product.
function StarPhotometry({camera,onClose}:{camera:Camera;onClose:()=>void}){
  const [channel,setChannel]=useState<string>('mean');
  const [hours,setHours]=useState(24);
  const [stars,setStars]=useState<StarSummary[]>([]);
  const [selected,setSelected]=useState<string[]>([]);
  const [series,setSeries]=useState<Record<string,StarSample[]>>({});
  const [loading,setLoading]=useState(true),[error,setError]=useState('');
  useEffect(()=>{
    const controller=new AbortController();setLoading(true);setError('');
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars?channel=${channel}&hours=${hours}`,{cache:'no-store',signal:controller.signal})
      .then(async r=>{if(!r.ok)throw new Error('Could not load star photometry.');return await r.json() as {stars:StarSummary[]}})
      .then(body=>{const rows=body.stars.filter(x=>x.image_x!=null&&x.image_y!=null);setStars(rows);
        setSelected(prev=>{const keep=prev.filter(k=>rows.some(r=>r.star_key===k));
          if(keep.length)return keep;
          return [...rows].sort((a,b)=>(b.found)-(a.found)).slice(0,3).map(r=>r.star_key)});})
      .catch(e=>{if(!controller.signal.aborted)setError(String(e.message||e))})
      .finally(()=>{if(!controller.signal.aborted)setLoading(false)});
    return()=>controller.abort();
  },[camera.id,channel,hours]);
  useEffect(()=>{
    const controller=new AbortController();
    for(const key of selected){
      if(series[`${channel}:${key}`])continue;
      void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars/series?star=${encodeURIComponent(key)}&channel=${channel}&hours=${hours}`,{cache:'no-store',signal:controller.signal})
        .then(async r=>r.ok?await r.json() as {samples:StarSample[]}:{samples:[]})
        .then(body=>setSeries(prev=>({...prev,[`${channel}:${key}`]:body.samples})))
        .catch(()=>{});
    }
    return()=>controller.abort();
  },[selected,channel,hours,camera.id,series]);
  const toggle=(key:string)=>setSelected(prev=>prev.includes(key)?prev.filter(k=>k!==key):[...prev,key]);
  const palette=['#56f0c5','#f3b647','#72ccef','#ff8fa3','#baff78','#c9a0ff'];
  const picked=selected.map((k,i)=>({key:k,colour:palette[i%palette.length],samples:series[`${channel}:${k}`]||[]}));
  const times=picked.flatMap(p=>p.samples.map(x=>Date.parse(x.at))).filter(Number.isFinite);
  const t0=times.length?Math.min(...times):0,t1=times.length?Math.max(...times):1;
  const band=(get:(x:StarSample)=>number|null)=>{
    const vals=picked.flatMap(p=>p.samples.map(get)).filter((v):v is number=>v!=null&&Number.isFinite(v));
    if(!vals.length)return[0,1];const lo=Math.min(...vals),hi=Math.max(...vals);
    return hi>lo?[lo,hi]:[lo-1,hi+1];};
  const path=(samples:StarSample[],get:(x:StarSample)=>number|null,lo:number,hi:number,w:number,h:number)=>{
    let d='',open=false;
    for(const sample of samples){const v=get(sample);const t=Date.parse(sample.at);
      if(v==null||!Number.isFinite(v)||!Number.isFinite(t)){open=false;continue}
      const x=t1>t0?(t-t0)/(t1-t0)*w:w/2, y=h-(v-lo)/(hi-lo)*h;
      d+=`${open?'L':'M'}${x.toFixed(1)} ${y.toFixed(1)} `;open=true;}
    return d.trim();};
  const plot=(label:string,get:(x:StarSample)=>number|null,unit:string)=>{
    const [lo,hi]=band(get);const w=560,h=118;
    return <div className="star-plot"><div className="star-plot-head"><strong>{label}</strong><small>{lo.toPrecision(3)} to {hi.toPrecision(3)} {unit}</small></div>
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label={label}>
        <rect x="0" y="0" width={w} height={h} className="star-plot-bg"/>
        {[0.25,0.5,0.75].map(f=><line key={f} x1="0" x2={w} y1={h*f} y2={h*f} className="star-grid"/>)}
        {picked.map(p=><path key={p.key} d={path(p.samples,get,lo,hi,w,h)} fill="none" stroke={p.colour} strokeWidth="1.4"/>)}
      </svg></div>;};
  const extent=(pick:(r:StarSummary)=>number)=>{
    const vals=stars.map(pick).filter(Number.isFinite);
    if(!vals.length)return[0,1];const lo=Math.min(...vals),hi=Math.max(...vals);
    const pad=Math.max(8,(hi-lo)*0.06);return[lo-pad,hi+pad];};
  const [xlo,xhi]=extent(r=>r.image_x as number),[ylo,yhi]=extent(r=>r.image_y as number);
  const S=330;
  return <div className="modal-backdrop"><div className="star-photometry"><button className="close" onClick={onClose} aria-label="Close"><X/></button>
    <span className="eyebrow">STAR PHOTOMETRY</span><h2>{camera.name}</h2>
    <p>Brightness of catalogue stars brighter than magnitude 4, fitted with a rotated two-dimensional Gaussian in each colour channel. A star dimming against its own clear-sky level is cloud along that line of sight.</p>
    <div className="star-controls">
      <span className="eyebrow">CHANNEL</span>
      {STAR_CHANNELS.map(c=><button key={c} type="button" aria-pressed={channel===c} onClick={()=>setChannel(c)}>{c==='mean'?'mean':c.toUpperCase()}</button>)}
      <span className="eyebrow">WINDOW</span>
      {[6,24,72].map(v=><button key={v} type="button" aria-pressed={hours===v} onClick={()=>setHours(v)}>{v} h</button>)}
      <small role="status">{loading?'Loading\u2026':`${stars.length} stars, ${selected.length} selected`}</small>
    </div>
    {error&&<p role="alert">{error}</p>}
    {!loading&&!error&&stars.length===0&&<div className="history-empty">No star photometry recorded for this camera yet.</div>}
    {stars.length>0&&<div className="star-panels">
      <div className="star-series">
        {plot('Total intensity above background',x=>x.flux,'counts')}
        {plot('Fitted image background',x=>x.background,'counts')}
        <div className="star-legend">{picked.map(p=>{const row=stars.find(r=>r.star_key===p.key);
          return <span key={p.key}><i style={{background:p.colour}}/>{row?`V ${row.vt_mag.toFixed(2)} at ${row.elevation_deg?.toFixed(0)}\u00b0 el`:p.key}<small>{p.samples.length} pts</small></span>})}
          {picked.length===0&&<small>Select stars in the scatter to plot them.</small>}</div>
      </div>
      <div className="star-scatter">
        <div className="star-plot-head"><strong>Star positions in the frame</strong><small>colour is brightness variation</small></div>
        <svg viewBox={`0 0 ${S} ${S}`} role="img" aria-label="Star image positions coloured by brightness variation">
          <rect x="0" y="0" width={S} height={S} className="star-plot-bg"/>
          {stars.map(r=>{const x=((r.image_x as number)-xlo)/(xhi-xlo)*S, y=((r.image_y as number)-ylo)/(yhi-ylo)*S;
            const on=selected.includes(r.star_key);
            return <g key={r.star_key} className={on?'chosen':''} onClick={()=>toggle(r.star_key)}>
              <circle cx={x} cy={y} r={Math.max(2.4,6.2-r.vt_mag*1.1)} fill={variationColour(r.variation)}
                stroke={on?'#fff':'none'} strokeWidth={on?1.6:0}>
              </circle><title>{`V ${r.vt_mag.toFixed(2)}  elevation ${r.elevation_deg?.toFixed(1)}\u00b0
found in ${r.found} of ${r.frames} frames
variation ${r.variation==null?'n/a':r.variation.toFixed(2)}
clear-sky flux ${r.clear_flux==null?'n/a':r.clear_flux.toPrecision(4)}`}</title></g>})}
        </svg>
        <div className="star-ramp"><small>steady</small>
          {[0,0.2,0.4,0.6,0.8,1].map(v=><i key={v} style={{background:variationColour(v)}}/>)}
          <small>obscured</small></div>
        <small className="star-hint">Click a star to add or remove it from the plots. Marker size follows catalogue magnitude.</small>
      </div>
    </div>}
  </div></div>;
}

const stamp=(value:string|null)=>value?new Date(value).toISOString().replace('T',' ').slice(0,16)+' UTC':null;

/// Compare the calibrations held for one camera and choose which one maps it.
function CalibrationPicker({camera,onClose}:{camera:Camera;onClose:()=>void}){
  const [rows,setRows]=useState<Calibration[]>([]),[selected,setSelected]=useState<string|null>(null);
  const [loading,setLoading]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState('');
  async function refresh(){
    try{
      const response=await fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/calibrations`,{cache:'no-store'});
      if(!response.ok)throw new Error('Could not load calibrations.');
      const body=await response.json() as {selected_calibration_id:string|null;calibrations:Calibration[]};
      setRows(body.calibrations);setSelected(body.selected_calibration_id);setError('');
    }catch(reason){setError(String((reason as Error).message||reason))}finally{setLoading(false)}
  }
  useEffect(()=>{void refresh()},[camera.id]);
  async function choose(id:string|null){
    setBusy(true);
    try{
      const response=await fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/calibrations`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({calibration_id:id})});
      if(!response.ok)throw new Error('Could not switch calibration.');
      await refresh();
    }catch(reason){setError(String((reason as Error).message||reason))}finally{setBusy(false)}
  }
  return <div className="modal-backdrop"><div className="calibration-picker"><button className="close" onClick={onClose} aria-label="Close"><X/></button>
    <span className="eyebrow">LENS CALIBRATIONS</span><h2>{camera.name}</h2>
    <p>Every calibration ever sent for this camera is kept, with the star fit it came from. Choose which one is used to map the camera onto the 100 km shell, or leave it automatic to follow each calibration&rsquo;s validity interval.</p>
    {error&&<p role="alert">{error}</p>}
    {loading?<p role="status">Loading calibrations&hellip;</p>:rows.length===0?<div className="history-empty">No calibration has been sent for this camera yet.</div>:<>
      {/* An explicit radiogroup rather than <input type=radio>: controlled radios
          sharing a name let the browser's own group behaviour diverge from React's
          state, which showed a calibration as chosen while none was selected. */}
      <div className="calibration-list" role="radiogroup" aria-label="Calibration used for mapping">
        <button type="button" role="radio" aria-checked={selected===null} disabled={busy} className={selected===null?'chosen':''} onClick={()=>void choose(null)}>
          <span className="mark" aria-hidden="true"/>
          <span><strong>Automatic</strong><small>Use whichever calibration is valid at each frame&rsquo;s observation time</small></span></button>
        {rows.map(row=><button key={row.id} type="button" role="radio" aria-checked={row.selected} disabled={busy} className={row.selected?'chosen':''} onClick={()=>void choose(row.id)}>
          <span className="mark" aria-hidden="true"/>
          <span><strong>{stamp(row.created_utc)}</strong>
            <small>{row.star_count==null?'star count unavailable':`${row.star_count} identified star${row.star_count===1?'':'s'}`}
              {' \u00b7 '}{row.residual_px==null?'no residual recorded':`${row.residual_px.toFixed(3)} px RMS`}
              {' \u00b7 '}{row.method}</small>
            <small>{row.valid_from_utc||row.valid_to_utc?`valid ${stamp(row.valid_from_utc)||'from the start'} to ${stamp(row.valid_to_utc)||'open'}`:'valid at any time'}</small>
          </span></button>)}
      </div>
      <p className="calibration-note">{selected===null?'Automatic selection is active.':'A fixed calibration is active; its validity interval is ignored.'} Switching rebuilds this camera&rsquo;s projection mesh on the next publish.</p>
    </>}
  </div></div>;
}

function Globe({epochMillis,live,onCredits,onLoading,sunLock,zoomRef}:{epochMillis:number;live:boolean;onCredits:()=>void;onLoading:(loading:boolean)=>void;sunLock:boolean;zoomRef:{current:((action:'in'|'out'|'reset')=>void)|null}}) {
  return <GaiaGlobeView className="globe-stage" getEpochMillis={()=>live?liveCutoff():epochMillis} onLoading={onLoading} sunLock={sunLock} showTools={false} zoomRef={zoomRef}>
    <button type="button" onClick={onCredits} aria-label="Data provider credits" title="Data provider credits" style={{position:'absolute',left:18,top:18,zIndex:2,border:0,background:'#031018cc',color:'#d6eee8',borderRadius:'50%',width:32,height:32,fontSize:24,cursor:'pointer'}}>ⓘ</button>
  </GaiaGlobeView>;
}

export default function Home() {
  const historyEnd=useRef(liveCutoff());
  const[historyMinutes,setHistoryMinutes]=useState<number[]>([]);
  useEffect(()=>{const load=()=>void fetch('/gaia/api/history',{cache:'no-store'}).then(r=>r.json()).then((times:string[])=>setHistoryMinutes([...new Set(times.filter(t=>Date.parse(t)<=liveCutoff()).map(t=>Math.max(0,Math.min(1440,Math.ceil((Date.parse(t)+60000-historyEnd.current)/60000)+1440))))].sort((a,b)=>a-b))).catch(console.warn);load();const timer=setInterval(load,60000);return()=>clearInterval(timer)},[]);
  const[framesLoading,setFramesLoading]=useState(false);
  useEffect(()=>{const pause=()=>setPlaying(false);window.addEventListener('gaia-pause-playback',pause);return()=>window.removeEventListener('gaia-pause-playback',pause)},[]);
  const [view, setView] = useState<ViewName>('globe'); const [playing, setPlaying] = useState(false); const [sunLock,setSunLock]=useState(false); const [speed,setSpeed]=useState(32); const zoomRef=useRef<((action:'in'|'out'|'reset')=>void)|null>(null); const [instrumentSites,setInstrumentSites]=useState<Record<string,string>>({}); const [sortKey,setSortKey]=useState<SortKey>('name'); const[photometryCamera,setPhotometryCamera]=useState<Camera|null>(null); const[calibratingCamera,setCalibratingCamera]=useState<Camera|null>(null); const [sortDesc,setSortDesc]=useState(false); const [timeMinutes,setTimeMinutes]=useState(1440);const [suggesting,setSuggesting]=useState(false);const [sent,setSent]=useState(false);const[editingCamera,setEditingCamera]=useState<Camera|null>(null);const[maskingCamera,setMaskingCamera]=useState<Camera|null>(null);const[browsingCamera,setBrowsingCamera]=useState<Camera|null>(null);const[cameras,setCameras]=useState<Camera[]>(fallbackCameras);
  useEffect(()=>{void fetch('/gaia/public/manifest.json',{cache:'no-store'}).then(async r=>{if(!r.ok)throw new Error();return await r.json() as {cameras?:{source_id:string;website_url:string|null}[]}}).then(manifest=>setInstrumentSites(Object.fromEntries((manifest.cameras||[]).filter(c=>c.website_url).map(c=>[c.source_id,c.website_url as string])))).catch(()=>{})},[]);
  useEffect(()=>{void fetch('/gaia/api/sources',{cache:'no-store'}).then(async r=>{if(!r.ok)throw new Error();return await r.json() as Camera[]}).then(rows=>{if(rows.length)setCameras(rows)}).catch(()=>{})},[]);
  useEffect(()=>{window.dispatchEvent(new CustomEvent('gaia-run-overview',{detail:{active:playing&&view==='globe',speed}}));return()=>{window.dispatchEvent(new CustomEvent('gaia-run-overview',{detail:{active:false}}))}},[playing,speed,view]);
  useEffect(()=>{const move=(event:Event)=>setTimeMinutes(1440+((event as CustomEvent).detail-historyEnd.current)/60000);window.addEventListener('gaia-overview-epoch',move);return()=>window.removeEventListener('gaia-overview-epoch',move)},[]);
  const selectedEpoch=timeMinutes===1440?liveCutoff():historyEnd.current-(1440-timeMinutes)*60_000;
  const sortedCameras=useMemo(()=>{
    const dir=sortDesc?-1:1,byName=(a:Camera,b:Camera)=>a.name.localeCompare(b.name,undefined,{numeric:true,sensitivity:'base'});
    return [...cameras].sort((a,b)=>{
      if(sortKey==='name')return dir*byName(a,b);
      if(sortKey==='calibrated')return a.calibrated===b.calibrated?byName(a,b):dir*(a.calibrated?-1:1);
      const av=a[sortKey],bv=b[sortKey];
      if(av==null&&bv==null)return byName(a,b);
      if(av==null)return 1;
      if(bv==null)return -1;
      return av===bv?byName(a,b):dir*(av-bv);
    });
  },[cameras,sortKey,sortDesc]);
  const sortHint=sortKey==='name'?(sortDesc?'Z to A':'A to Z')
    :sortKey==='latitude_deg'?(sortDesc?'north to south':'south to north')
    :sortKey==='longitude_deg'?(sortDesc?'east to west':'west to east')
    :(sortDesc?'uncalibrated first':'calibrated first');
  const unlocated=cameras.filter(c=>c.latitude_deg==null||c.longitude_deg==null).length;
  async function submitSuggestion(e:React.FormEvent<HTMLFormElement>){e.preventDefault();const form=new FormData(e.currentTarget);const body={name:form.get('name')||undefined,contact:form.get('contact')||undefined,suggestion:form.get('suggestion'),page_url:location.href};try{const response=await fetch('/gaia/api/suggestions',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});if(!response.ok)throw new Error();setSent(true)}catch{alert('The suggestion service is not available yet. Please try again shortly.')}}
  async function toggleCamera(camera:Camera){const enabled=!camera.enabled;const response=await fetch(`/gaia/api/sources/${camera.id}/enabled`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({enabled})});if(response.ok)setCameras(rows=>rows.map(row=>row.id===camera.id?{...row,enabled}:row))}
  // Quality weight is stored as the exponent of a power of two so the menu
  // steps are exact and the mosaic weight is a clean 1, 1/2, 1/4 ... 1/256.
  // The crop and mask are read back and sent with the weight rather than
  // omitted. A backend that resolves missing fields to the stored values does
  // not need this, but one that writes the request straight through would erase
  // an operator's crop rectangle and obstruction outlines.
  async function setQuality(camera:Camera,quality_exponent:number){const previous=camera.quality_exponent;setCameras(rows=>rows.map(row=>row.id===camera.id?{...row,quality_exponent}:row));const url=`/gaia/api/sources/${encodeURIComponent(camera.id)}/settings`;try{const current=await fetch(url,{cache:'no-store'});const settings=current.ok?await current.json() as {crop?:unknown,mask?:unknown,mask_enabled?:boolean}:{};const body:Record<string,unknown>={quality_exponent,mask_enabled:settings.mask_enabled!==false};if(settings.crop)body.crop=settings.crop;if(settings.mask)body.mask=settings.mask;const response=await fetch(url,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});if(!response.ok)throw new Error()}catch{setCameras(rows=>rows.map(row=>row.id===camera.id?{...row,quality_exponent:previous}:row));alert('Could not save the quality weight.')}}
  async function removeCamera(camera:Camera){if(!window.confirm(`Remove ${camera.name} from the GAIA registry? Its archived images and calibrations will be preserved.`))return;const response=await fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}`,{method:'DELETE'});if(response.ok)setCameras(rows=>rows.filter(row=>row.id!==camera.id));else alert('Could not remove this camera.')}
  return <main className="app-shell">
    <header className="topbar">
      <button className="brand" onClick={() => setView('globe')} aria-label="GAIA home"><svg className="aurora-brand" viewBox="0 0 64 64" aria-hidden="true"><defs><linearGradient id="aurora-logo" x1="0" y1="0" x2="1" y2="1"><stop stopColor="#baff78"/><stop offset=".5" stopColor="#56f0c5"/><stop offset="1" stopColor="#559ef8"/></linearGradient></defs><circle cx="32" cy="34" r="24" fill="#092b3b" stroke="#67c8d7" strokeWidth="1.5"/><path d="M18 20l9-4 6 6-4 8 6 5-5 7-2 13-7-9 1-10-7-5zM44 21l7 7-5 8-8-3 1-9z" fill="#3d8580"/><path d="M9 30C1 18 16 5 34 7S63 20 54 30C44 41 15 37 9 26" fill="none" stroke="url(#aurora-logo)" strokeWidth="5" strokeLinecap="round"/><path d="M12 28C18 37 44 38 53 28M14 24l2 7M22 29l1 6M33 31v6M43 29l-1 6" fill="none" stroke="#baffae" strokeWidth="1.3" opacity=".8"/></svg><span><strong><i className="rook-mark" aria-hidden="true">♜</i> GAIA</strong><small>Global Auroral Image Archive</small></span></button>
      <nav aria-label="Main navigation">
        <button className={view === 'globe' ? 'active' : ''} onClick={() => setView('globe')}>Globe</button>
        <button className={view === 'cameras' ? 'active' : ''} onClick={() => setView('cameras')}>Cameras</button>
        <button className={view === 'status' ? 'active' : ''} onClick={() => setView('status')}>Data status</button>
        <button className={view === 'calibrate' ? 'active' : ''} onClick={() => setView('calibrate')}>Contribute</button>
        <button className={view === 'about' ? 'active' : ''} onClick={() => setView('about')}>About</button>
      </nav>
      <div className="live-pill"><span /> {timeMinutes===1440?`LIVE −${LIVE_DELAY_MINUTES} MIN`:'PLAYBACK'} · {new Date(selectedEpoch).toISOString().slice(11,16)} UTC</div>
    </header>
    <section className="workspace viewer-only">
      <section className="viewer">
        {view === 'globe' && <Globe onLoading={setFramesLoading} epochMillis={selectedEpoch} live={timeMinutes===1440} onCredits={()=>setView('about')} sunLock={sunLock} zoomRef={zoomRef}/>}
        {view === 'globe' && <div className="timeline-dock"><button onClick={()=>{if(!playing&&timeMinutes===1440&&historyMinutes.length)setTimeMinutes(historyMinutes[0]);setPlaying(!playing)}} aria-label={playing?'Pause 24 hour playback':'Play last 24 hours'}>{playing?'Ⅱ':'▶'}</button><div className="speed-control" role="group" aria-label="Playback speed"><button type="button" aria-label="Slow the animation down" title="Slow down" disabled={speed<=SPEEDS[0]} onClick={()=>setSpeed(v=>SPEEDS[Math.max(0,SPEEDS.indexOf(v as typeof SPEEDS[number])-1)])}>&minus;</button><span aria-live="polite">{speed<1?speed:speed.toFixed(0)}&times;</span><button type="button" aria-label="Speed the animation up" title="Speed up" disabled={speed>=SPEEDS[SPEEDS.length-1]} onClick={()=>setSpeed(v=>SPEEDS[Math.min(SPEEDS.length-1,SPEEDS.indexOf(v as typeof SPEEDS[number])+1)])}>+</button></div><label className="sun-lock" title="Hold the sun upwards; drag vertically to change the viewing tilt"><input type="checkbox" checked={sunLock} onChange={event=>setSunLock(event.target.checked)}/><span><strong>Sun up</strong><small>rotate vs. sun–earth line</small></span></label><div><div className="timeline-label"><strong>LAST 24 HOURS</strong><time>{new Date(selectedEpoch).toISOString().replace('T',' ').slice(0,19)} UTC {framesLoading?'· loading':''}</time></div><input type="range" min="0" max="1440" step="1" value={timeMinutes} onChange={e=>{setTimeMinutes(Number(e.target.value));setPlaying(false);window.dispatchEvent(new CustomEvent('gaia-scrub-overview',{detail:historyEnd.current-(1440-Number(e.target.value))*60000}))}}/><div className="timeline-ticks"><span>−24 h</span><span>−18 h</span><span>−12 h</span><span>−6 h</span><span>now −{LIVE_DELAY_MINUTES} min</span></div></div></div>}
        {view === 'cameras' && <div className="route-panel"><div className="route-heading"><Satellite/><div><span className="eyebrow">CAMERA REGISTRY</span><h2>{cameras.length} image sources</h2></div></div><p className="panel-intro">Red cameras can be ingested and credited, but cannot enter the 100 km mosaic until their lens model is fitted.</p><div className="camera-sort" role="group" aria-label="Sort camera stations"><span className="eyebrow">SORT BY</span>{sortOptions.map(([key,label])=><button key={key} type="button" aria-pressed={sortKey===key} onClick={()=>{if(sortKey===key){setSortDesc(!sortDesc)}else{setSortKey(key);setSortDesc(false)}}} title={sortKey===key?`Reverse the ${label.toLowerCase()} order`:`Sort by ${label.toLowerCase()}`}>{label}{sortKey===key?(sortDesc?' \u25bc':' \u25b2'):''}</button>)}<small role="status">{sortHint}{(sortKey==='latitude_deg'||sortKey==='longitude_deg')&&unlocated>0?` \u00b7 ${unlocated} without a location last`:''}</small></div><div className="camera-list">{sortedCameras.map(camera=><article key={camera.id} className={camera.enabled?'':'camera-disabled'}><div className="camera-identity"><div className="camera-thumbnail"><span>NO FRAME</span><img loading="lazy" src={`/gaia/api/sources/${camera.id}/latest`} alt={`Current ${camera.name} frame`} onLoad={e=>e.currentTarget.classList.add('loaded')}/></div><div>{instrumentSites[camera.id]?<a className="camera-link" href={instrumentSites[camera.id]} target="_blank" rel="noreferrer noopener" title={`Open the ${camera.name} instrument page`}><strong>{camera.name}</strong> ↗</a>:<strong>{camera.name}</strong>}<small>{camera.producer}</small></div></div><div className="camera-meta"><span>{camera.latitude_deg==null||camera.longitude_deg==null?'Location needed':`${Math.abs(camera.latitude_deg).toFixed(2)}°${camera.latitude_deg>=0?'N':'S'} · ${Math.abs(camera.longitude_deg).toFixed(2)}°${camera.longitude_deg>=0?'E':'W'}`}</span><small>{camera.timestamp_mode==='archive'?'Archive timestamp':'Download timestamp'} · {camera.images_24h} images / 24 h</small><div><button type="button" onClick={()=>setBrowsingCamera(camera)}>Browse history</button><button type="button" onClick={()=>setCalibratingCamera(camera)}>Calibrations</button><button type="button" onClick={()=>setPhotometryCamera(camera)}>Star photometry</button><button type="button" onClick={()=>setEditingCamera(camera)}>Adjust location</button><button type="button" onClick={()=>setMaskingCamera(camera)}>Edit crop &amp; mask</button><button type="button" onClick={()=>void toggleCamera(camera)}>{camera.enabled?'Pause camera':'Resume camera'}</button><button className="danger" type="button" onClick={()=>void removeCamera(camera)}>Remove camera</button></div><label className="camera-quality" title="Down-weight a camera whose images are noisier, softer or less well exposed than the rest. The mosaic divides by the sum of the weights, so this only matters where projections overlap."><span>Quality weight</span><select value={camera.quality_exponent??0} onChange={event=>void setQuality(camera,Number(event.target.value))}>{qualitySteps.map(exponent=><option key={exponent} value={exponent}>{exponent===0?'1 \u00b7 full weight':`1/${2**-exponent} \u00b7 2^${exponent}`}</option>)}</select></label></div><div className={`calibration-state ${camera.calibrated?'calibrated':''}`}><b>{camera.enabled?(camera.calibrated?'CALIBRATED':'NOT CALIBRATED'):'PAUSED'}</b>{camera.enabled&&<a className={camera.calibrated?'recalibrate':undefined} href={`/aida/?gaia=1&source_id=${encodeURIComponent(camera.id)}`} target="_blank" rel="noreferrer noopener" title={camera.calibrated?'Fit a new lens model from the latest frame; the current calibration is kept':'Fit a lens model from the latest frame'}>{camera.calibrated?'Recalibrate latest in AIDA ↗':'Calibrate latest in AIDA ↗'}</a>}</div></article>)}</div></div>}
        {view === 'status' && <div className="route-panel"><div className="route-heading"><Activity/><div><span className="eyebrow">INGESTION PIPELINE</span><h2>Data flow status</h2></div></div><div className="pipeline">{['Acquire','Calibrate','Mask','Project','Tessellate'].map((x,i)=><div key={x}><span className={i===0?'running':''}>{i===0?'↻':'·'}</span><strong>{x}</strong><small>{i===0?'polling sources':'framework ready'}</small></div>)}</div><div className="source-list">{cameras.map(camera=><article key={camera.id}><span className={`source-state ${camera.state}`}/><div><strong>{camera.name}</strong><small>{camera.producer}</small></div><div><b>{camera.timestamp_mode==='archive'?'Archive time':'Download time'}</b><small>{camera.message||`${camera.images_24h} images in 24 h`}</small></div></article>)}</div></div>}
        {view === 'calibrate' && <div className="route-panel narrow"><div className="route-heading"><Aperture/><div><span className="eyebrow">ADD COVERAGE</span><h2>Contribute an imager</h2></div></div><p>Every image retains the producer’s name, institution, copyright and requested acknowledgement. Fixed cameras normally reuse a seasonal calibration; phone images are calibrated individually.</p><div className="choice-grid"><article><span>01</span><h3>Calibrate the image</h3><p>Use AIDA/WISC to match stars and fit the lens. GAIA accepts its native calibration HDF5.</p><a href="/aida/?gaia=1" target="_blank">Open AIDA calibrator ↗</a></article><article><span>02</span><h3>Register a source</h3><p>Add a crawler JSON entry with cadence, timestamp policy, station location, producer and copyright.</p><a href="https://github.com/jvierine/gaia#adding-an-image-source" target="_blank">Read the source guide ↗</a></article></div></div>}
        {view === 'about' && <div className="route-panel narrow"><div className="route-heading"><CircleHelp/><div><span className="eyebrow">OPEN SCIENCE INFRASTRUCTURE</span><h2>About the data center &amp; credits</h2></div></div><p>GAIA maps calibrated auroral images onto a 100 km emission shell.</p><h3>Authors</h3><p>Juha Vierinen and Björn Gustavsson</p><p>GAIA is an independent image aggregation and geographic projection service.</p><p>Image copyrights remain with their producers. GAIA does not transfer ownership or replace the producer’s terms.</p><h3>Suggest an improvement</h3><p>Report a bad image, missing source, calibration issue or viewer idea. Suggestions are stored for manual review; they do not change GAIA automatically.</p><button className="suggest-link" onClick={()=>{setSuggesting(true);setSent(false)}}><Lightbulb size={14}/> Suggest an improvement</button><Credits/></div>}
        <div className="source-strip">{view==='globe'&&<div className="strip-zoom" role="group" aria-label="Map controls"><button type="button" aria-label="Zoom in" title="Zoom in" onClick={()=>zoomRef.current?.('in')}>+</button><button type="button" aria-label="Zoom out" title="Zoom out" onClick={()=>zoomRef.current?.('out')}>&minus;</button><button type="button" aria-label="Reset globe" title="Reset view" onClick={()=>zoomRef.current?.('reset')}>◎</button></div>}<div><Database size={15} /><span><strong>{playing?'Smoothed playback':timeMinutes===1440?'Delayed live frames':'Archived frames'}</strong> {framesLoading?'Loading…':'Selected at or before timeline time; gaps over 10 min omitted'}</span></div><div className="legend"><span><span className="dot live" /> clear</span><span><span className="dot delay" /> delayed</span><span><span className="dot idle" /> unavailable</span></div></div>
      </section>
    </section>
    {suggesting&&<div className="modal-backdrop" role="presentation"><form className="suggestion-box" onSubmit={submitSuggestion}><button type="button" className="close" onClick={()=>setSuggesting(false)} aria-label="Close"><X/></button>{sent?<div className="sent"><Send/><h2>Suggestion received</h2><p>Thank you. The GAIA team will review it before deciding what to send to Codex.</p></div>:<><span className="eyebrow">HUMAN-REVIEWED INPUT</span><h2>Suggest an improvement</h2><p>Report a bad image, missing source, calibration issue or viewer idea. Suggestions are stored for manual review; they do not change GAIA automatically.</p><label>Your suggestion<textarea name="suggestion" minLength={4} maxLength={8000} required placeholder="What should we improve?"/></label><div className="form-row"><label>Name <span>optional</span><input name="name"/></label><label>Contact <span>optional</span><input name="contact"/></label></div><button className="send-button" type="submit"><Send size={15}/> Send suggestion</button></>}</form></div>}
    {photometryCamera&&<StarPhotometry camera={photometryCamera} onClose={()=>setPhotometryCamera(null)}/>}
    {calibratingCamera&&<CalibrationPicker camera={calibratingCamera} onClose={()=>setCalibratingCamera(null)}/>}
    {editingCamera&&<LocationEditor camera={editingCamera} onClose={()=>setEditingCamera(null)} onSaved={saved=>setCameras(rows=>rows.map(row=>row.id===saved.id?saved:row))}/>} 
    {maskingCamera&&<MaskEditor camera={maskingCamera} onClose={()=>setMaskingCamera(null)}/>} 
    {browsingCamera&&<FrameBrowser camera={browsingCamera} onClose={()=>setBrowsingCamera(null)}/>}
  </main>;
}
