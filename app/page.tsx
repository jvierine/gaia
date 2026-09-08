'use client';

import { Activity, Aperture, CircleHelp, Database, Lightbulb, Satellite, Send, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { startGaiaGlobe } from '../src/globe';

type ViewName = 'globe' | 'cameras' | 'status' | 'calibrate' | 'about';
type Camera = {id:string;name:string;producer:string;state:string;timestamp_mode:string;latitude_deg:number|null;longitude_deg:number|null;calibrated:boolean;enabled:boolean;images_24h:number;message?:string|null};

const fallbackCameras:Camera[]=[
  {id:'starvisor-popovo',name:'STARVISOR Popovo',producer:'Popovo camera operator / STARVISOR',state:'waiting',timestamp_mode:'download_time',latitude_deg:57,longitude_deg:38,calibrated:false,enabled:true,images_24h:0},
  {id:'irf-kiruna-alis',name:'IRF Kiruna all-sky',producer:'Swedish Institute of Space Physics',state:'waiting',timestamp_mode:'archive',latitude_deg:67.84,longitude_deg:20.41,calibrated:false,enabled:true,images_24h:0},
];

function LocationEditor({camera,onClose,onSaved}:{camera:Camera,onClose:()=>void,onSaved:(camera:Camera)=>void}){
  const [lat,setLat]=useState(camera.latitude_deg??0);const[lon,setLon]=useState(camera.longitude_deg??0);const[saving,setSaving]=useState(false);
  const point=(event:React.PointerEvent<HTMLDivElement>)=>{const r=event.currentTarget.getBoundingClientRect();setLon(Math.max(-180,Math.min(180,(event.clientX-r.left)/r.width*360-180)));setLat(Math.max(-90,Math.min(90,90-(event.clientY-r.top)/r.height*180)))};
  async function save(){setSaving(true);try{const response=await fetch(`/gaia/api/sources/${camera.id}/location`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({latitude_deg:lat,longitude_deg:lon})});if(!response.ok)throw new Error();onSaved({...camera,latitude_deg:lat,longitude_deg:lon});onClose()}catch{alert('The camera service is not available yet. Please try again shortly.')}finally{setSaving(false)}}
  return <div className="modal-backdrop"><div className="location-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CAMERA POSITION</span><h2>{camera.name}</h2><p>Drag the marker or enter exact WGS84 coordinates.</p><div className="location-map" onPointerDown={point} onPointerMove={e=>{if(e.buttons)point(e)}}><span className="map-equator"/><span className="map-meridian"/><i style={{left:`${(lon+180)/360*100}%`,top:`${(90-lat)/180*100}%`}}/></div><div className="form-row"><label>Latitude<input type="number" min="-90" max="90" step="0.0001" value={lat.toFixed(4)} onChange={e=>setLat(Number(e.target.value))}/></label><label>Longitude<input type="number" min="-180" max="180" step="0.0001" value={lon.toFixed(4)} onChange={e=>setLon(Number(e.target.value))}/></label></div><button className="send-button" onClick={save} disabled={saving}>{saving?'Saving…':'Save camera position'}</button></div></div>
}

