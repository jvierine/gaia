'use client';

import PlaybackToolbar from '../src/PlaybackToolbar';
import {ServiceDescription,ProviderContacts} from '../src/ProviderContacts';
import {liveCutoff,LIVE_DELAY_MINUTES} from '../src/live-time';
import { Activity, Aperture, CircleHelp, Database, Lightbulb, Satellite, Send, X } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import GaiaGlobeView from '../src/GaiaGlobeView';
import {clipToFrame,frameBoundary,voronoiEdges} from '../src/voronoi';
// `extent` is imported under another name: the panel already has a local one
// for the scatter, and a shadowed import would silently call the wrong function.
import {densityColour,extent as binExtent,histogram1d,histogram2d,tickLabel,ticks} from '../src/histogram';
import {clampLevels,fractionToLevel,levelToFraction,suggest,transfer,FULL_RANGE,
        type Levels} from '../src/levels';
import {addWindow,edgeNear,fractionToTime,keogramImage,makeWindow,removeWindowAt,resizeWindow,
        scatterPoints,selectedSeconds,type KeogramPair,type Window} from '../src/keogram';

type ViewName = 'globe' | 'cameras' | 'status' | 'calibrate' | 'about';
type Camera = {id:string;name:string;producer:string;state:string;timestamp_mode:string;latitude_deg:number|null;longitude_deg:number|null;calibrated:boolean;processing?:{state:string;total?:number;done?:number;ready?:number;error?:string|null};enabled:boolean;quality_exponent:number;images_24h:number;message?:string|null};
type CameraFrame = {id:string;observation_utc:string;width:number|null;height:number|null};
type Calibration = {id:string;created_utc:string;valid_from_utc:string|null;valid_to_utc:string|null;method:string;residual_px:number|null;star_count:number|null;selected:boolean};
// Playback step interval is PLAYBACK_STEP_MS divided by the chosen speed.
const PLAYBACK_STEP_MS = 150;
const SPEEDS = [0.25, 0.5, 1, 2, 4, 8, 16, 32] as const;
type StarSummary = {star_key:string;vt_mag:number;ra_hours_j2000:number;dec_deg_j2000:number;image_x:number|null;image_y:number|null;elevation_deg:number|null;frames:number;found:number;median_flux:number|null;clear_flux:number|null;median_background:number|null;variation:number|null};
type StarSample = {at:string;flux:number|null;background:number|null;amplitude:number|null;sigma_major:number|null;sigma_minor:number|null;angle_deg:number|null;centroid_offset_px:number|null;elevation_deg:number};
const STAR_CHANNELS = ['mean','r','g','b'] as const;
/// The average reads warm, the colour channels read as themselves.
const CHANNEL_COLOUR:Record<string,string> = {mean:'#f5a524',r:'#ff5f56',g:'#3ddc84',b:'#5aa9ff'};
type KeogramOption={partner_id:string;partner_name:string;separation_km:number;colocated:boolean};
type FrameStar={star_key:string;vt_mag:number;x:number|null;y:number|null;elevation_deg:number|null;flux:number|null;flux_snr:number|null;detected:boolean;saturated:boolean;certain:boolean;usable:boolean;washed_out:boolean;optical_depth:number|null;best_flux:number|null;relative:number|null};
type Distribution={star_key:string;samples:number;days:number;channels:Record<string,{flux:number[];amplitude:number[];background:number[]}>};
type StarNight={night:number;from:string;to:string;frames:number;looked_for:number;detections:number};
type StarFrame={image_id:string|null;observation_utc?:string;width?:number|null;height?:number|null;field?:[number,number][]|null;stars:FrameStar[]};
/// A star at its own best through the window reads bright, one that has faded
/// reads dark and red. The quantity is the star's intensity as a fraction of
/// its own maximum, so a faint star and a bright one are on the same scale.
/// The colour follows the square root of that fraction: a star's best is the
/// single clearest moment at the top of its arc, so most samples sit well below
/// it and a linear ramp would put nearly every star in the dark end.
const relativeColour=(v:number|null)=>{
  if(v==null)return '#3b4a57';
  const t=Math.sqrt(Math.min(1,Math.max(0,v)));
  return `hsl(${Math.round(48*t)} 92% ${Math.round(26+46*t)}%)`};
