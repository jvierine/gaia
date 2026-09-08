'use client';

import { Activity, Aperture, CircleHelp, Database, Lightbulb, Radio, Satellite, Send, SlidersHorizontal, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { startGaiaGlobe } from '../src/globe';

type ViewName = 'globe' | 'cameras' | 'status' | 'calibrate' | 'about';
type Camera = {id:string;name:string;producer:string;state:string;timestamp_mode:string;latitude_deg:number|null;longitude_deg:number|null;calibrated:boolean;images_24h:number;message?:string|null};

const fallbackCameras:Camera[]=[
  {id:'starvisor-popovo',name:'STARVISOR Popovo',producer:'Popovo camera operator / STARVISOR',state:'waiting',timestamp_mode:'download_time',latitude_deg:57,longitude_deg:38,calibrated:false,images_24h:0},
  {id:'irf-kiruna-alis',name:'IRF Kiruna all-sky',producer:'Swedish Institute of Space Physics',state:'waiting',timestamp_mode:'archive',latitude_deg:67.84,longitude_deg:20.41,calibrated:false,images_24h:0},
];

function LocationEditor({camera,onClose,onSaved}:{camera:Camera,onClose:()=>void,onSaved:(camera:Camera)=>void}){
  const [lat,setLat]=useState(camera.latitude_deg??0);const[lon,setLon]=useState(camera.longitude_deg??0);const[saving,setSaving]=useState(false);
  const point=(event:React.PointerEvent<HTMLDivElement>)=>{const r=event.currentTarget.getBoundingClientRect();setLon(Math.max(-180,Math.min(180,(event.clientX-r.left)/r.width*360-180)));setLat(Math.max(-90,Math.min(90,90-(event.clientY-r.top)/r.height*180)))};
  async function save(){setSaving(true);try{const response=await fetch(`/gaia/api/sources/${camera.id}/location`,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({latitude_deg:lat,longitude_deg:lon})});if(!response.ok)throw new Error();onSaved({...camera,latitude_deg:lat,longitude_deg:lon});onClose()}catch{alert('The camera service is not available yet. Please try again shortly.')}finally{setSaving(false)}}
  return <div className="modal-backdrop"><div className="location-editor"><button className="close" onClick={onClose} aria-label="Close"><X/></button><span className="eyebrow">CAMERA POSITION</span><h2>{camera.name}</h2><p>Drag the marker or enter exact WGS84 coordinates.</p><div className="location-map" onPointerDown={point} onPointerMove={e=>{if(e.buttons)point(e)}}><span className="map-equator"/><span className="map-meridian"/><i style={{left:`${(lon+180)/360*100}%`,top:`${(90-lat)/180*100}%`}}/></div><div className="form-row"><label>Latitude<input type="number" min="-90" max="90" step="0.0001" value={lat.toFixed(4)} onChange={e=>setLat(Number(e.target.value))}/></label><label>Longitude<input type="number" min="-180" max="180" step="0.0001" value={lon.toFixed(4)} onChange={e=>setLon(Number(e.target.value))}/></label></div><button className="send-button" onClick={save} disabled={saving}>{saving?'Saving…':'Save camera position'}</button></div></div>
}

function Globe() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    try { return startGaiaGlobe(canvas); } catch (error) { console.error('GAIA WebGL failed', error); canvas.classList.add('webgl-failed'); }
  }, []);

  return <div className="globe-stage">
    <canvas ref={canvasRef} className="globe-canvas" aria-label="Interactive WebGL Earth with auroral image coverage" />
    <div className="coverage-readout"><span>67.2N 21.0E</span><strong>100 km emission shell</strong></div>
    <div className="globe-tools" aria-label="Map controls"><button type="button" aria-label="Zoom in">+</button><button type="button" aria-label="Zoom out">−</button><button type="button" aria-label="Reset globe">◎</button></div>
  </div>;
}