function MaskEditor({camera,onClose}:{camera:Camera,onClose:()=>void}){
  const[mode,setMode]=useState<'crop'|'mask'|'nodes'>('mask');
  const[loading,setLoading]=useState(true);const[loadError,setLoadError]=useState('');
  useEffect(()=>{
    const controller=new AbortController();
    setLoading(true);setLoadError('');
    void fetch(`/gaia/api/sources/${encodeURIComponent(camera.id)}/settings`,{cache:'no-store',signal:controller.signal}).then(async response=>{
      if(!response.ok)throw new Error('Could not load saved crop and mask. Close and reopen to retry.');
      const settings=await response.json();
      if(controller.signal.aborted)return;
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
  async function save(){if(loading||loadError)return;if(polygons.some(p=>p.length>0&&p.length<3)){alert('Each mask needs at least three points. Finish or delete the incomplete mask before saving.');return;}setSaving(true);try{const response=await fetch(`/gaia/api/sources/${camera.id}/settings`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({crop:{left:crop[0]/100,top:crop[1]/100,right:crop[2]/100,bottom:crop[3]/100},mask:{coordinate_system:'normalized_image',polygons:polygons.filter(p=>p.length>=3)}})});if(!response.ok)throw new Error();onClose()}catch{alert('Could not save crop and mask.')}finally{setSaving(false)}}
  if(loading||loadError)return <div className="modal-backdrop"><div className="mask-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><h2>{camera.name}</h2><p role="status">{loadError||'Loading saved crop and mask…'}</p></div></div>;
  return <div className="modal-backdrop"><div className="mask-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CROP &amp; OBSTRUCTION MASK</span><h2>{camera.name}</h2><p>Drag inside the cyan crop rectangle to move it. Drag any edge or corner to resize. Draw mask adds a node on every click, including outside the image. Choose Move nodes to drag existing nodes. Each mask is a separate excluded obstruction area; use New mask for another tree, building, or other obstruction.</p><div className="mask-actions"><button aria-pressed={mode==='crop'} onClick={()=>setMode('crop')}>Crop — move / resize</button><button aria-pressed={mode==='mask'} onClick={()=>setMode('mask')}>Draw mask — add nodes</button><button aria-pressed={mode==='nodes'} onClick={()=>setMode('nodes')}>Move nodes</button><span role="status">{points.length} mask nodes</span></div><div className="mask-actions mask-selector"><button onClick={()=>{drag.current=null;setPolygons(all=>[...all,[]]);setActiveMask(polygons.length);setMode('mask')}}>+ New mask</button>{polygons.map((p,i)=><button key={i} aria-pressed={activeMask===i} onClick={()=>{drag.current=null;setActiveMask(i);setMode('nodes')}}>Mask {i+1} ({p.length} nodes)</button>)}<button onClick={()=>{drag.current=null;setPolygons(all=>all.length===1?[[]]:all.filter((_,i)=>i!==activeMask));setActiveMask(0)}}>Delete selected mask</button></div><div className={`mask-image editing-${mode}`} onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={()=>drag.current=null} onPointerCancel={()=>drag.current=null} onLostPointerCapture={()=>drag.current=null}><div className="mask-frame"><img draggable={false} ref={imageRef} src={`/gaia/api/sources/${camera.id}/latest`} alt={`Latest frame from ${camera.name}`}/><svg viewBox="0 0 1 1" preserveAspectRatio="none">{polygons.map((polygon,i)=><polygon key={i} style={{opacity:i===activeMask?1:.45}} points={polygon.map(p=>p.join(',')).join(' ')}/>)}<rect className="crop-box" data-kind="crop" x={crop[0]/100} y={crop[1]/100} width={(crop[2]-crop[0])/100} height={(crop[3]-crop[1])/100}/><rect className="crop-grab" data-kind="crop" x={crop[0]/100} y={crop[1]/100} width={(crop[2]-crop[0])/100} height=".035"/></svg>{points.map((p,i)=><span className="mask-node-dot" data-kind="node" data-index={i} key={i} style={{left:`${p[0]*100}%`,top:`${p[1]*100}%`}}/>)}{mode==='crop'&&<div className="crop-interaction" data-kind="crop" style={{left:`${crop[0]}%`,top:`${crop[1]}%`,width:`${crop[2]-crop[0]}%`,height:`${crop[3]-crop[1]}%`}}>{['n','s','e','w','nw','ne','sw','se'].map(edge=><span key={edge} data-kind="crop" data-edge={edge} className={`crop-handle handle-${edge}`} title={`Resize crop ${edge}`}/>)}</div>}</div></div><details><summary>Exact crop coordinates (optional)</summary><div className="crop-grid">{['Left','Top','Right','Bottom'].map((label,i)=><label key={label}>{label}<input type="number" min="0" max="100" step="0.1" value={Number(crop[i].toFixed(1))} onChange={e=>setCrop(v=>v.map((x,j)=>j===i?Number(e.target.value):x))}/></label>)}</div></details><div className="mask-actions"><button onClick={()=>setPoints(p=>p.slice(0,-1))}>Undo point</button><button onClick={()=>setPoints([])}>Clear selected mask</button><button className="send-button" onClick={save} disabled={saving}>{saving?'Saving…':'Save crop and mask'}</button></div></div></div>
}

function Credits(){
  const[providers,setProviders]=useState<{name:string;website:string|null;acknowledgement:string;copyright:string}[]>([]);
  const[error,setError]=useState(false);
  useEffect(()=>{void fetch('/gaia/api/credits').then(r=>{if(!r.ok)throw new Error();return r.json()}).then(setProviders).catch(()=>setError(true))},[]);
  return <section aria-label="Data provider credits"><h3>Data provider credits</h3>{error?<p>Could not load provider credits. Please reopen this page to retry.</p>:providers.length?providers.map((p,i)=><article key={i}><h4>{p.website&&/^https?:\/\//.test(p.website)?<a href={p.website} target="_blank" rel="noopener noreferrer">{p.name} ↗</a>:p.name}</h4><p>{p.acknowledgement}</p><p>{p.copyright}</p></article>):<p>Loading credits…</p>}</section>;
}

function Globe({epochMillis,live,onCredits,onLoading}:{epochMillis:number;live:boolean;onCredits:()=>void;onLoading:(loading:boolean)=>void}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const epochRef=useRef(()=>epochMillis);epochRef.current=()=>live?Date.now():epochMillis;
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    try { return startGaiaGlobe(canvas,()=>epochRef.current(),onLoading); } catch (error) { onLoading(false);console.error('GAIA WebGL failed', error); canvas.classList.add('webgl-failed'); }
  }, []);

  return <div className="globe-stage">
    <button type="button" onClick={onCredits} aria-label="Data provider credits" title="Data provider credits" style={{position:'absolute',left:18,top:18,zIndex:2,border:0,background:'#031018cc',color:'#d6eee8',borderRadius:'50%',width:32,height:32,fontSize:24,cursor:'pointer'}}>ⓘ</button>
    <canvas ref={canvasRef} className="globe-canvas" aria-label="Interactive WebGL Earth with auroral image coverage" />
    <div className="coverage-readout"><span>67.2N 21.0E</span><strong>100 km emission shell</strong></div>
    <div className="globe-tools" aria-label="Map controls"><button type="button" aria-label="Zoom in" onClick={()=>canvasRef.current?.dispatchEvent(new CustomEvent('gaia-zoom',{detail:'in'}))}>+</button><button type="button" aria-label="Zoom out" onClick={()=>canvasRef.current?.dispatchEvent(new CustomEvent('gaia-zoom',{detail:'out'}))}>−</button><button type="button" aria-label="Reset globe" onClick={()=>canvasRef.current?.dispatchEvent(new CustomEvent('gaia-zoom',{detail:'reset'}))}>◎</button></div>
  </div>;
}

export default function Home() {
  const historyEnd=useRef(Date.now());
  const[historyMinutes,setHistoryMinutes]=useState<number[]>([]);
  useEffect(()=>{const load=()=>void fetch('/gaia/api/history',{cache:'no-store'}).then(r=>r.json()).then((times:string[])=>setHistoryMinutes([...new Set(times.map(t=>Math.max(0,Math.min(1440,Math.ceil((Date.parse(t)+60000-historyEnd.current)/60000)+1440))))].sort((a,b)=>a-b))).catch(console.warn);load();const timer=setInterval(load,60000);return()=>clearInterval(timer)},[]);
  const[framesLoading,setFramesLoading]=useState(false);
  const [view, setView] = useState<ViewName>('globe'); const [playing, setPlaying] = useState(false); const [timeMinutes,setTimeMinutes]=useState(1440);const [suggesting,setSuggesting]=useState(false);const [sent,setSent]=useState(false);const[editingCamera,setEditingCamera]=useState<Camera|null>(null);const[maskingCamera,setMaskingCamera]=useState<Camera|null>(null);const[cameras,setCameras]=useState<Camera[]>(fallbackCameras);
  useEffect(()=>{void fetch('/gaia/api/sources',{cache:'no-store'}).then(async r=>{if(!r.ok)throw new Error();return await r.json() as Camera[]}).then(rows=>{if(rows.length)setCameras(rows)}).catch(()=>{})},[]);
  useEffect(()=>{if(!playing||framesLoading||view!=='globe')return;const timer=window.setTimeout(()=>setTimeMinutes(v=>historyMinutes.find(m=>m>v)??historyMinutes[0]??1440),350);return()=>window.clearTimeout(timer)},[playing,framesLoading,timeMinutes,view,historyMinutes]);
  const selectedEpoch=timeMinutes===1440?Date.now():historyEnd.current-(1440-timeMinutes)*60_000;
  async function submitSuggestion(e:React.FormEvent<HTMLFormElement>){e.preventDefault();const form=new FormData(e.currentTarget);const body={name:form.get('name')||undefined,contact:form.get('contact')||undefined,suggestion:form.get('suggestion'),page_url:location.href};try{const response=await fetch('/gaia/api/suggestions',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});if(!response.ok)throw new Error();setSent(true)}catch{alert('The suggestion service is not available yet. Please try again shortly.')}}
  async function toggleCamera(camera:Camera){const enabled=!camera.enabled;const response=await fetch(`/gaia/api/sources/${camera.id}/enabled`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({enabled})});if(response.ok)setCameras(rows=>rows.map(row=>row.id===camera.id?{...row,enabled}:row))}
  return <main className="app-shell">
    <header className="topbar">
      <button className="brand" onClick={() => setView('globe')} aria-label="GAIA home"><svg className="aurora-brand" viewBox="0 0 64 64" aria-hidden="true"><defs><linearGradient id="aurora-logo" x1="0" y1="0" x2="1" y2="1"><stop stopColor="#baff78"/><stop offset=".5" stopColor="#56f0c5"/><stop offset="1" stopColor="#559ef8"/></linearGradient></defs><circle cx="32" cy="34" r="24" fill="#092b3b" stroke="#67c8d7" strokeWidth="1.5"/><path d="M18 20l9-4 6 6-4 8 6 5-5 7-2 13-7-9 1-10-7-5zM44 21l7 7-5 8-8-3 1-9z" fill="#3d8580"/><path d="M9 30C1 18 16 5 34 7S63 20 54 30C44 41 15 37 9 26" fill="none" stroke="url(#aurora-logo)" strokeWidth="5" strokeLinecap="round"/><path d="M12 28C18 37 44 38 53 28M14 24l2 7M22 29l1 6M33 31v6M43 29l-1 6" fill="none" stroke="#baffae" strokeWidth="1.3" opacity=".8"/></svg><span><strong>GAIA</strong><small>Global Auroral Image Archive</small></span></button>
      <nav aria-label="Main navigation">
        <button className={view === 'globe' ? 'active' : ''} onClick={() => setView('globe')}>Globe</button>
        <button className={view === 'cameras' ? 'active' : ''} onClick={() => setView('cameras')}>Cameras</button>
        <button className={view === 'status' ? 'active' : ''} onClick={() => setView('status')}>Data status</button>
        <button className={view === 'calibrate' ? 'active' : ''} onClick={() => setView('calibrate')}>Contribute</button>
        <button className={view === 'about' ? 'active' : ''} onClick={() => setView('about')}>About</button>
      </nav>
      <div className="live-pill"><span /> LIVE · 12:42 UTC</div>
    </header>
    <section className="workspace viewer-only">
      <section className="viewer">
        {view === 'globe' && <Globe onLoading={setFramesLoading} epochMillis={selectedEpoch} live={timeMinutes===1440} onCredits={()=>setView('about')}/>}
        {view === 'globe' && <div className="timeline-dock"><button onClick={()=>{if(!playing&&timeMinutes===1440&&historyMinutes.length)setTimeMinutes(historyMinutes[0]);setPlaying(!playing)}} aria-label={playing?'Pause 24 hour playback':'Play last 24 hours'}>{playing?'Ⅱ':'▶'}</button><div><div className="timeline-label"><strong>LAST 24 HOURS</strong><time>{new Date(selectedEpoch).toISOString().replace('T',' ').slice(0,19)} UTC {framesLoading?'· loading':''}</time></div><input type="range" min="0" max="1440" step="1" value={timeMinutes} onChange={e=>{setTimeMinutes(Number(e.target.value));setPlaying(false)}}/><div className="timeline-ticks"><span>−24 h</span><span>−18 h</span><span>−12 h</span><span>−6 h</span><span>now</span></div></div></div>}
        {view === 'cameras' && <div className="route-panel"><div className="route-heading"><Satellite/><div><span className="eyebrow">CAMERA REGISTRY</span><h2>{cameras.length} image sources</h2></div></div><p className="panel-intro">Red cameras can be ingested and credited, but cannot enter the 100 km mosaic until their lens model is fitted.</p><div className="camera-list">{cameras.map(camera=><article key={camera.id} className={camera.enabled?'':'camera-disabled'}><div className="camera-identity"><div className="camera-thumbnail"><span>NO FRAME</span><img loading="lazy" src={`/gaia/api/sources/${camera.id}/latest`} alt={`Current ${camera.name} frame`} onLoad={e=>e.currentTarget.classList.add('loaded')}/></div><div><strong>{camera.name}</strong><small>{camera.producer}</small></div></div><div className="camera-meta"><span>{camera.latitude_deg==null||camera.longitude_deg==null?'Location needed':`${Math.abs(camera.latitude_deg).toFixed(2)}°${camera.latitude_deg>=0?'N':'S'} · ${Math.abs(camera.longitude_deg).toFixed(2)}°${camera.longitude_deg>=0?'E':'W'}`}</span><small>{camera.timestamp_mode==='archive'?'Archive timestamp':'Download timestamp'} · {camera.images_24h} images / 24 h</small><div><button type="button" onClick={()=>setEditingCamera(camera)}>Adjust location</button><button type="button" onClick={()=>setMaskingCamera(camera)}>Edit crop &amp; mask</button><button type="button" onClick={()=>void toggleCamera(camera)}>{camera.enabled?'Disable camera':'Enable camera'}</button></div></div><div className={`calibration-state ${camera.calibrated?'calibrated':''}`}><b>{camera.enabled?(camera.calibrated?'CALIBRATED':'NOT CALIBRATED'):'DISABLED'}</b>{camera.enabled&&!camera.calibrated&&<a href={`https://juha.no/aida/?gaia=1&source_id=${encodeURIComponent(camera.id)}&return=https%3A%2F%2Fjuha.no%2Fgaia%2F`} target="_blank">Calibrate in AIDA ↗</a>}</div></article>)}</div></div>}
        {view === 'status' && <div className="route-panel"><div className="route-heading"><Activity/><div><span className="eyebrow">INGESTION PIPELINE</span><h2>Data flow status</h2></div></div><div className="pipeline">{['Acquire','Calibrate','Mask','Project','Tessellate'].map((x,i)=><div key={x}><span className={i===0?'running':''}>{i===0?'↻':'·'}</span><strong>{x}</strong><small>{i===0?'polling sources':'framework ready'}</small></div>)}</div><div className="source-list">{cameras.map(camera=><article key={camera.id}><span className={`source-state ${camera.state}`}/><div><strong>{camera.name}</strong><small>{camera.producer}</small></div><div><b>{camera.timestamp_mode==='archive'?'Archive time':'Download time'}</b><small>{camera.message||`${camera.images_24h} images in 24 h`}</small></div></article>)}</div></div>}
        {view === 'calibrate' && <div className="route-panel narrow"><div className="route-heading"><Aperture/><div><span className="eyebrow">ADD COVERAGE</span><h2>Contribute an imager</h2></div></div><p>Every image retains the producer’s name, institution, copyright and requested acknowledgement. Fixed cameras normally reuse a seasonal calibration; phone images are calibrated individually.</p><div className="choice-grid"><article><span>01</span><h3>Calibrate the image</h3><p>Use AIDA/WISC to match stars and fit the lens. GAIA accepts its native calibration HDF5.</p><a href="https://juha.no/aida/?gaia=1" target="_blank">Open AIDA calibrator ↗</a></article><article><span>02</span><h3>Register a source</h3><p>Add a crawler JSON entry with cadence, timestamp policy, station location, producer and copyright.</p><a href="https://github.com/jvierine/gaia#adding-an-image-source" target="_blank">Read the source guide ↗</a></article></div></div>}
        {view === 'about' && <div className="route-panel narrow"><div className="route-heading"><CircleHelp/><div><span className="eyebrow">OPEN SCIENCE INFRASTRUCTURE</span><h2>About the data center &amp; credits</h2></div></div><p>GAIA maps calibrated auroral images onto a 100 km emission shell.</p><h3>Authors</h3><p>Juha Vierinen and Björn Gustavsson · UiT The Arctic University of Norway</p><p>Image copyrights remain with their producers. GAIA does not transfer ownership or replace the producer’s terms.</p><Credits/></div>}
        <div className="source-strip"><div><Database size={15} /><span><strong>{timeMinutes===1440?'Live frames':'Archived frames'}</strong> {framesLoading?'Loading…':'Selected at or before timeline time; gaps over 10 min omitted'}</span></div><div className="legend"><span className="dot live" /> clear <span className="dot delay" /> delayed <span className="dot idle" /> unavailable</div></div>
      </section>
    </section>
    <footer><img src="uit-logo-white.png" alt="UiT The Arctic University of Norway" /><span>GAIA Data Center</span><button className="suggest-link" onClick={()=>{setSuggesting(true);setSent(false)}}><Lightbulb size={14}/> Suggest an improvement</button><span className="authors">Juha Vierinen · Björn Gustavsson</span></footer>
    {suggesting&&<div className="modal-backdrop" role="presentation"><form className="suggestion-box" onSubmit={submitSuggestion}><button type="button" className="close" onClick={()=>setSuggesting(false)} aria-label="Close"><X/></button>{sent?<div className="sent"><Send/><h2>Suggestion received</h2><p>Thank you. The GAIA team will review it before deciding what to send to Codex.</p></div>:<><span className="eyebrow">HUMAN-REVIEWED INPUT</span><h2>Suggest an improvement</h2><p>Report a bad image, missing source, calibration issue or viewer idea. Suggestions are stored for manual review; they do not change GAIA automatically.</p><label>Your suggestion<textarea name="suggestion" minLength={4} maxLength={8000} required placeholder="What should we improve?"/></label><div className="form-row"><label>Name <span>optional</span><input name="name"/></label><label>Contact <span>optional</span><input name="contact"/></label></div><button className="send-button" type="submit"><Send size={15}/> Send suggestion</button></>}</form></div>}
    {editingCamera&&<LocationEditor camera={editingCamera} onClose={()=>setEditingCamera(null)} onSaved={saved=>setCameras(rows=>rows.map(row=>row.id===saved.id?saved:row))}/>} 
    {maskingCamera&&<MaskEditor camera={maskingCamera} onClose={()=>setMaskingCamera(null)}/>} 
  </main>;
}