/// Magnitudes a star has fallen below its own best, the photometric way to say
/// the same thing: cloud optical depth is linear in magnitudes.
const magnitudesDown=(v:number|null)=>v==null||!(v>0)?null:-2.5*Math.log10(v);
/// Steady stars read cool, strongly varying ones warm.
const variationColour = (v:number|null) => v==null?'#5d7080':`hsl(${Math.round(190-190*Math.min(1,Math.max(0,v)))} 78% 55%)`;
type SortKey = 'name' | 'latitude_deg' | 'longitude_deg' | 'calibrated';
// Manual per-camera quality weight, offered as powers of two from 1 to 1/256.
const qualitySteps = [0,-1,-2,-3,-4,-5,-6,-7,-8];
const sortOptions:[SortKey,string][] = [['name','Name'],['latitude_deg','Latitude'],['longitude_deg','Longitude'],['calibrated','Calibration']];


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
  const[date,setDate]=useState('');const[loading,setLoading]=useState(true);
  const[frames,setFrames]=useState<CameraFrame[]>([]);const[index,setIndex]=useState(0);const[error,setError]=useState('');
  useEffect(()=>{const controller=new AbortController();setLoading(true);setError('');setFrames([]);setIndex(0);void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/frames${date?'?date='+encodeURIComponent(date):''}`,{cache:'no-store',signal:controller.signal}).then(async response=>{if(!response.ok)throw new Error('Could not load frame history.');return await response.json() as CameraFrame[]}).then(rows=>{if(controller.signal.aborted)return;setFrames(rows);setIndex(Math.max(0,rows.length-1))}).catch(reason=>{if(!controller.signal.aborted)setError(String(reason.message||reason))}).finally(()=>{if(!controller.signal.aborted)setLoading(false)});return()=>controller.abort()},[camera.id,date]);
  const frame=frames[index];
  return <div className="modal-backdrop"><div className="frame-browser"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CAMERA FRAME HISTORY</span><h2>{camera.name}</h2><p>Browse a UTC date or the last 24 hours. Open the exact selected frame in AIDA for calibration.</p><div className="history-date-controls"><label>Day (UTC) <input aria-label="Camera history date (UTC)" type="date" value={date} onChange={e=>setDate(e.target.value)}/></label><button type="button" onClick={()=>setDate('')} disabled={!date}>Last 24 hours</button></div>{loading?<div role="status"><p>Loading {date||'last 24 hours'}…</p><progress aria-label="Camera frame history loading"/></div>:error?<p role="alert">{error}</p>:!frame?<div className="history-empty">No archived frames {date?`on ${date} UTC`:'in the last 24 hours'}.</div>:<><div className="history-image"><img src={`/gaia/api/images/${encodeURIComponent(frame.id)}/original`} alt={`${camera.name} at ${frame.observation_utc}`}/></div><div className="history-meta"><time>{new Date(frame.observation_utc).toISOString().replace('T',' ').replace('.000Z',' UTC')}</time><span>{index+1} / {frames.length}{frame.width&&frame.height?` · ${frame.width} × ${frame.height}`:''}</span></div><input className="history-slider" aria-label="Historical camera frame" type="range" min="0" max={Math.max(0,frames.length-1)} step="1" value={index} onChange={event=>setIndex(Number(event.target.value))}/><div className="history-actions"><button onClick={()=>setIndex(value=>Math.max(0,value-1))} disabled={index===0}>← Previous</button><button onClick={()=>setIndex(value=>Math.min(frames.length-1,value+1))} disabled={index===frames.length-1}>Next →</button><a className="send-button" href={`/aida/?gaia=1&source_id=${encodeURIComponent(camera.id)}&image_id=${encodeURIComponent(frame.id)}`} target="_blank" rel="noopener noreferrer">Calibrate selected frame in AIDA ↗</a></div></>}</div></div>;
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
/// One keogram, drawn as pixels rather than as marks. A canvas is the honest
/// container for this: the data is already an image, one column per frame and
/// one row per sample along the cut, and turning it into thousands of SVG rects
/// would cost far more and show exactly the same thing.
function KeogramCanvas({rows,side,samples,label}:
  {rows:KeogramPair['rows'];side:'a'|'b';samples:number;label:string}){
  const ref=useRef<HTMLCanvasElement|null>(null);
  useEffect(()=>{
    const canvas=ref.current;if(!canvas)return;
    const {width,height,data}=keogramImage(rows,side,samples);
    canvas.width=width;canvas.height=height;
    const context=canvas.getContext('2d');if(!context)return;
    context.clearRect(0,0,width,height);
    // The clamped array is typed over ArrayBufferLike, which ImageData's own
    // signature narrows to ArrayBuffer; the buffer here is always the plain one.
    context.putImageData(new ImageData(data as ImageData['data'],width,height),0,0);
  },[rows,side,samples]);
  // The element is stretched by CSS; the backing store stays one pixel per
  // sample, so no interpolation invents structure that was never measured.
  return <canvas ref={ref} className="keogram-canvas" role="img" aria-label={label}/>;
}

/// The two cameras' intensities against each other, one point per sample pair.
/// Drawn with additive alpha so the density of the cloud is visible: a fit will
/// be driven by where the points pile up, and the operator should see that pile
/// rather than a uniform smear.
function KeogramScatter({sets,size}:
  {sets:ReturnType<typeof scatterPoints>;size:number}){
  const ref=useRef<HTMLCanvasElement|null>(null);
  useEffect(()=>{
    const canvas=ref.current;if(!canvas)return;
    canvas.width=size;canvas.height=size;
    const context=canvas.getContext('2d');if(!context)return;
    context.clearRect(0,0,size,size);
    // Fixed 0-255 axes: these are 8-bit intensities, and a rescaled axis would
    // hide both saturation and a dark, unusable window.
    context.strokeStyle='rgba(255,255,255,0.12)';context.lineWidth=1;
    for(let i=0;i<=4;i++){const at=Math.round(i/4*(size-1))+0.5;
      context.beginPath();context.moveTo(at,0);context.lineTo(at,size);context.stroke();
      context.beginPath();context.moveTo(0,at);context.lineTo(size,at);context.stroke();}
    // The line of equal response, for reference: points above it mean the
    // vertical camera reads brighter than the horizontal one.
    context.strokeStyle='rgba(255,255,255,0.28)';
    context.beginPath();context.moveTo(0,size);context.lineTo(size,0);context.stroke();
    const colour={r:'rgba(255,90,80,0.35)',g:'rgba(80,220,120,0.35)',b:'rgba(90,150,255,0.35)'};
    for(const set of sets){
      context.fillStyle=colour[set.channel];
      for(const [x,y] of set.points){
        context.fillRect(x/255*(size-1),size-1-y/255*(size-1),1.6,1.6);
      }
    }
  },[sets,size]);
  return <canvas ref={ref} className="keogram-scatter" role="img"
    aria-label="Intensities of the two cameras against each other, by colour channel"/>;
}

function StarPhotometry({camera,onClose}:{camera:Camera;onClose:()=>void}){
  const [channel,setChannel]=useState<string>('mean');
  const [hours,setHours]=useState(24);
  // Observing nights this camera has, and the run of them being shown. A night
  // is local solar noon to noon, so one period of darkness is one entry and a
  // selection never cuts an evening in half.
  const [nights,setNights]=useState<StarNight[]>([]);
  const [range,setRange]=useState<{from:number;to:number}|null>(null);
  const [stars,setStars]=useState<StarSummary[]>([]);
  const [selected,setSelected]=useState<string[]>([]);
  const [series,setSeries]=useState<Record<string,StarSample[]>>({});
  const [loading,setLoading]=useState(true),[error,setError]=useState('');
  // Scrubbing the series: the instant the user has slid to, and the nearest
  // measured frame to it.
  const [cursor,setCursor]=useState<number|null>(null);
  const [frame,setFrame]=useState<StarFrame|null>(null);
  const [frameBusy,setFrameBusy]=useState(false);
  // A cursor the step buttons have already fetched a frame for, so moving to it
  // does not immediately fetch the same frame again.
  const satisfied=useRef<number|null>(null);
  // Magnification of the frame view, with the centre it is magnified about in
  // image pixels. Stars are a pixel or two across, so this is the difference
  // between seeing a fit and guessing at it.
  // Which of the two left-hand views is showing: the series through time, or
  // the distributions those series are drawn from.
  const [leftTab,setLeftTab]=useState<'series'|'histograms'|'keograms'>('series');
  // The keogram pair: which partner camera, the extracted pair, and the time
  // intervals the operator has brushed out of it as fit material.
  const [pairOptions,setPairOptions]=useState<KeogramOption[]>([]);
  const [partner,setPartner]=useState<string>('');
  const [keogram,setKeogram]=useState<KeogramPair|null>(null);
  const [keoBusy,setKeoBusy]=useState(false);
  const [keoError,setKeoError]=useState('');
  // Selected windows carry the rows they were made from, so a selection made
  // on one night survives the panel moving to another and still feeds the
  // scatter. They are cleared when the pair changes, since a window means
  // nothing against a different pair of cameras.
  const [windows,setWindows]=useState<Window[]>([]);
  // An in-progress brush, in panel fractions, so it can be drawn before it is
  // committed on pointer release.
  const [brush,setBrush]=useState<{from:number;to:number}|null>(null);
  // An edge being dragged: which window, which end.
  const [grab,setGrab]=useState<{index:number;edge:'from'|'to'}|null>(null);
  // The histograms are built from the whole archive for one star, not from the
  // window on screen. A star at high latitude barely changes elevation, so its
  // air mass hardly varies night to night and its clear-sky level is far better
  // determined over months than over hours; that level is what a fading is
  // measured against.
  const [histStar,setHistStar]=useState<string|null>(null);
  const [distribution,setDistribution]=useState<Distribution|null>(null);
  const [distBusy,setDistBusy]=useState(false);
  const [zoom,setZoom]=useState(1);
  const [centre,setCentre]=useState<[number,number]|null>(null);
  const pan=useRef<{x:number;y:number;cx:number;cy:number}|null>(null);
  // The window every request in this panel asks for: the selected run of
  // nights when there is one, the trailing hours otherwise.
  const chosen=range?nights.filter(n=>n.night>=range.from&&n.night<=range.to):[];
  const span=chosen.length
    ?`from=${encodeURIComponent(chosen[chosen.length-1].from)}&to=${encodeURIComponent(chosen[0].to)}`
    :`hours=${hours}`;
  const pickNight=(night:number,extend:boolean)=>setRange(prev=>
    extend&&prev?{from:Math.min(prev.from,night),to:Math.max(prev.to,night)}:{from:night,to:night});
  useEffect(()=>{
    const controller=new AbortController();setLoading(true);setError('');
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars?channel=${channel}&${span}`,{cache:'no-store',signal:controller.signal})
      .then(async r=>{if(!r.ok)throw new Error('Could not load star photometry.');return await r.json() as {stars:StarSummary[]}})
      .then(body=>{const rows=body.stars.filter(x=>x.image_x!=null&&x.image_y!=null);setStars(rows);
        setSelected(prev=>{const keep=prev.filter(k=>rows.some(r=>r.star_key===k));
          if(keep.length)return keep;
          return [...rows].sort((a,b)=>(b.found)-(a.found)).slice(0,3).map(r=>r.star_key)});})
      .catch(e=>{if(!controller.signal.aborted)setError(String(e.message||e))})
      .finally(()=>{if(!controller.signal.aborted)setLoading(false)});
    return()=>controller.abort();
  },[camera.id,channel,span]);
  useEffect(()=>{
    const controller=new AbortController();
    for(const key of selected){
      if(series[`${span}:${channel}:${key}`])continue;
      void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars/series?star=${encodeURIComponent(key)}&channel=${channel}&${span}`,{cache:'no-store',signal:controller.signal})
        .then(async r=>r.ok?await r.json() as {samples:StarSample[]}:{samples:[]})
        .then(body=>setSeries(prev=>({...prev,[`${span}:${channel}:${key}`]:body.samples})))
        .catch(()=>{});
    }
    return()=>controller.abort();
  },[selected,channel,span,camera.id,series]);
  useEffect(()=>{
    const controller=new AbortController();
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars/nights?channel=${channel}&days=60`,{cache:'no-store',signal:controller.signal})
      .then(async r=>r.ok?await r.json() as {nights:StarNight[]}:{nights:[]})
      .then(body=>setNights(body.nights))
      .catch(()=>{});
    return()=>controller.abort();
  },[camera.id,channel]);
  useEffect(()=>{setRange(null)},[camera.id]);
  useEffect(()=>{
    setHistStar(prev=>prev&&selected.includes(prev)?prev:(selected[0]??null));
  },[selected]);
  useEffect(()=>{
    if(leftTab!=='histograms'||!histStar){setDistribution(null);return}
    const controller=new AbortController();setDistBusy(true);
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars/distribution?star=${encodeURIComponent(histStar)}`,{cache:'no-store',signal:controller.signal})
      .then(async r=>r.ok?await r.json() as Distribution:null)
      .then(body=>{if(!controller.signal.aborted)setDistribution(body)})
      .catch(()=>{})
      .finally(()=>{if(!controller.signal.aborted)setDistBusy(false)});
    return()=>controller.abort();
  },[leftTab,histStar,camera.id]);
  useEffect(()=>{
    if(leftTab!=='keograms')return;
    const controller=new AbortController();
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/keogram-pairs`,{cache:'no-store',signal:controller.signal})
      .then(async r=>r.ok?await r.json() as {pairs:KeogramOption[]}:{pairs:[]})
      .then(body=>{if(controller.signal.aborted)return;
        setPairOptions(body.pairs);
        setPartner(prev=>body.pairs.some(p=>p.partner_id===prev)?prev:(body.pairs[0]?.partner_id??''));})
      .catch(()=>{});
    return()=>controller.abort();
  },[leftTab,camera.id]);
  useEffect(()=>{setPartner('');setKeogram(null);setWindows([])},[camera.id]);
  // A window is a statement about one pair; changing the pair retires them all.
  useEffect(()=>{setWindows(prev=>prev.filter(w=>w.pair===`${camera.id}|${partner}`))},[partner,camera.id]);
  useEffect(()=>{
    if(leftTab!=='keograms'||!partner){setKeogram(null);return}
    const controller=new AbortController();setKeoBusy(true);setKeoError('');
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/keogram?partner=${encodeURIComponent(partner)}&${span}`,{cache:'no-store',signal:controller.signal})
      .then(async r=>{if(!r.ok)throw new Error(await r.text()||'Could not build the keogram pair.');
        return await r.json() as KeogramPair})
      .then(body=>{if(!controller.signal.aborted)setKeogram(body)})
      .catch(e=>{if(!controller.signal.aborted){setKeogram(null);setKeoError(String(e.message||e))}})
      .finally(()=>{if(!controller.signal.aborted)setKeoBusy(false)});
    return()=>controller.abort();
  },[leftTab,partner,camera.id,span]);
  // A new camera, channel or window invalidates the scrubbed frame.
  useEffect(()=>{setCursor(null);setFrame(null)},[camera.id,channel,span]);
  useEffect(()=>{setZoom(1);setCentre(null)},[frame?.image_id]);
  useEffect(()=>{
    if(cursor==null){setFrame(null);return}
    if(satisfied.current===cursor){satisfied.current=null;return}
    const controller=new AbortController();
    // The pointer moves far faster than the request; wait for it to settle.
    const timer=window.setTimeout(()=>{setFrameBusy(true);
      void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars/frame?at=${encodeURIComponent(new Date(cursor).toISOString())}&channel=${channel}&${span}`,{cache:'no-store',signal:controller.signal})
        .then(async r=>r.ok?await r.json() as StarFrame:null)
        .then(body=>{if(!controller.signal.aborted)setFrame(body)})
        .catch(()=>{})
        .finally(()=>{if(!controller.signal.aborted)setFrameBusy(false)})},120);
    return()=>{controller.abort();window.clearTimeout(timer)};
  },[cursor,camera.id,channel,span]);
  // Stepping is by measured frame, so a gap in the archive is one press rather
  // than a hunt with the slider.
  const stepFrame=async(step:'next'|'prev')=>{
    if(cursor==null)return;
    setFrameBusy(true);
    try{
      const body=await fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/stars/frame?at=${encodeURIComponent(new Date(cursor).toISOString())}&step=${step}&channel=${channel}&${span}`,{cache:'no-store'})
        .then(async r=>r.ok?await r.json() as StarFrame:null);
      if(!body?.image_id||!body.observation_utc)return;
      const at=Date.parse(body.observation_utc);
      satisfied.current=at;setFrame(body);setCursor(at);
    }catch{}finally{setFrameBusy(false)}
  };
  const toggle=(key:string)=>setSelected(prev=>prev.includes(key)?prev.filter(k=>k!==key):[...prev,key]);
  const palette=['#56f0c5','#f3b647','#72ccef','#ff8fa3','#baff78','#c9a0ff'];
  const picked=selected.map((k,i)=>({key:k,colour:palette[i%palette.length],samples:series[`${span}:${channel}:${k}`]||[]}));
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
  // Pointer position along a plot, as a time. The SVG is stretched to its box,
  // so the fraction has to come from the rendered width, not the viewBox.
  const timeAt=(event:React.PointerEvent<SVGSVGElement>)=>{
    const box=event.currentTarget.getBoundingClientRect();
    if(box.width<=0||!(t1>t0))return null;
    const fraction=Math.min(1,Math.max(0,(event.clientX-box.left)/box.width));
    return Math.round(t0+fraction*(t1-t0));};
  const plot=(label:string,get:(x:StarSample)=>number|null,unit:string,scrub=false)=>{
    const [lo,hi]=band(get);const w=560,h=118;
    const cursorX=scrub&&cursor!=null&&t1>t0?(cursor-t0)/(t1-t0)*w:null;
    const scrubbing=(event:React.PointerEvent<SVGSVGElement>)=>{
      if(!scrub)return;const at=timeAt(event);if(at!=null)setCursor(at)};
    return <div className="star-plot"><div className="star-plot-head"><strong>{label}</strong><small>{lo.toPrecision(3)} to {hi.toPrecision(3)} {unit}</small></div>
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" role="img" aria-label={label}
        className={scrub?'scrubbable':undefined}
        onPointerDown={event=>{if(scrub){event.currentTarget.setPointerCapture(event.pointerId);scrubbing(event)}}}
        onPointerMove={event=>{if(scrub&&event.buttons===1)scrubbing(event)}}>
        <rect x="0" y="0" width={w} height={h} className="star-plot-bg"/>
        {[0.25,0.5,0.75].map(f=><line key={f} x1="0" x2={w} y1={h*f} y2={h*f} className="star-grid"/>)}
        {picked.map(p=><path key={p.key} d={path(p.samples,get,lo,hi,w,h)} fill="none" stroke={p.colour} strokeWidth="1.4"/>)}
        {cursorX!=null&&<line x1={cursorX} x2={cursorX} y1="0" y2={h} className="star-cursor"/>}
      </svg></div>;};
  // Plot geometry. These SVGs scale uniformly rather than being stretched to
  // their box, because stretched text is unreadable and these ones carry axis
  // labels.
  const PLOT={w:600,h:180,left:52,right:8,top:8,bottom:26};
  const axisLines=(lo:number,hi:number,peak:number,area:{x:number;y:number;w:number;h:number})=>{
    const xs=ticks(lo,hi,5),ys=ticks(0,peak,4);
    return <>
      {xs.map(v=>{const x=area.x+(v-lo)/(hi-lo)*area.w;
        return <g key={`x${v}`}>
          <line x1={x} x2={x} y1={area.y+area.h} y2={area.y+area.h+4} className="star-axis"/>
          <text x={x} y={area.y+area.h+15} className="star-tick" textAnchor="middle">{tickLabel(v)}</text>
        </g>})}
      {ys.map(v=>{const y=area.y+area.h-(peak>0?v/peak:0)*area.h;
        return <g key={`y${v}`}>
          <line x1={area.x-4} x2={area.x} y1={y} y2={y} className="star-axis"/>
          <line x1={area.x} x2={area.x+area.w} y1={y} y2={y} className="star-grid"/>
          <text x={area.x-7} y={y+3.5} className="star-tick" textAnchor="end">{tickLabel(v)}</text>
        </g>})}
      <line x1={area.x} x2={area.x} y1={area.y} y2={area.y+area.h} className="star-axis"/>
      <line x1={area.x} x2={area.x+area.w} y1={area.y+area.h} y2={area.y+area.h} className="star-axis"/>
    </>;};
  const histogramPlot=(label:string,key:'flux'|'amplitude'|'background',unit:string)=>{
    const channels=STAR_CHANNELS.map(c=>({channel:c,values:distribution?.channels?.[c]?.[key]??[]}));
    const all=channels.flatMap(c=>c.values);
    // One span for all four, so the channels can be compared by eye.
    const [lo,hi]=binExtent(all);
    const bins=44;
    const area={x:PLOT.left,y:PLOT.top,w:PLOT.w-PLOT.left-PLOT.right,h:PLOT.h-PLOT.top-PLOT.bottom};
    const binned=channels.map(c=>({...c,bins:histogram1d(c.values,bins,[lo,hi])}));
    const peak=binned.reduce((m,c)=>Math.max(m,c.bins.peak),0)||1;
    const outline=(counts:number[])=>{
      let d=`M${area.x} ${area.y+area.h}`;
      counts.forEach((n,i)=>{const x0=area.x+i/bins*area.w,x1=area.x+(i+1)/bins*area.w,
        y=area.y+area.h-n/peak*area.h;
        d+=`L${x0.toFixed(1)} ${y.toFixed(1)}L${x1.toFixed(1)} ${y.toFixed(1)}`});
      return `${d}L${area.x+area.w} ${area.y+area.h}`;};
    return <div className="star-plot"><div className="star-plot-head"><strong>{label}</strong>
      <small>{all.length} measurements {unit}</small></div>
      <svg className="star-hist" viewBox={`0 0 ${PLOT.w} ${PLOT.h}`} role="img" aria-label={`${label} distribution`}>
        <rect x={area.x} y={area.y} width={area.w} height={area.h} className="star-plot-bg"/>
        {axisLines(lo,hi,peak,area)}
        {binned.map(c=>c.bins.total>0&&<path key={c.channel} d={outline(c.bins.counts)} fill="none"
          stroke={CHANNEL_COLOUR[c.channel]} strokeWidth={c.channel==='mean'?1.8:1.1}
          opacity={c.channel==='mean'?1:.85}/>)}
      </svg></div>;};
  // Pointer position within a keogram panel, as a fraction of its width. Both
  // panels share the time axis, so a brush started on one applies to both.
  const brushFraction=(event:React.PointerEvent<HTMLDivElement>)=>{
    const box=event.currentTarget.getBoundingClientRect();
    return Math.min(1,Math.max(0,(event.clientX-box.left)/Math.max(1,box.width)));};
  const keogramPanel=()=>{
    const rows=keogram?.rows??[];
    const pairKey=`${camera.id}|${partner}`;
    const scatter=scatterPoints(windows);
    const counted=scatter[0]?.points.length??0;
    const chosen=selectedSeconds(windows)/60;
    // Instant under the pointer, from its position across the panel.
    const timeAt=(event:React.PointerEvent<HTMLDivElement>)=>{
      const box=event.currentTarget.getBoundingClientRect();
      return fractionToTime(rows,Math.min(1,Math.max(0,(event.clientX-box.left)/Math.max(1,box.width))));};
    // How close counts as grabbing an end: six pixels of the panel, expressed
    // in time, so the handle is the same size whatever the window spans.
    const grabTolerance=()=>{
      if(rows.length<2)return 0;
      const span=Date.parse(rows[rows.length-1].at)-Date.parse(rows[0].at);
      return span*0.008;};
    const band=(w:{from:number;to:number})=>{
      const first=rows.length?Date.parse(rows[0].at):0;
      const last=rows.length>1?Date.parse(rows[rows.length-1].at):first+1;
      const at=(t:number)=>Math.min(100,Math.max(0,(t-first)/Math.max(1,last-first)*100));
      return {left:`${at(w.from)}%`,width:`${Math.max(0.4,at(w.to)-at(w.from))}%`};};
    // Windows from other nights are kept but cannot be drawn on this keogram.
    const onScreen=(w:Window)=>rows.length>1
      &&w.to>=Date.parse(rows[0].at)&&w.from<=Date.parse(rows[rows.length-1].at);
    const elsewhere=windows.filter(w=>!onScreen(w));
    const strip=(side:'a'|'b',name:string,coverage:number)=>
      <div className="keogram-strip">
        <div className="star-plot-head"><strong>{name}</strong>
          <small>{(coverage*100).toFixed(0)}% of the cut in view</small></div>
        <div className="keogram-frame"
          onPointerDown={event=>{event.currentTarget.setPointerCapture(event.pointerId);
            const at=timeAt(event);
            const edge=edgeNear(windows,at,grabTolerance());
            if(edge)setGrab(edge); else setBrush({from:at,to:at})}}
          onPointerMove={event=>{
            const at=timeAt(event);
            if(grab)setWindows(prev=>resizeWindow(prev,grab.index,grab.edge,at,rows));
            else if(brush)setBrush({from:brush.from,to:at});
            else{
              // Show the grab cursor when an end is within reach.
              const near=edgeNear(windows,at,grabTolerance());
              event.currentTarget.style.cursor=near?'ew-resize':'crosshair';}}}
          onPointerUp={()=>{
            if(grab){setGrab(null);return}
            if(!brush)return;
            const width=Math.abs(brush.to-brush.from);
            // A click rather than a drag: take back whatever is under it.
            if(width<grabTolerance())setWindows(prev=>removeWindowAt(prev,brush.from));
            else setWindows(prev=>addWindow(prev,makeWindow(pairKey,brush.from,brush.to,rows)));
            setBrush(null);}}>
          <KeogramCanvas rows={rows} side={side} samples={keogram?.samples??1}
            label={`Keogram for ${name}`}/>
          {windows.filter(onScreen).map((w,n)=><span key={n} className="keogram-band" style={band(w)}/>)}
          {brush&&<span className="keogram-band keogram-band-live" style={{
            left:`${Math.min(...[brush.from,brush.to].map(t=>Number(band({from:t,to:t}).left.replace('%',''))))}%`,
            width:`${Math.abs(Number(band({from:brush.to,to:brush.to}).left.replace('%',''))-Number(band({from:brush.from,to:brush.from}).left.replace('%','')))}%`}}/>}
        </div>
      </div>;
    return <>
      <div className="keogram-pick">
        <span className="eyebrow">PAIR</span>
        <select value={partner} onChange={event=>setPartner(event.target.value)}
          aria-label="Camera to pair with">
          {pairOptions.length===0&&<option value="">no overlapping camera</option>}
          {pairOptions.map(p=><option key={p.partner_id} value={p.partner_id}>
            {p.partner_name} {p.colocated?'\u00b7 co-located':`\u00b7 ${p.separation_km.toFixed(0)} km`}
          </option>)}
        </select>
        <small role="status">{keoBusy?'sampling the cut in both cameras\u2026'
          :keoError?keoError
          :keogram?`${keogram.rows.length} paired frames of ${keogram.frames_available}, about every ${Math.max(1,Math.round(keogram.cadence_seconds/60))} min, ${keogram.samples} samples across ${(2*keogram.half_width_km).toFixed(0)} km`
          :'pick a camera to compare against'}</small>
      </div>
      {keogram&&keogram.rows.length>0&&<>
        {strip('a',keogram.a.name,keogram.a.coverage)}
        {strip('b',keogram.b.name,keogram.b.coverage)}
        <div className="keogram-axis"><small>
          {new Date(keogram.from).toISOString().replace('T',' ').slice(0,16)} UTC</small>
          <small>drag to select, drag an end to move it, click inside one to drop it</small>
          <small>{new Date(keogram.to).toISOString().replace('T',' ').slice(0,16)} UTC</small></div>
        {windows.length>0&&<div className="keogram-windows">
          <span className="eyebrow">WINDOWS</span>
          {windows.map((w,n)=><button key={n} type="button" onClick={()=>setWindows(prev=>prev.filter((_,i)=>i!==n))}
            title="Remove this window">
            {new Date(w.from).toISOString().replace('T',' ').slice(5,16)}
            {'\u2013'}{new Date(w.to).toISOString().slice(11,16)}
            {' \u00b7 '}{w.rows.length} frames {'\u00d7'}</button>)}
          {elsewhere.length>0&&<small>{elsewhere.length} from other nights, still contributing</small>}
        </div>}
        <div className="star-plot"><div className="star-plot-head">
          <strong>{keogram.a.name} against {keogram.b.name}</strong>
          <small>{counted} sample pairs{windows.length?` from ${chosen.toFixed(0)} min in ${windows.length} window${windows.length>1?'s':''}`:' \u00b7 select a window to fill this'}</small></div>
          <KeogramScatter sets={scatter} size={320}/>
          <div className="star-legend">
            <span><i style={{background:'rgb(255,90,80)'}}/>red</span>
            <span><i style={{background:'rgb(80,220,120)'}}/>green</span>
            <span><i style={{background:'rgb(90,150,255)'}}/>blue</span>
            <small>Horizontal: {keogram.a.name}. Vertical: {keogram.b.name}. The faint
              diagonal is equal response. A straight line through a channel is the
              pair\u2019s gain ratio; its slope is what the equalization solve wants,
              and it can only be read from windows with real structure in them.
              Windows keep the frames they were made from, so several nights can be
              selected and fitted together. Intensities come from {keogram.pixel_source}.</small>
          </div>
        </div>
      </>}
      {keogram&&keogram.rows.length===0&&!keoBusy&&
        <div className="history-empty">No simultaneous frames from both cameras in this window.</div>}
    </>;};
  const jointPlot=()=>{
    const set=distribution?.channels?.[channel];
    // Background along the horizontal, star peak up the vertical: the eye then
    // reads how far the peak falls as the sky brightens.
    const xs=set?.background??[],ys=set?.amplitude??[];
    const n=Math.min(xs.length,ys.length);
    const columns=Math.max(10,Math.min(56,Math.round(Math.sqrt(n)*1.6)));
    const rows=Math.max(8,Math.round(columns*0.62));
    const grid=histogram2d(xs,ys,columns,rows);
    const h=300;
    const area={x:PLOT.left,y:PLOT.top,w:PLOT.w-PLOT.left-PLOT.right,h:h-PLOT.top-PLOT.bottom};
    const cw=area.w/grid.columns,ch=area.h/grid.rows;
    const xs2=ticks(grid.xLow,grid.xHigh,5),ys2=ticks(grid.yLow,grid.yHigh,5);
    return <div className="star-plot"><div className="star-plot-head"><strong>Peak against background</strong>
      <small>{channel==='mean'?'average':channel.toUpperCase()} · {grid.total} measurements</small></div>
      <svg className="star-hist" viewBox={`0 0 ${PLOT.w} ${h}`} role="img"
        aria-label="Joint distribution of sky background and star peak">
        <rect x={area.x} y={area.y} width={area.w} height={area.h} className="star-plot-bg"/>
        {grid.counts.map((c,i)=>{const colour=densityColour(c/(grid.peak||1));if(!colour)return null;
          const cx=area.x+(i%grid.columns)*cw,cy=area.y+area.h-(Math.floor(i/grid.columns)+1)*ch;
          return <rect key={i} x={cx} y={cy} width={cw+0.4} height={ch+0.4} fill={colour}/>})}
        {xs2.map(v=>{const x=area.x+(v-grid.xLow)/(grid.xHigh-grid.xLow)*area.w;
          return <g key={`x${v}`}><line x1={x} x2={x} y1={area.y+area.h} y2={area.y+area.h+4} className="star-axis"/>
            <text x={x} y={area.y+area.h+15} className="star-tick" textAnchor="middle">{tickLabel(v)}</text></g>})}
        {ys2.map(v=>{const y=area.y+area.h-(v-grid.yLow)/(grid.yHigh-grid.yLow)*area.h;
          return <g key={`y${v}`}><line x1={area.x-4} x2={area.x} y1={y} y2={y} className="star-axis"/>
            <text x={area.x-7} y={y+3.5} className="star-tick" textAnchor="end">{tickLabel(v)}</text></g>})}
        <line x1={area.x} x2={area.x} y1={area.y} y2={area.y+area.h} className="star-axis"/>
        <line x1={area.x} x2={area.x+area.w} y1={area.y+area.h} y2={area.y+area.h} className="star-axis"/>
      </svg>
      <div className="star-axes"><small>horizontal: sky background</small>
        <small>vertical: peak above background</small></div>
    </div>;};
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
      {[6,24,72].map(v=><button key={v} type="button" aria-pressed={!range&&hours===v} onClick={()=>{setRange(null);setHours(v)}}>{v} h</button>)}
      <small role="status">{loading?'Loading\u2026':`${stars.length} stars, ${selected.length} selected`}</small>
    </div>
    {nights.length>0&&<div className="star-nights" role="group" aria-label="Observing nights">
      <span className="eyebrow">NIGHT</span>
      {nights.map(n=>{const on=!!range&&n.night>=range.from&&n.night<=range.to;
        // A night runs local noon to noon, so name it by the evening it starts.
        const label=new Date(n.from).toISOString().slice(5,10);
        return <button key={n.night} type="button" aria-pressed={on}
          title={`${n.frames} frames, ${n.detections} detections\nClick to show this night, shift-click to extend across consecutive nights`}
          onClick={event=>pickNight(n.night,event.shiftKey)}>{label}</button>})}
      <small role="status">{range
        ?`${chosen.length} night${chosen.length===1?'':'s'}, ${chosen.reduce((t,n)=>t+n.frames,0)} frames \u00b7 shift-click to extend`
        :'or pick a night; shift-click a second to span consecutive nights'}</small>
    </div>}
    {error&&<p role="alert">{error}</p>}
    {!loading&&!error&&stars.length===0&&<div className="history-empty">No star photometry recorded for this camera yet.</div>}
    {stars.length>0&&<div className="star-panels">
      <div className="star-series">
        <div className="star-subtabs" role="tablist" aria-label="Left panel view">
          <button type="button" role="tab" aria-selected={leftTab==='series'} onClick={()=>setLeftTab('series')}>Time series</button>
          <button type="button" role="tab" aria-selected={leftTab==='histograms'} onClick={()=>setLeftTab('histograms')}>Histograms</button>
          <button type="button" role="tab" aria-selected={leftTab==='keograms'} onClick={()=>setLeftTab('keograms')}>Keogram pairs</button>
          <small>{leftTab==='series'?'brightness and sky through the window above'
            :leftTab==='histograms'?'one star over the whole archive, all four channels'
            :'two cameras along the sky they both see, for intensity equalization'}</small>
        </div>
        {leftTab==='keograms'?keogramPanel():leftTab==='histograms'?<>
          <div className="star-histstar">
            <span className="eyebrow">STAR</span>
            {selected.map(k=>{const row=stars.find(r=>r.star_key===k);
              return <button key={k} type="button" aria-pressed={histStar===k} onClick={()=>setHistStar(k)}>
                {row?`V ${row.vt_mag.toFixed(2)}`:k.slice(0,10)}</button>})}
            <small role="status">{!histStar?'select a star in the frame'
              :distBusy?'loading the whole archive\u2026'
              :`${distribution?.samples??0} measurements over the whole archive, every night it was seen`}</small>
          </div>
          {histogramPlot('Total intensity above background','flux','counts')}
          {histogramPlot('Peak intensity above background','amplitude','counts')}
          {histogramPlot('Fitted sky background','background','counts')}
          {jointPlot()}
          <div className="star-legend">{STAR_CHANNELS.map(c=><span key={c}><i style={{background:CHANNEL_COLOUR[c]}}/>{c==='mean'?'average':c.toUpperCase()}</span>)}
            <small>One star at a time, over the whole archive rather than the window above: a star this far north barely changes elevation, so its clear-sky level is far better determined over months than over hours.</small></div>
        </>:<>
        {plot('Total intensity above background',x=>x.flux,'counts',true)}
        {times.length>0&&<div className="star-scrub">
          <input type="range" aria-label="Time along the brightness series"
            min={t0} max={t1} step={Math.max(1000,Math.round((t1-t0)/600))}
            value={cursor??t1}
            onChange={event=>setCursor(Number(event.target.value))}/>
          <small role="status">{cursor==null?'Drag the brightness plot or this slider to open a frame'
            :`${new Date(cursor).toISOString().replace('T',' ').slice(0,19)} UTC${frameBusy?' \u00b7 loading\u2026':''}`}</small>
          {cursor!=null&&<button type="button" onClick={()=>setCursor(null)}>Clear</button>}
        </div>}
        {plot('Fitted image background',x=>x.background,'counts')}
        <div className="star-legend">{picked.map(p=>{const row=stars.find(r=>r.star_key===p.key);
          return <span key={p.key}><i style={{background:p.colour}}/>{row?`V ${row.vt_mag.toFixed(2)} at ${row.elevation_deg?.toFixed(0)}\u00b0 el`:p.key}<small>{p.samples.length} pts</small></span>})}
          {picked.length===0&&<small>Select stars in the scatter to plot them.</small>}</div></>}
      </div>
      <div className="star-scatter">
        {frame&&frame.image_id?(()=>{
          // Scrubbed frame: the image itself, with every star looked for in it
          // drawn where it was found, coloured by how far it has fallen from
          // its own best in this window.
          const placed=frame.stars.filter(r=>r.x!=null&&r.y!=null);
          const W=frame.width||Math.ceil(Math.max(...placed.map(r=>r.x as number),1)*1.05);
          const H=frame.height||Math.ceil(Math.max(...placed.map(r=>r.y as number),1)*1.05);
          const identified=placed.filter(r=>r.detected);
          const found=identified.length;
          // Markers and boundaries keep a constant size on screen, so zooming
          // magnifies the picture rather than the annotation.
          const r0=Math.max(2.5,Math.min(W,H)/150)/zoom;
          const spanX=W/zoom,spanY=H/zoom;
          const [cx,cy]=centre??[W/2,H/2];
          const vbox=[Math.min(W-spanX/2,Math.max(spanX/2,cx))-spanX/2,
                      Math.min(H-spanY/2,Math.max(spanY/2,cy))-spanY/2,spanX,spanY];
          // Each star owns the sky nearer to it than to any other, and every
          // boundary takes the colour of the more faded of the two stars that
          // meet across it. That is what rings a clouded star in dark red all
          // the way round: it is the fainter one on every edge it owns, so its
          // whole perimeter carries its own colour and the cloudy patches of
          // sky read at a glance.
          // Cut the cells to the sky the lens actually sees. A fisheye's corners
          // are ground and housing, and a cell running out there claims a
          // region its star never looked at.
          const boundary=frame.field&&frame.field.length>2
            ?clipToFrame(frame.field.map(([x,y])=>({x,y})),W,H)
            :frameBoundary(W,H);
          const cells=voronoiEdges(identified.map(r=>({x:r.x as number,y:r.y as number})),boundary);
          const fainter=(a:number,b:number)=>{
            const [p,q]=[identified[a]?.relative,identified[b]?.relative];
            if(p==null)return q??null;
            if(q==null)return p;
            return Math.min(p,q);
          };
          return <>
            <div className="star-plot-head"><strong>Star positions in the frame</strong>
              <small>{found} of {frame.stars.length} found{frame.observation_utc?` \u00b7 ${new Date(frame.observation_utc).toISOString().replace('T',' ').slice(0,19)} UTC`:''}</small></div>
            <div className="star-zoom" role="group" aria-label="Frame magnification">
              <button type="button" aria-label="Zoom in" onClick={()=>setZoom(z=>Math.min(16,z*1.6))}>+</button>
              <button type="button" aria-label="Zoom out" onClick={()=>setZoom(z=>Math.max(1,z/1.6))}>&minus;</button>
              <button type="button" aria-label="Fit the whole frame" onClick={()=>{setZoom(1);setCentre(null)}}>Fit</button>
              <span className="star-step">
                <button type="button" aria-label="Previous frame" title="Step back one measured frame"
                  disabled={frameBusy} onClick={()=>void stepFrame('prev')}>‹ Prev</button>
                <button type="button" aria-label="Next frame" title="Step forward one measured frame"
                  disabled={frameBusy} onClick={()=>void stepFrame('next')}>Next ›</button>
              </span>
              <small>{zoom<1.05?'whole frame':`${zoom.toFixed(1)}\u00d7`}</small>
            </div>
            <svg className="star-frame" viewBox={vbox.join(' ')} role="img"
              aria-label="Scrubbed camera frame with identified stars"
              onWheel={event=>{event.preventDefault();
                const box=event.currentTarget.getBoundingClientRect();
                const at:[number,number]=[vbox[0]+(event.clientX-box.left)/box.width*vbox[2],
                                          vbox[1]+(event.clientY-box.top)/box.height*vbox[3]];
                // Zoom about the pointer, so the feature under it stays put.
                setZoom(z=>{const next=Math.min(16,Math.max(1,z*Math.exp(-event.deltaY*0.0015)));
                  if(next!==z)setCentre(c=>{const[cx,cy]=c??[W/2,H/2];
                    return [at[0]+(cx-at[0])*z/next,at[1]+(cy-at[1])*z/next]});
                  return next})}}
              onPointerDown={event=>{if(zoom<=1)return;event.currentTarget.setPointerCapture(event.pointerId);
                const[cx,cy]=centre??[W/2,H/2];pan.current={x:event.clientX,y:event.clientY,cx,cy}}}
              onPointerMove={event=>{const start=pan.current;if(!start)return;
                const box=event.currentTarget.getBoundingClientRect();
                setCentre([start.cx-(event.clientX-start.x)/box.width*vbox[2],
                           start.cy-(event.clientY-start.y)/box.height*vbox[3]])}}
              onPointerUp={()=>{pan.current=null}}
              onPointerCancel={()=>{pan.current=null}}
              style={{cursor:zoom>1?'grab':'default'}}>
              <image href={`/gaia/api/images/${encodeURIComponent(frame.image_id)}/original`}
                x="0" y="0" width={W} height={H} preserveAspectRatio="none"/>
              <g className="star-cells">{cells.map((e,i)=><line key={i} x1={e.x1} y1={e.y1} x2={e.x2} y2={e.y2}
                stroke={relativeColour(fainter(e.a,e.b))} strokeWidth={r0*0.7} strokeLinecap="round"/>)}</g>
              {placed.map(r=>{const on=selected.includes(r.star_key);
                const size=r0*(r.detected?1.6:1.1)*(on?1.5:1);
                const x=r.x as number,y=r.y as number;
                // A star this camera should see whenever the sky is clear, and
                // did not: that is a total fading, not a missing measurement,
                // unless the frame is washed out and the detector explains it.
                const certainMiss=r.certain&&!r.usable;
                return <g key={r.star_key} className={on?'chosen':''} onClick={()=>toggle(r.star_key)}>
                  {certainMiss
                    ?<g fill="none" strokeWidth={r0*0.5}>
                      <circle cx={x} cy={y} r={size*1.7}
                        stroke={r.washed_out?'#7e93a5':relativeColour(0)}
                        strokeDasharray={r.washed_out?`${r0*0.9} ${r0*0.7}`:undefined}/>
                      {on&&<circle cx={x} cy={y} r={size*2.4} stroke="#fff" strokeWidth={r0*0.35}/>}
                    </g>
                    :r.saturated
                    // Background plus star has reached the top of the range, so
                    // the peak is clipped and this flux is an underestimate.
                    // Crossed out rather than filled: the colour is still the
                    // fading, but the number behind it is not to be trusted.
                    ?<g stroke={relativeColour(r.relative)} strokeWidth={r0*0.55} strokeLinecap="round">
                      <line x1={x-size} y1={y-size} x2={x+size} y2={y+size}/>
                      <line x1={x-size} y1={y+size} x2={x+size} y2={y-size}/>
                      {on&&<circle cx={x} cy={y} r={size*1.5} fill="none" stroke="#fff" strokeWidth={r0*0.4}/>}
                    </g>
                    :<circle cx={x} cy={y} r={size}
                    fill={r.detected?relativeColour(r.relative):'none'}
                    fillOpacity={r.detected?0.9:0}
                    stroke={on?'#fff':r.detected?'rgba(0,0,0,.55)':'rgba(255,255,255,.45)'}
                    strokeWidth={r0*(on?0.55:0.3)}
                    strokeDasharray={r.detected?undefined:`${r0*0.8} ${r0*0.6}`}/>}
                  <title>{`V ${r.vt_mag.toFixed(2)}  elevation ${r.elevation_deg?.toFixed(1)??'?'}\u00b0
${certainMiss?(r.washed_out
  ?'MISSING, but the frame is washed out: the detector explains it, so no fading is claimed\n'
  :`TOTAL FADING: bright enough and high enough to be seen in a clear sky, and not found${r.optical_depth==null?'':`, tau at least ${r.optical_depth.toFixed(2)}`}\n`):''}${r.saturated?'SATURATED: background plus star fills the range, so this flux is an underestimate\n':''}${r.detected?`flux ${r.flux?.toPrecision(4)} of best ${r.best_flux?.toPrecision(4)}
${r.relative==null?'':(r.relative*100).toFixed(0)+'% of its own maximum, '+(magnitudesDown(r.relative)?.toFixed(2)??'?')+' mag down'}`:'not detected in this frame'}
SNR ${r.flux_snr==null?'n/a':r.flux_snr.toFixed(1)}`}</title>
                </g>})}
            </svg>
            <div className="star-ramp"><small>faded</small>
              {[0,0.04,0.16,0.36,0.64,1].map(v=><i key={v} style={{background:relativeColour(v)}}/>)}
              <small>at its best</small></div>
            <small className="star-hint">Colour is this star's intensity as a fraction of its own maximum over the window, so faint and bright stars read alike; the ramp is square-root spaced because a star's best is its single clearest moment near the top of its arc. Dashed rings were looked for and not found; crosses saturated, their peak clipped by a background that bright aurora has lifted, so their intensity reads low. A dark red circle is a star brighter than magnitude 3.5 and above 20 degrees that this camera should have seen and did not: a total fading, counted as cloud. The same circle in grey means the frame's sky is above 240 counts, where a full detector explains the miss and no fading is claimed. The web divides the camera's field into the region nearest each identified star, every boundary taking the colour of the more faded star across it, so a clouded star is ringed in dark red all the way round. Click a star to plot it.</small>
          </>})():<>
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
        <small className="star-hint">Click a star to add or remove it from the plots. Marker size follows catalogue magnitude. Drag the brightness plot to open the frame behind it.</small>
        </>}
      </div>
    </div>}
  </div></div>;
}

const stamp=(value:string|null)=>value?new Date(value).toISOString().replace('T',' ').slice(0,16)+' UTC':null;

/// Compare the calibrations held for one camera and choose which one maps it.
type DriftStar={star_key:string;vt_mag:number;azimuth_deg:number;elevation_deg:number;
  predicted_x:number;predicted_y:number;centroid_x:number|null;centroid_y:number|null;offset_px:number|null};
type Drift={night:number;current_night?:boolean;calibration_id:string|null;calibration_star_count:number|null;
  calibration_residual_px:number|null;
  frame:{image_id:string;observation_utc:string;width:number|null;height:number|null;found:number;
    rms_offset_px:number|null;sky_level?:number|null;brightest_level?:number|null}|null;
  stars?:DriftStar[];refit_available?:boolean;refit_reason?:string;note?:string};

/// Where the sky says the stars are against where they were found.
///
/// A lens drifts, and the symptom is a standing offset between the two. Drawing
/// them on the frame itself is the only presentation that shows *which* way it
/// drifted -- a uniform shift, a rotation and a change of scale look nothing
/// alike, and a single RMS number hides all three.
function CalibrationDrift({camera}:{camera:Camera}){
  const [drift,setDrift]=useState<Drift|null>(null);
  const [busy,setBusy]=useState(true),[refitting,setRefitting]=useState(false);
  const [error,setError]=useState(''),[outcome,setOutcome]=useState('');
  // Magnification of the frame, about a centre in image pixels. A drift of a
  // pixel or two is invisible at whole-frame scale, which is the scale at
  // which it matters.
  const [zoom,setZoom]=useState(1);
  const [centre,setCentre]=useState<[number,number]|null>(null);
  const pan=useRef<{x:number;y:number;cx:number;cy:number}|null>(null);
  // Display levels. A star frame is mostly dark, so the default stretch hides
  // the very stars the operator is here to check.
  const [levels,setLevels]=useState<Levels>(FULL_RANGE);
  const [touchedLevels,setTouchedLevels]=useState(false);
  const grabLevel=useRef<'min'|'max'|null>(null);
  // The residuals of the refit, once there is one: judging a new fit against
  // the old model's residuals would say nothing about the new one.
  const [refitted,setRefitted]=useState<{stars:DriftStar[];rms:number}|null>(null);
  useEffect(()=>{setZoom(1);setCentre(null);setRefitted(null);
    setLevels(FULL_RANGE);setTouchedLevels(false)},[camera.id]);
  const load=async()=>{
    setBusy(true);
    try{
      const r=await fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/calibration/drift`,{cache:'no-store'});
      if(!r.ok)throw new Error(await r.text()||'Could not check for lens drift.');
      setDrift(await r.json() as Drift);setError('');
    }catch(e){setError(String((e as Error).message||e))}finally{setBusy(false)}
  };
  useEffect(()=>{void load()},[camera.id]);
  const refit=async()=>{
    setRefitting(true);setOutcome('');
    try{
      const r=await fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/calibration/refit`,{method:'POST'});
      const body=await r.json().catch(()=>null) as {residual_px_before?:number;residual_px_after?:number;
        star_count?:number;stars?:DriftStar[]}|null;
      if(!r.ok)throw new Error((body as unknown as {error?:string})?.error||await r.text()||'The refit was refused.');
      if(body?.stars?.length)setRefitted({stars:body.stars,rms:body.residual_px_after??0});
      setOutcome(`Added a new calibration from ${body?.star_count} stars: `
        +`${body?.residual_px_before?.toFixed(3)} px \u2192 ${body?.residual_px_after?.toFixed(3)} px RMS. `
        +`It is not live \u2014 select it above when you want it used.`);
    }catch(e){setError(String((e as Error).message||e))}finally{setRefitting(false)}
  };
  const frame=drift?.frame;
  const stars=drift?.stars??[];
  const w=frame?.width??0,h=frame?.height??0;
  return <div className="drift-panel">
    <span className="eyebrow">LENS DRIFT, RICHEST FRAME OF THE LAST NIGHT OBSERVED</span>
    {busy?<p role="status">Looking for the richest frame of the last night observed&hellip;</p>
      :error?<p role="alert">{error}</p>
      :!frame?<div className="history-empty">{drift?.note||'No stars have been measured for this camera.'}</div>
      :<>
      <div className="drift-meta">
        <span><strong>{frame.found}</strong> stars found</span>
        <span>calibration in force used <strong>{drift?.calibration_star_count??'an unrecorded number of'}</strong></span>
        <span>offset <strong>{frame.rms_offset_px==null?'\u2014':`${frame.rms_offset_px.toFixed(2)} px`}</strong> RMS</span>
        <time>{frame.observation_utc.replace('T',' ').slice(0,19)} UTC{drift?.current_night?' \u00b7 tonight':' \u00b7 last night observed'}</time>
      </div>
      {w>0&&h>0&&(()=>{
        const spanX=w/zoom,spanY=h/zoom;
        const [cx,cy]=centre??[w/2,h/2];
        const vbox=[Math.min(w-spanX/2,Math.max(spanX/2,cx))-spanX/2,
                    Math.min(h-spanY/2,Math.max(spanY/2,cy))-spanY/2,spanX,spanY];
        // Marker sizes follow the view, not the image, so a ring stays a ring
        // at every magnification instead of swelling into a disc.
        const ring=Math.max(2,spanX/110),dot=Math.max(1,spanX/300);
        const residual=(st:DriftStar)=>st.centroid_x==null||st.centroid_y==null?null
          :[st.centroid_x-st.predicted_x,st.centroid_y-st.predicted_y] as [number,number];
        // After a refit the scatter has to be against the *new* parameters:
        // the old model's residuals say nothing about whether the new fit is
        // any better, and that judgement is the whole point of refitting.
        const shown=refitted?refitted.stars:stars;
        const pairs=shown.map(residual).filter((v):v is [number,number]=>v!=null);
        const {slope,intercept}=transfer(levels);
        const filterId=`drift-levels-${camera.id.replace(/[^a-z0-9]/gi,'')}`;
        // One span for both residual axes, so a systematic shift in x cannot be
        // mistaken for one in y by a difference of scale.
        const reach=Math.max(0.5,...pairs.flatMap(([dx,dy])=>[Math.abs(dx),Math.abs(dy)]));
        const R=120,pad=18,S=R+pad;
        const rms=pairs.length
          ?Math.sqrt(pairs.reduce((t,[dx,dy])=>t+dx*dx+dy*dy,0)/pairs.length):0;
        const mean:[number,number]=pairs.length
          ?[pairs.reduce((t,p)=>t+p[0],0)/pairs.length,pairs.reduce((t,p)=>t+p[1],0)/pairs.length]
          :[0,0];
        return <div className="drift-views">
          <div className="drift-frame">
            <div className="drift-zoom">
              <button type="button" aria-label="Zoom in" onClick={()=>setZoom(z=>Math.min(24,z*1.6))}>+</button>
              <button type="button" aria-label="Zoom out" onClick={()=>setZoom(z=>Math.max(1,z/1.6))}>&minus;</button>
              <button type="button" aria-label="Fit the whole frame" onClick={()=>{setZoom(1);setCentre(null)}}>Fit</button>
              <small>{zoom<1.05?'whole frame':`${zoom.toFixed(1)}\u00d7`}</small>
            </div>
            <svg className="drift-image" viewBox={vbox.join(' ')} role="img"
              aria-label="Projected and fitted star positions on the richest frame of the last night observed"
              onWheel={event=>{event.preventDefault();
                const box=event.currentTarget.getBoundingClientRect();
                const at:[number,number]=[vbox[0]+(event.clientX-box.left)/box.width*vbox[2],
                                          vbox[1]+(event.clientY-box.top)/box.height*vbox[3]];
                setZoom(z=>{const next=Math.min(24,Math.max(1,z*(event.deltaY<0?1.25:0.8)));
                  if(next>1.001)setCentre(at); else setCentre(null);
                  return next})}}
              onPointerDown={event=>{if(zoom<=1.001)return;
                event.currentTarget.setPointerCapture(event.pointerId);
                pan.current={x:event.clientX,y:event.clientY,cx:vbox[0]+vbox[2]/2,cy:vbox[1]+vbox[3]/2}}}
              onPointerMove={event=>{const p=pan.current;if(!p)return;
                const box=event.currentTarget.getBoundingClientRect();
                setCentre([p.cx-(event.clientX-p.x)/box.width*vbox[2],
                           p.cy-(event.clientY-p.y)/box.height*vbox[3]])}}
              onPointerUp={()=>{pan.current=null}}>
              {/* Linear and clipping rather than a curve: the question asked of
                  this image is geometric, and a curve that shifts apparent
                  centroids would be answering a different one. */}
              <defs><filter id={filterId} colorInterpolationFilters="sRGB">
                <feComponentTransfer>
                  <feFuncR type="linear" slope={slope} intercept={intercept}/>
                  <feFuncG type="linear" slope={slope} intercept={intercept}/>
                  <feFuncB type="linear" slope={slope} intercept={intercept}/>
                </feComponentTransfer></filter></defs>
              <image href={`/gaia/api/images/${encodeURIComponent(frame.image_id)}/original`}
                x={0} y={0} width={w} height={h} preserveAspectRatio="none"
                filter={levels.min===0&&levels.max===255?undefined:`url(#${filterId})`}/>
              {/* An open purple ring for where the sky says the star is, with the
                  amber centroid as a filled dot inside it. Concentric rather than
                  side by side, so both are legible at once and the offset reads as
                  the dot sitting off-centre in its own ring. */}
              {stars.map(st=>st.centroid_x==null||st.centroid_y==null?null:
                <line key={`l${st.star_key}`} x1={st.predicted_x} y1={st.predicted_y}
                  x2={st.centroid_x} y2={st.centroid_y} className="drift-link"/>)}
              {stars.map(st=><circle key={`p${st.star_key}`} cx={st.predicted_x} cy={st.predicted_y}
                r={ring} className="drift-predicted"/>)}
              {stars.map(st=>st.centroid_x==null||st.centroid_y==null?null:
                <circle key={`f${st.star_key}`} cx={st.centroid_x} cy={st.centroid_y}
                  r={dot} className="drift-fitted"/>)}
            </svg>
          </div>
          <div className="drift-levels">
            <span className="eyebrow">LEVELS</span>
            {/* White at the top, black at the bottom, the way a colour bar is
                read. Dragging either handle re-stretches the frame beside it. */}
            <div className="drift-bar"
              onPointerDown={event=>{
                event.currentTarget.setPointerCapture(event.pointerId);
                const box=event.currentTarget.getBoundingClientRect();
                const at=fractionToLevel((event.clientY-box.top)/Math.max(1,box.height));
                grabLevel.current=Math.abs(at-levels.max)<Math.abs(at-levels.min)?'max':'min';
                setTouchedLevels(true);
                setLevels(prev=>clampLevels({...prev,[grabLevel.current as 'min'|'max']:at}));}}
              onPointerMove={event=>{
                if(!grabLevel.current)return;
                const box=event.currentTarget.getBoundingClientRect();
                const at=fractionToLevel((event.clientY-box.top)/Math.max(1,box.height));
                setLevels(prev=>clampLevels({...prev,[grabLevel.current as 'min'|'max']:at}));}}
              onPointerUp={()=>{grabLevel.current=null}}>
              <div className="drift-bar-ramp"/>
              {/* The window itself, so what is kept is visible rather than inferred. */}
              <span className="drift-bar-window" style={{
                top:`${levelToFraction(levels.max)*100}%`,
                height:`${Math.max(1,(levelToFraction(levels.min)-levelToFraction(levels.max))*100)}%`}}/>
              <span className="drift-bar-handle" style={{top:`${levelToFraction(levels.max)*100}%`}}
                role="slider" aria-label="White point" aria-valuemin={0} aria-valuemax={255}
                aria-valuenow={levels.max} tabIndex={0}
                onKeyDown={event=>{const d=event.key==='ArrowUp'?4:event.key==='ArrowDown'?-4:0;
                  if(d){event.preventDefault();setTouchedLevels(true);
                    setLevels(prev=>clampLevels({...prev,max:prev.max+d}))}}}/>
              <span className="drift-bar-handle" style={{top:`${levelToFraction(levels.min)*100}%`}}
                role="slider" aria-label="Black point" aria-valuemin={0} aria-valuemax={255}
                aria-valuenow={levels.min} tabIndex={0}
                onKeyDown={event=>{const d=event.key==='ArrowUp'?4:event.key==='ArrowDown'?-4:0;
                  if(d){event.preventDefault();setTouchedLevels(true);
                    setLevels(prev=>clampLevels({...prev,min:prev.min+d}))}}}/>
            </div>
            <div className="drift-levels-key">
              <span>white <strong>{levels.max}</strong></span>
              <span>black <strong>{levels.min}</strong></span>
            </div>
            <button type="button" onClick={()=>{setLevels(FULL_RANGE);setTouchedLevels(false)}}
              disabled={!touchedLevels}>Full range</button>
            {/* From this frame's own sky and brightest star, so the faintest
                measured star still shows rather than a guessed window. */}
            <button type="button" onClick={()=>{
              setLevels(suggest(frame.sky_level??null,frame.brightest_level??null));
              setTouchedLevels(true);}}>Stretch</button>
          </div>
          <div className="drift-residuals">
            <div className="star-plot-head">
              <strong>Residuals{refitted?' (refitted)':''}</strong>
              <small>{pairs.length} stars &middot; &plusmn;{reach.toFixed(1)} px</small></div>
            {/* Fitted minus projected, in image pixels. The shape is the
                diagnosis: a cloud centred on the origin is noise, a cloud
                displaced from it is a pointing offset, and a cloud stretched
                or swirled is a scale or rotation error. */}
            <svg className="drift-scatter" viewBox={`${-S} ${-S} ${2*S} ${2*S}`} role="img"
              aria-label="Scatter of fitted minus projected star positions">
              <circle cx={0} cy={0} r={R} className="drift-axis-ring"/>
              <circle cx={0} cy={0} r={R/2} className="drift-axis-ring"/>
              <line x1={-R} y1={0} x2={R} y2={0} className="drift-axis"/>
              <line x1={0} y1={-R} x2={0} y2={R} className="drift-axis"/>
              {rms>0&&<circle cx={0} cy={0} r={rms/reach*R} className="drift-rms"/>}
              {pairs.map(([dx,dy],n)=><circle key={n} cx={dx/reach*R} cy={dy/reach*R}
                r={2.4} className="drift-fitted"/>)}
              {pairs.length>0&&<circle cx={mean[0]/reach*R} cy={mean[1]/reach*R} r={5}
                className="drift-mean"/>}
              <text x={R} y={-6} className="drift-tick" textAnchor="end">+{reach.toFixed(1)} px</text>
              <text x={4} y={-R+2} className="drift-tick">&minus;y</text>
            </svg>
            <div className="drift-residual-key">
              <span>RMS <strong>{rms.toFixed(2)} px</strong></span>
              <span>mean <strong>{mean[0].toFixed(2)}, {mean[1].toFixed(2)}</strong></span>
            </div>
            <small>Dashed ring is the RMS; the larger open dot is the mean offset.
              A cloud sitting off the origin is a pointing error rather than noise.
              {refitted?' Against the newly fitted parameters, so this is the quality of the new fit.'
                       :' Against the calibration in force.'}</small>
          </div>
        </div>;})()}
      <div className="drift-legend">
        <span><i className="swatch-predicted"/>projected from the star ephemeris</span>
        <span><i className="swatch-fitted"/>fitted Gaussian centroid</span>
      </div>
      <div className="drift-actions">
        <button type="button" disabled={!drift?.refit_available||refitting} onClick={()=>void refit()}>
          {refitting?'Fitting\u2026':'Refit lens parameters (WISC/AIDA)'}</button>
        <small>{drift?.refit_reason}</small>
      </div>
      {outcome&&<p className="drift-outcome" role="status">{outcome}</p>}
      <small className="drift-note">A refit is added to the list above and left unselected.
        Which calibration a camera uses stays an administrator&rsquo;s decision.</small>
    </>}
  </div>;
}

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
    <CalibrationDrift camera={camera}/>
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
  const [view, setView] = useState<ViewName>('globe'); const [playing, setPlaying] = useState(false); const [sunLock,setSunLock]=useState(false); const [speed,setSpeed]=useState(32); const zoomRef=useRef<((action:'in'|'out'|'reset')=>void)|null>(null); const [instrumentSites,setInstrumentSites]=useState<Record<string,string>>({}); const [sortKey,setSortKey]=useState<SortKey>('name'); const[photometryCamera,setPhotometryCamera]=useState<Camera|null>(null); const[calibratingCamera,setCalibratingCamera]=useState<Camera|null>(null); const [sortDesc,setSortDesc]=useState(false); const [timeMinutes,setTimeMinutes]=useState(1440);const [suggesting,setSuggesting]=useState(false);const [sent,setSent]=useState(false);const[editingCamera,setEditingCamera]=useState<Camera|null>(null);const[maskingCamera,setMaskingCamera]=useState<Camera|null>(null);const[browsingCamera,setBrowsingCamera]=useState<Camera|null>(null);const[cameras,setCameras]=useState<Camera[]>([]);
  useEffect(()=>{void fetch('/gaia/public/manifest.json',{cache:'no-store'}).then(async r=>{if(!r.ok)throw new Error();return await r.json() as {cameras?:{source_id:string;website_url:string|null}[]}}).then(manifest=>setInstrumentSites(Object.fromEntries((manifest.cameras||[]).filter(c=>c.website_url).map(c=>[c.source_id,c.website_url as string])))).catch(()=>{})},[]);
  const [cameraListLoading,setCameraListLoading]=useState(true),[cameraListError,setCameraListError]=useState(''),[cameraListAttempt,setCameraListAttempt]=useState(0);
  useEffect(()=>{
    const controller=new AbortController();let disposed=false,timedOut=false;
    setCameraListLoading(true);setCameraListError('');
    const timeout=setTimeout(()=>{timedOut=true;controller.abort()},45000);
    void fetch('/gaia/api/sources',{cache:'no-store',signal:controller.signal}).then(async r=>{
      if(!r.ok)throw Error('Camera registry request failed (HTTP '+r.status+').');
      const rows=await r.json();if(!Array.isArray(rows))throw Error('Invalid camera registry response.');
      if(!disposed)setCameras(rows as Camera[]);
    }).catch(error=>{if(!disposed)setCameraListError(timedOut?'Camera list loading timed out. Please retry.':String(error.message||'Could not load camera list.'))})
      .finally(()=>{clearTimeout(timeout);if(!disposed)setCameraListLoading(false)});
    return()=>{disposed=true;clearTimeout(timeout);controller.abort()};
  },[cameraListAttempt]);

  useEffect(()=>{if(view!=='cameras')return;const refresh=()=>setCameraListAttempt(n=>n+1);refresh();const timer=setInterval(refresh,15000);window.addEventListener('focus',refresh);return()=>{clearInterval(timer);window.removeEventListener('focus',refresh)}},[view]);

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
      <button className="brand" onClick={() => setView('globe')} aria-label="GAIA home"><svg className="aurora-brand" viewBox="0 0 64 64" aria-hidden="true"><defs><linearGradient id="aurora-logo" x1="0" y1="0" x2="1" y2="1"><stop stopColor="#baff78"/><stop offset=".5" stopColor="#56f0c5"/><stop offset="1" stopColor="#559ef8"/></linearGradient></defs><circle cx="32" cy="34" r="24" fill="#092b3b" stroke="#67c8d7" strokeWidth="1.5"/><path d="M18 20l9-4 6 6-4 8 6 5-5 7-2 13-7-9 1-10-7-5zM44 21l7 7-5 8-8-3 1-9z" fill="#3d8580"/><path d="M9 30C1 18 16 5 34 7S63 20 54 30C44 41 15 37 9 26" fill="none" stroke="url(#aurora-logo)" strokeWidth="5" strokeLinecap="round"/><path d="M12 28C18 37 44 38 53 28M14 24l2 7M22 29l1 6M33 31v6M43 29l-1 6" fill="none" stroke="#baffae" strokeWidth="1.3" opacity=".8"/></svg><span><strong><i className="rook-mark" aria-hidden="true">♜</i> GAIA</strong><small>Global Auroral Image Aggregator</small></span></button>
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
        {view === 'globe' && <PlaybackToolbar playing={playing} onPlay={()=>{if(!playing&&timeMinutes===1440&&historyMinutes.length)setTimeMinutes(historyMinutes[0]);setPlaying(!playing)}} speed={speed} onSpeed={setSpeed} sunLock={sunLock} onSunLock={setSunLock} at={new Date(selectedEpoch).toISOString()} max={1440} value={timeMinutes} onPause={()=>setPlaying(false)} onScrub={v=>{setTimeMinutes(v);window.dispatchEvent(new CustomEvent('gaia-scrub-overview',{detail:historyEnd.current-(1440-v)*60000}))}} onLatest={()=>{setPlaying(false);setTimeMinutes(1440);window.dispatchEvent(new Event('gaia-leave-overview'))}}/>}
        {view === 'cameras' && <div className="route-panel"><div className="route-heading"><Satellite/><div><span className="eyebrow">CAMERA REGISTRY</span><h2>{cameraListLoading&&!cameras.length?'Loading cameras…':cameraListError&&!cameras.length?'Camera list unavailable':cameras.length+' image sources'}</h2></div></div>{cameraListLoading&&<div role="status" aria-live="polite"><p>Loading the camera list from the server…</p><progress aria-label="Camera list loading" style={{width:'100%',height:16,accentColor:'#70d9ac'}}/><p>The full list appears when the server responds. Thumbnails load separately as you scroll.</p></div>}{cameraListError&&<div role="alert"><p>{cameraListError}</p><button type="button" onClick={()=>setCameraListAttempt(n=>n+1)}>Retry camera list</button></div>}{!cameraListLoading&&!cameraListError&&!cameras.length&&<p>No cameras are registered.</p>}<p className="panel-intro">Red cameras can be ingested and credited, but cannot enter the 100 km mosaic until their lens model is fitted.</p><div className="camera-sort" role="group" aria-label="Sort camera stations"><span className="eyebrow">SORT BY</span>{sortOptions.map(([key,label])=><button key={key} type="button" aria-pressed={sortKey===key} onClick={()=>{if(sortKey===key){setSortDesc(!sortDesc)}else{setSortKey(key);setSortDesc(false)}}} title={sortKey===key?`Reverse the ${label.toLowerCase()} order`:`Sort by ${label.toLowerCase()}`}>{label}{sortKey===key?(sortDesc?' \u25bc':' \u25b2'):''}</button>)}<small role="status">{sortHint}{(sortKey==='latitude_deg'||sortKey==='longitude_deg')&&unlocated>0?` \u00b7 ${unlocated} without a location last`:''}</small></div><div className="camera-list">{sortedCameras.map(camera=><article key={camera.id} className={camera.enabled?'':'camera-disabled'}><div className="camera-identity"><div className="camera-thumbnail"><span>NO FRAME</span><img loading="lazy" src={`/gaia/api/sources/${camera.id}/latest`} alt={`Current ${camera.name} frame`} onLoad={e=>e.currentTarget.classList.add('loaded')}/></div><div>{instrumentSites[camera.id]?<a className="camera-link" href={instrumentSites[camera.id]} target="_blank" rel="noreferrer noopener" title={`Open the ${camera.name} instrument page`}><strong>{camera.name}</strong> ↗</a>:<strong>{camera.name}</strong>}<small>{camera.producer}</small></div></div><div className="camera-meta"><span>{camera.latitude_deg==null||camera.longitude_deg==null?'Location needed':`${Math.abs(camera.latitude_deg).toFixed(2)}°${camera.latitude_deg>=0?'N':'S'} · ${Math.abs(camera.longitude_deg).toFixed(2)}°${camera.longitude_deg>=0?'E':'W'}`}</span><small>{camera.timestamp_mode==='archive'?'Archive timestamp':'Download timestamp'} · {camera.images_24h} images / 24 h</small><div><button type="button" onClick={()=>setBrowsingCamera(camera)}>Browse history</button><button type="button" onClick={()=>setCalibratingCamera(camera)}>Calibrations</button><button type="button" onClick={()=>setPhotometryCamera(camera)}>Star photometry</button><button type="button" onClick={()=>setEditingCamera(camera)}>Adjust location</button><button type="button" onClick={()=>setMaskingCamera(camera)}>Edit crop &amp; mask</button><button type="button" onClick={()=>void toggleCamera(camera)}>{camera.enabled?'Pause camera':'Resume camera'}</button><button className="danger" type="button" onClick={()=>void removeCamera(camera)}>Remove camera</button></div><label className="camera-quality" title="Down-weight a camera whose images are noisier, softer or less well exposed than the rest. The mosaic divides by the sum of the weights, so this only matters where projections overlap."><span>Quality weight</span><select value={camera.quality_exponent??0} onChange={event=>void setQuality(camera,Number(event.target.value))}>{qualitySteps.map(exponent=><option key={exponent} value={exponent}>{exponent===0?'1 \u00b7 full weight':`1/${2**-exponent} \u00b7 2^${exponent}`}</option>)}</select></label></div><div className={`calibration-state ${camera.calibrated?'calibrated':''}`}><b>{camera.enabled?(camera.calibrated?'CALIBRATED':'NOT CALIBRATED'):'PAUSED'}</b>{camera.enabled&&camera.calibrated&&<div role="status" style={{maxWidth:230,margin:'8px 0',fontSize:12}}>{({queued:'Queued for projection',processing:'Projecting & scaling images',publishing:'Publishing projected images',ready:'Projected images ready',no_images:'No recent images to project',error:'Projection failed',stalled:'Processing stalled'} as Record<string,string>)[camera.processing?.state||'queued']} {(camera.processing?.ready??0)>0&&<span> · {camera.processing?.ready} frames</span>}{camera.processing?.state==='processing'&&<><progress aria-label={camera.name+' projection progress'} max={camera.processing.total||1} value={camera.processing.done||0} style={{width:'100%'}}/><span>{camera.processing.done||0} / {camera.processing.total||0} frames attempted</span></>}{['queued','publishing'].includes(camera.processing?.state||'queued')&&<progress aria-label={camera.name+' processing pending'} style={{width:'100%'}}/>}{camera.processing?.error&&<small>{camera.processing.error}</small>}</div>}{camera.enabled&&<a className={camera.calibrated?'recalibrate':undefined} href={`/aida/?gaia=1&source_id=${encodeURIComponent(camera.id)}`} target="_blank" rel="noreferrer noopener" title={camera.calibrated?'Fit a new lens model from the latest frame; the current calibration is kept':'Fit a lens model from the latest frame'}>{camera.calibrated?'Recalibrate latest in AIDA ↗':'Calibrate latest in AIDA ↗'}</a>}</div></article>)}</div></div>}
        {view === 'status' && <div className="route-panel"><div className="route-heading"><Activity/><div><span className="eyebrow">INGESTION PIPELINE</span><h2>Data flow status</h2></div></div><div className="pipeline">{['Acquire','Calibrate','Mask','Project','Tessellate'].map((x,i)=><div key={x}><span className={i===0?'running':''}>{i===0?'↻':'·'}</span><strong>{x}</strong><small>{i===0?'polling sources':'framework ready'}</small></div>)}</div><div className="source-list">{cameras.map(camera=><article key={camera.id}><span className={`source-state ${camera.state}`}/><div><strong>{camera.name}</strong><small>{camera.producer}</small></div><div><b>{camera.timestamp_mode==='archive'?'Archive time':'Download time'}</b><small>{camera.message||`${camera.images_24h} images in 24 h`}</small></div></article>)}</div></div>}
        {view === 'calibrate' && <div className="route-panel narrow"><div className="route-heading"><Aperture/><div><span className="eyebrow">ADD COVERAGE</span><h2>Contribute an imager</h2></div></div><p>Every image retains the producer’s name, institution, copyright and requested acknowledgement. Fixed cameras normally reuse a seasonal calibration; phone images are calibrated individually.</p><div className="choice-grid"><article><span>01</span><h3>Calibrate the image</h3><p>Use AIDA/WISC to match stars and fit the lens. GAIA accepts its native calibration HDF5.</p><a href="/aida/?gaia=1" target="_blank">Open AIDA calibrator ↗</a></article><article><span>02</span><h3>Register a source</h3><p>Add a crawler JSON entry with cadence, timestamp policy, station location, producer and copyright.</p><a href="https://github.com/jvierine/gaia#adding-an-image-source" target="_blank">Read the source guide ↗</a></article></div></div>}
        {view === 'about' && <div className="route-panel narrow"><div className="route-heading"><CircleHelp/><div><span className="eyebrow">OPEN SCIENCE INFRASTRUCTURE</span><h2>About the data center &amp; credits</h2></div></div><ServiceDescription/><h3>Authors</h3><p>Juha Vierinen and Björn Gustavsson</p><p>GAIA is an independent image aggregation and geographic projection service.</p><p>Image copyrights remain with their producers. GAIA does not transfer ownership or replace the producer’s terms.</p><h3>Suggest an improvement</h3><p>Report a bad image, missing source, calibration issue or viewer idea.</p><button className="suggest-link" onClick={()=>{setSuggesting(true);setSent(false)}}><Lightbulb size={14}/> Suggest an improvement</button><ProviderContacts/><Credits/></div>}
        <div className="source-strip">{view==='globe'&&<div className="strip-zoom" role="group" aria-label="Map controls"><button type="button" aria-label="Zoom in" title="Zoom in" onClick={()=>zoomRef.current?.('in')}>+</button><button type="button" aria-label="Zoom out" title="Zoom out" onClick={()=>zoomRef.current?.('out')}>&minus;</button><button type="button" aria-label="Reset globe" title="Reset view" onClick={()=>zoomRef.current?.('reset')}>◎</button></div>}<div><Database size={15} /><span><strong>{playing?'Smoothed playback':timeMinutes===1440?'Delayed live frames':'Archived frames'}</strong> {framesLoading?'Loading…':'Selected at or before timeline time; gaps over 10 min omitted'}</span></div><div className="legend"><span><span className="dot live" /> clear</span><span><span className="dot delay" /> delayed</span><span><span className="dot idle" /> unavailable</span></div></div>
      </section>
    </section>
    {suggesting&&<div className="modal-backdrop" role="presentation"><form className="suggestion-box" onSubmit={submitSuggestion}><button type="button" className="close" onClick={()=>setSuggesting(false)} aria-label="Close"><X/></button>{sent?<div className="sent"><Send/><h2>Suggestion received</h2><p>Thank you. The GAIA team will review it before deciding what to send to Codex.</p></div>:<><span className="eyebrow">HUMAN-REVIEWED INPUT</span><h2>Suggest an improvement</h2><p>Report a bad image, missing source, calibration issue or viewer idea.</p><label>Your suggestion<textarea name="suggestion" minLength={4} maxLength={8000} required placeholder="What should we improve?"/></label><div className="form-row"><label>Name <span>optional</span><input name="name"/></label><label>Contact <span>optional</span><input name="contact"/></label></div><button className="send-button" type="submit"><Send size={15}/> Send suggestion</button></>}</form></div>}
    {photometryCamera&&<StarPhotometry camera={photometryCamera} onClose={()=>setPhotometryCamera(null)}/>}
    {calibratingCamera&&<CalibrationPicker camera={calibratingCamera} onClose={()=>setCalibratingCamera(null)}/>}
    {editingCamera&&<LocationEditor camera={editingCamera} onClose={()=>setEditingCamera(null)} onSaved={saved=>setCameras(rows=>rows.map(row=>row.id===saved.id?saved:row))}/>} 
    {maskingCamera&&<MaskEditor camera={maskingCamera} onClose={()=>setMaskingCamera(null)}/>} 
    {browsingCamera&&<FrameBrowser camera={browsingCamera} onClose={()=>setBrowsingCamera(null)}/>}
  </main>;
}