export default function Home() {
  const [view, setView] = useState<ViewName>('globe'); const [playing, setPlaying] = useState(false); const [timeMinutes,setTimeMinutes]=useState(1440);const [suggesting,setSuggesting]=useState(false);const [sent,setSent]=useState(false);const[editingCamera,setEditingCamera]=useState<Camera|null>(null);const[cameras,setCameras]=useState<Camera[]>(fallbackCameras);
  useEffect(()=>{void fetch('/gaia/api/sources',{cache:'no-store'}).then(async r=>{if(!r.ok)throw new Error();return await r.json() as Camera[]}).then(rows=>{if(rows.length)setCameras(rows)}).catch(()=>{})},[]);
  useEffect(()=>{if(!playing)return;const timer=window.setInterval(()=>setTimeMinutes(v=>v>=1440?0:v+5),250);return()=>window.clearInterval(timer)},[playing]);
  async function submitSuggestion(e:React.FormEvent<HTMLFormElement>){e.preventDefault();const form=new FormData(e.currentTarget);const body={name:form.get('name')||undefined,contact:form.get('contact')||undefined,suggestion:form.get('suggestion'),page_url:location.href};try{const response=await fetch('/gaia/api/suggestions',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});if(!response.ok)throw new Error();setSent(true)}catch{alert('The suggestion service is not available yet. Please try again shortly.')}}
  return <main className="app-shell">
    <header className="topbar">
      <button className="brand" onClick={() => setView('globe')} aria-label="GAIA home"><span className="brand-mark"><i /><i /><i /></span><span><strong>GAIA</strong><small>Global Auroral Image Archive</small></span></button>
      <nav aria-label="Main navigation">
        <button className={view === 'globe' ? 'active' : ''} onClick={() => setView('globe')}>Globe</button>
        <button className={view === 'cameras' ? 'active' : ''} onClick={() => setView('cameras')}>Cameras</button>
        <button className={view === 'status' ? 'active' : ''} onClick={() => setView('status')}>Data status</button>
        <button className={view === 'calibrate' ? 'active' : ''} onClick={() => setView('calibrate')}>Contribute</button>
        <button className={view === 'about' ? 'active' : ''} onClick={() => setView('about')}>About</button>
      </nav>
      <div className="live-pill"><span /> LIVE · 12:42 UTC</div>
    </header>
    <section className="workspace">
      <aside className="control-panel">
        <div className="eyebrow"><Radio size={14} /> GLOBAL MOSAIC</div><h1>Aurora now</h1><p className="timestamp">08 September 2026 · 12:42:00 UTC</p>
        <div className="metric-grid"><div><strong>18</strong><span>active imagers</span></div><div><strong>94%</strong><span>clear-sky coverage</span></div></div>
        <label className="control-label">RENDERING</label>
        <button className="select-button"><span><Aperture size={16} /> Equalized mosaic</span><span>⌄</span></button>
        <button className="select-button"><span><Satellite size={16} /> Magnetic-zenith priority</span><span>⌄</span></button>
        <div className="quality-card"><div><span className="quality-dot" /><strong>Quality mask active</strong></div><p>Cloud, moon, trees and structures are down-weighted before tessellation.</p></div>
        <button className="settings-button"><SlidersHorizontal size={16} /> Display settings</button>
      </aside>
      <section className="viewer">
        {view === 'globe' && <Globe />}
        {view === 'globe' && <div className="timeline-dock"><button onClick={()=>setPlaying(!playing)} aria-label={playing?'Pause 24 hour playback':'Play last 24 hours'}>{playing?'Ⅱ':'▶'}</button><div><div className="timeline-label"><strong>LAST 24 HOURS</strong><time>{timeMinutes===1440?'NOW':`${Math.floor((1440-timeMinutes)/60)}h ${(1440-timeMinutes)%60}m ago`}</time></div><input type="range" min="0" max="1440" step="1" value={timeMinutes} onChange={e=>{setTimeMinutes(Number(e.target.value));setPlaying(false)}}/><div className="timeline-ticks"><span>−24 h</span><span>−18 h</span><span>−12 h</span><span>−6 h</span><span>now</span></div></div></div>}
        {view === 'cameras' && <div className="route-panel"><div className="route-heading"><Satellite/><div><span className="eyebrow">CAMERA REGISTRY</span><h2>{cameras.length} image sources</h2></div></div><p className="panel-intro">Red cameras can be ingested and credited, but cannot enter the 100 km mosaic until their lens model is fitted.</p><div className="camera-list">{cameras.map(camera=><article key={camera.id}><div className="camera-identity"><span className="camera-icon">◉</span><div><strong>{camera.name}</strong><small>{camera.producer}</small></div></div><div className="camera-meta"><span>{camera.latitude_deg==null||camera.longitude_deg==null?'Location needed':`${Math.abs(camera.latitude_deg).toFixed(2)}°${camera.latitude_deg>=0?'N':'S'} · ${Math.abs(camera.longitude_deg).toFixed(2)}°${camera.longitude_deg>=0?'E':'W'}`}</span><small>{camera.timestamp_mode==='archive'?'Archive timestamp':'Download timestamp'} · {camera.images_24h} images / 24 h</small><button type="button" onClick={()=>setEditingCamera(camera)}>Adjust location</button></div><div className={`calibration-state ${camera.calibrated?'calibrated':''}`}><b>{camera.calibrated?'CALIBRATED':'NOT CALIBRATED'}</b>{!camera.calibrated&&<a href={`https://juha.no/aida/?gaia=1&source_id=${encodeURIComponent(camera.id)}&return=https%3A%2F%2Fjuha.no%2Fgaia%2F`} target="_blank">Calibrate in AIDA ↗</a>}</div></article>)}</div></div>}
        {view === 'status' && <div className="route-panel"><div className="route-heading"><Activity/><div><span className="eyebrow">INGESTION PIPELINE</span><h2>Data flow status</h2></div></div><div className="pipeline">{['Acquire','Calibrate','Mask','Project','Tessellate'].map((x,i)=><div key={x}><span className={i===0?'running':''}>{i===0?'↻':'·'}</span><strong>{x}</strong><small>{i===0?'polling sources':'framework ready'}</small></div>)}</div><div className="source-list">{cameras.map(camera=><article key={camera.id}><span className={`source-state ${camera.state}`}/><div><strong>{camera.name}</strong><small>{camera.producer}</small></div><div><b>{camera.timestamp_mode==='archive'?'Archive time':'Download time'}</b><small>{camera.message||`${camera.images_24h} images in 24 h`}</small></div></article>)}</div></div>}
        {view === 'calibrate' && <div className="route-panel narrow"><div className="route-heading"><Aperture/><div><span className="eyebrow">ADD COVERAGE</span><h2>Contribute an imager</h2></div></div><p>Every image retains the producer’s name, institution, copyright and requested acknowledgement. Fixed cameras normally reuse a seasonal calibration; phone images are calibrated individually.</p><div className="choice-grid"><article><span>01</span><h3>Calibrate the image</h3><p>Use AIDA/WISC to match stars and fit the lens. GAIA accepts its native calibration HDF5.</p><a href="https://juha.no/aida/?gaia=1" target="_blank">Open AIDA calibrator ↗</a></article><article><span>02</span><h3>Register a source</h3><p>Add a crawler JSON entry with cadence, timestamp policy, station location, producer and copyright.</p><a href="https://github.com/jvierine/gaia#adding-an-image-source" target="_blank">Read the source guide ↗</a></article></div></div>}
        {view === 'about' && <div className="route-panel narrow"><div className="route-heading"><CircleHelp/><div><span className="eyebrow">OPEN SCIENCE INFRASTRUCTURE</span><h2>About the data center</h2></div></div><p>GAIA maps calibrated auroral images onto a 100 km emission shell. Overlap is tessellated with preference for the view closest to magnetic zenith, after robust dynamic-range matching and cloud/obstruction masking.</p><h3>Authors</h3><p>Juha Vierinen and Björn Gustavsson · UiT The Arctic University of Norway</p><h3>Data acknowledgements</h3><p>Image copyrights remain with their producers. Every displayed frame and mosaic carries source-level attribution; GAIA does not transfer ownership or replace the producer’s terms.</p></div>}
        <div className="source-strip"><div><Database size={15} /><span><strong>Latest composite</strong> 31 seconds ago</span></div><div className="legend"><span className="dot live" /> clear <span className="dot delay" /> delayed <span className="dot idle" /> unavailable</div></div>
      </section>
    </section>
    <footer><img src="uit-logo-white.png" alt="UiT The Arctic University of Norway" /><span>GAIA Data Center</span><button className="suggest-link" onClick={()=>{setSuggesting(true);setSent(false)}}><Lightbulb size={14}/> Suggest an improvement</button><span className="authors">Juha Vierinen · Björn Gustavsson</span></footer>
    {suggesting&&<div className="modal-backdrop" role="presentation"><form className="suggestion-box" onSubmit={submitSuggestion}><button type="button" className="close" onClick={()=>setSuggesting(false)} aria-label="Close"><X/></button>{sent?<div className="sent"><Send/><h2>Suggestion received</h2><p>Thank you. The GAIA team will review it before deciding what to send to Codex.</p></div>:<><span className="eyebrow">HUMAN-REVIEWED INPUT</span><h2>Suggest an improvement</h2><p>Report a bad image, missing source, calibration issue or viewer idea. Suggestions are stored for manual review; they do not change GAIA automatically.</p><label>Your suggestion<textarea name="suggestion" minLength={4} maxLength={8000} required placeholder="What should we improve?"/></label><div className="form-row"><label>Name <span>optional</span><input name="name"/></label><label>Contact <span>optional</span><input name="contact"/></label></div><button className="send-button" type="submit"><Send size={15}/> Send suggestion</button></>}</form></div>}
    {editingCamera&&<LocationEditor camera={editingCamera} onClose={()=>setEditingCamera(null)} onSaved={saved=>setCameras(rows=>rows.map(row=>row.id===saved.id?saved:row))}/>} 
  </main>;
}
