import GoogleAccess,{AccountInfo} from './GoogleAccess';
import {liveCutoff,readyFrames,LIVE_DELAY_MINUTES} from './live-time';
import React,{useEffect,useRef,useState} from 'react';
import './public-viewer.css';
import GaiaLogo from './GaiaLogo';
import GaiaGlobeView from './GaiaGlobeView';
import {manifestUrl,archiveMode} from './public-manifest';

type Camera={source_id:string;name:string;producer:string;institution:string;website_url:string;latitude_deg:number|null;longitude_deg:number|null;acknowledgement:string;copyright:string;calibrated:boolean;map_index:number|null};
type Manifest={generated_utc:string;images:{at:string}[];cameras?:Camera[];overview?:{images:{at:string}[]}};
const location=(camera:Camera)=>camera.latitude_deg==null||camera.longitude_deg==null?'Location unavailable':`${Math.abs(camera.latitude_deg).toFixed(3)}°${camera.latitude_deg>=0?'N':'S'}, ${Math.abs(camera.longitude_deg).toFixed(3)}°${camera.longitude_deg>=0?'E':'W'}`;

export default function PublicViewer(){return <GoogleAccess><PublicContent/></GoogleAccess>}
function PublicContent(){
  const epoch=useRef(liveCutoff()),loading=useRef(false),selectedAt=useRef<number|null>(null);
  const [speed,setSpeed]=useState(32);
  useEffect(()=>{const pause=()=>setPlay(false);window.addEventListener('gaia-pause-playback',pause);return()=>window.removeEventListener('gaia-pause-playback',pause)},[]);
  const [manifest,setManifest]=useState<Manifest>(),[index,setIndex]=useState(-1),[play,setPlay]=useState(false),[credits,setCredits]=useState(false),[error,setError]=useState(''),[sunLock,setSunLock]=useState(false);
  useEffect(()=>{const controller=new AbortController();const refresh=()=>selectedAt.current!==null?Promise.resolve():fetch(manifestUrl,{cache:'no-store',signal:controller.signal}).then(r=>{if(!r.ok)throw Error('Published images unavailable');return r.json()}).then(m=>{const images=readyFrames(m.images);if(selectedAt.current!==null){const at=selectedAt.current;const next=images.findIndex(f=>Date.parse(f.at)>=at);setIndex(next<0?Math.max(0,images.length-1):next)}setManifest({...m,images});setError('')}).catch(e=>{if(!controller.signal.aborted)setError(String(e.message))});void refresh();const timer=setInterval(refresh,60000);return()=>{controller.abort();clearInterval(timer)}},[]);
  useEffect(()=>{if(manifest?.images.length)epoch.current=Date.parse(manifest.images[index<0?manifest.images.length-1:Math.min(index,manifest.images.length-1)].at)},[manifest,index]);
  useEffect(()=>{window.dispatchEvent(new CustomEvent('gaia-run-overview',{detail:{active:play,speed}}));return()=>{window.dispatchEvent(new CustomEvent('gaia-run-overview',{detail:{active:false}}))}},[play,speed]);
  useEffect(()=>{const move=(event:Event)=>{const t=(event as CustomEvent).detail;epoch.current=t;setIndex(Math.max(0,(manifest?.images||[]).findIndex(f=>Date.parse(f.at)>=t)))};window.addEventListener('gaia-overview-epoch',move);return()=>window.removeEventListener('gaia-overview-epoch',move)},[manifest]);
  const frames=manifest?.images||[],at=frames[index<0?frames.length-1:Math.min(index,frames.length-1)]?.at;
  selectedAt.current=index<0?null:(at?Date.parse(at):null);
  return <main className="gaia-public">
    <header className="public-header"><div className="public-brand"><GaiaLogo/><div><h1>GAIA <span>Data Center</span></h1><p>Global Auroral Image Archive</p></div></div><button aria-expanded={credits} onClick={()=>setCredits(!credits)}>ⓘ <span>Info &amp; credits</span></button></header>
    <GaiaGlobeView className="public-globe" getEpochMillis={()=>epoch.current} onLoading={value=>{loading.current=value}} sunLock={sunLock}>
    {credits&&<section className="public-info" role="dialog" aria-label="Information and credits">
      <div className="public-info-heading"><h2>About GAIA</h2><button onClick={()=>setCredits(false)} aria-label="Close information">✕</button></div>
      <p>GAIA compiles low-resolution, geographically projected views of aurora from publicly available data sources. GAIA <strong>does not provide or redistribute the original high-resolution camera images</strong>. Obtain originals directly from the originating provider using the camera links below or by clicking projected imagery on the globe.</p>
      <p>Hover over a projected image to see its institution and station location. Click or tap it to open the originating provider. Gold dots mark contributing camera stations.</p>
      <p>GAIA was inspired in part by the <a href="https://data.phys.ucalgary.ca/" target="_blank" rel="noreferrer">University of Calgary THEMIS All-Sky Imager array</a>, whose continent-scale observations have been enormously influential in auroral research. GAIA is an independent project.</p>
      <p>Drag to rotate the globe. Scroll or pinch to zoom. Press Play to explore recent auroral activity, or Latest to see the newest ready images. Live viewing uses a {LIVE_DELAY_MINUTES}-minute delay to allow cameras to arrive and processing to finish. Tick <strong>Sun up</strong> to hold the sun upwards while Earth rotates. Drag vertically to adjust the viewing tilt.</p>
      <AccountInfo/>
      <h2>GAIA aggregation and projection service</h2>
      <p>GAIA is an independent image aggregation and geographic projection service developed by Juha Vierinen and Björn Gustavsson. Contributing camera networks remain independently operated, and their images remain the copyright of their respective producers.</p>
      <h2>Camera providers &amp; credits</h2>
      <p>Every listed camera links to its originating provider. All images remain the copyright of their respective producers.</p>
      <div className="camera-credits">{manifest?.cameras?.map(camera=><details key={camera.source_id}><summary><a href={camera.website_url} target="_blank" rel="noreferrer">{camera.name} ↗</a></summary><p><strong>{camera.institution}</strong><br/>{location(camera)}<br/>{camera.acknowledgement}<br/>{camera.copyright}</p></details>)}</div>
    </section>}</GaiaGlobeView>
    <section className="public-playback" aria-label="Aurora playback"><div className="public-playback-row"><button className="public-play" aria-label={play?'Pause playback':'Play 24-hour overview'} disabled={!frames.length} onClick={()=>{if(!play&&index<0)setIndex(0);setPlay(!play)}}>{play?'❚❚':'▶'}</button><label>Speed <select aria-label="Playback speed" value={speed} onChange={e=>setSpeed(Number(e.target.value))}>{[1,2,4,8,16,32].map(v=><option key={v} value={v}>{v}×</option>)}</select></label><label className="sun-lock" title="Hold the sun-earth line fixed with the sun upwards, drag vertically to tilt towards the nightside"><input type="checkbox" checked={sunLock} onChange={event=>setSunLock(event.target.checked)}/><span><strong>Sun up</strong><small>rotate vs. sun–earth line</small></span></label><button disabled={!frames.length} onClick={()=>{setPlay(false);window.dispatchEvent(new Event('gaia-leave-overview'));setIndex(-1)}} title={`Newest published frame at least ${LIVE_DELAY_MINUTES} minutes behind now`}>Latest (−{LIVE_DELAY_MINUTES} min)</button><time>{at?new Date(at).toISOString().replace('T',' ').replace('.000Z',' UTC'):'Waiting for publication'}</time></div><input aria-label="Observation time" disabled={!frames.length} type="range" min={0} max={Math.max(0,frames.length-1)} value={index<0?Math.max(0,frames.length-1):Math.min(index,frames.length-1)} onPointerDown={()=>setPlay(false)} onChange={e=>{setPlay(false);const wanted=Date.parse(frames[Number(e.target.value)].at);const samples=manifest?.overview?.images||[];const t=samples.length?samples.reduce((a,b)=>Math.abs(Date.parse(a.at)-wanted)<Math.abs(Date.parse(b.at)-wanted)?a:b).at:frames[Number(e.target.value)].at;epoch.current=Date.parse(t);setIndex(Math.max(0,frames.findIndex(f=>Date.parse(f.at)>=Date.parse(t))))}}/><div className="public-timeline-labels"><span>{frames.length?new Date(frames[0].at).toISOString().slice(0,16).replace('T',' '):'No frames'} UTC</span><span>{frames.length} overview samples · 12-minute spacing · {archiveMode?"full archive":"last 24 hours"}</span></div>{error&&<span role="alert">{error}</span>}</section>
  </main>
}
